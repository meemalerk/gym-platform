//! Billing use-cases — the gym→member side (ADR-0010).
//!
//! Authority here is simpler than coaching and stricter than goals: **money is
//! managed by managers**. Owners and admins write plans, subscriptions,
//! invoices and payments; a head coach or trainer has no business setting
//! prices, and a member may read only their own subscription and invoices.
//!
//! Nothing in this service talks to a payment provider. `Payment` records what
//! a gym says it received; Stripe arrives later behind the same seam
//! (`PaymentProvider::Stripe` plus a provider reference), and when it does, the
//! shape of everything here is unchanged.

use std::sync::Arc;

use chrono::{Datelike, NaiveDate};
use gym_domain::{
    GymId, InvoiceId, MembershipPlanId, TenantContext, UserId,
    billing::{
        BillingInterval, Invoice, InvoiceStatus, MemberSubscription, MembershipPlan, Payment,
        PaymentProvider,
    },
    entitlement::Feature,
    ids::{PaymentId, SubscriptionId},
};
use uuid::Uuid;

use crate::{
    ApplicationError, ApplicationResult,
    ports::{
        BillingRepository, CheckoutSession, CheckoutSessionRequest, Clock, InvoiceView,
        PaymentGateway, PlanView, SubscriptionView, UserRepository,
    },
};

#[derive(Debug, Clone)]
pub struct CreatePlanCommand {
    pub name: String,
    pub description: Option<String>,
    pub price_minor: i64,
    pub currency: String,
    pub interval: BillingInterval,
    /// What the plan confers. Empty is refused by the domain: a plan that takes
    /// money and grants nothing is the one shape a member would rightly
    /// complain about.
    pub grants: Vec<Feature>,
}

#[derive(Debug, Clone)]
pub struct SubscribeCommand {
    pub member_id: UserId,
    pub plan_id: MembershipPlanId,
    pub started_on: NaiveDate,
}

#[derive(Debug, Clone)]
pub struct IssueInvoiceCommand {
    pub member_id: UserId,
    pub subscription_id: Option<SubscriptionId>,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
    pub period: Option<(NaiveDate, NaiveDate)>,
    pub due_on: NaiveDate,
}

#[derive(Debug, Clone)]
pub struct RecordPaymentCommand {
    pub invoice_id: InvoiceId,
    pub amount_minor: i64,
    pub provider: PaymentProvider,
    pub provider_ref: Option<String>,
    pub received_on: NaiveDate,
    pub note: Option<String>,
}

/// A payment a processor has confirmed.
///
/// A struct rather than eight positional arguments: the two `Uuid`-shaped ids
/// and the amount are exactly the kind of thing that gets transposed at a call
/// site, and the compiler cannot see it. Named fields make a swapped
/// `member_id`/`invoice_id` a compile error instead of a misapplied payment.
#[derive(Debug, Clone)]
pub struct GatewayPaymentCommand {
    pub gym_id: GymId,
    pub member_id: UserId,
    pub invoice_id: InvoiceId,
    pub amount_minor: i64,
    pub provider: PaymentProvider,
    /// The gateway's reference for this attempt — the idempotency key.
    pub session_id: String,
    pub received_on: NaiveDate,
}

#[derive(Clone)]
pub struct BillingService {
    pub billing: Arc<dyn BillingRepository>,
    pub users: Arc<dyn UserRepository>,
    pub clock: Arc<dyn Clock>,
    /// The seam (ADR-0010). A deployment with no processor configured still
    /// gets a working `BillingService` — `create_checkout_session` just fails
    /// with `ApplicationError::Unavailable` — via a null implementation
    /// (`gym_infrastructure::stripe::NotConfigured`).
    pub gateway: Arc<dyn PaymentGateway>,
}

impl BillingService {
    fn ensure_manager(&self, tenant: &TenantContext) -> ApplicationResult<()> {
        if tenant.capabilities.can_manage_gym() {
            Ok(())
        } else {
            Err(ApplicationError::Forbidden)
        }
    }

    // --------------------------------------------------------------- plans

    /// Plans a gym offers. Readable by anyone in the gym: a member choosing
    /// what to join needs to see the prices.
    pub async fn list_plans(&self, tenant: &TenantContext) -> ApplicationResult<Vec<PlanView>> {
        self.billing.list_plans(tenant).await
    }

    pub async fn create_plan(
        &self,
        tenant: &TenantContext,
        cmd: CreatePlanCommand,
    ) -> ApplicationResult<MembershipPlan> {
        self.ensure_manager(tenant)?;

        let plan = MembershipPlan::new(
            MembershipPlanId::from(Uuid::now_v7()),
            tenant.gym_id,
            &cmd.name,
            cmd.description,
            cmd.price_minor,
            &cmd.currency,
            cmd.interval,
            cmd.grants,
        )?;

        self.billing.insert_plan(tenant, &plan).await?;
        Ok(plan)
    }

    /// Stop offering a plan. Existing subscribers keep their terms — this is
    /// why archiving exists instead of deleting.
    pub async fn archive_plan(
        &self,
        tenant: &TenantContext,
        id: MembershipPlanId,
    ) -> ApplicationResult<()> {
        self.ensure_manager(tenant)?;

        self.billing
            .find_plan(tenant, id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "plan" })?;

        if self.billing.archive_plan(tenant, id).await? {
            Ok(())
        } else {
            Err(ApplicationError::Conflict("this plan".to_owned()))
        }
    }

    // -------------------------------------------------------- subscriptions

    /// Subscriptions the caller may see: the gym's for a manager, their own
    /// for everyone else.
    pub async fn list_subscriptions(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<SubscriptionView>> {
        let all = self.billing.list_subscriptions(tenant).await?;
        if tenant.capabilities.can_manage_gym() {
            return Ok(all);
        }
        Ok(all
            .into_iter()
            .filter(|v| v.subscription.member_id == tenant.actor_id)
            .collect())
    }

    /// Put a member on a plan, and issue the first invoice in the same breath —
    /// a subscription that owes nothing yet is the state gyms forget to bill.
    ///
    /// **Two callers, one rule** (ADR-0031). A manager may subscribe anybody.
    /// A member may subscribe *themselves*, to a plan that is *on sale*, and
    /// to nothing else — which is what turns "you cannot start a workout,
    /// these plans would let you" from a dead end into a sentence with a
    /// button under it.
    ///
    /// Self-service is not a loosening of the billing model: the plan's price
    /// and grants are the gym's, copied at signup exactly as before, and the
    /// invoice is issued the same way. The only thing the member chooses is
    /// which of the gym's own offers to take.
    pub async fn subscribe(
        &self,
        tenant: &TenantContext,
        cmd: SubscribeCommand,
    ) -> ApplicationResult<(MemberSubscription, Invoice)> {
        let for_themselves = cmd.member_id == tenant.actor_id;
        if !for_themselves {
            self.ensure_manager(tenant)?;
        }

        let plan = self
            .billing
            .find_plan(tenant, cmd.plan_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "plan" })?;

        // An archived plan is one the gym has stopped selling. A manager may
        // still put somebody on it — that is how you honour a price you
        // promised — but nobody may sign *themselves* up to something that is
        // no longer offered.
        if for_themselves && !plan.is_offered() {
            return Err(ApplicationError::NotFound { entity: "plan" });
        }

        // Nor may they start a second one while the first is running: two
        // active subscriptions is two invoices a month and one very reasonable
        // complaint.
        if for_themselves
            && self
                .billing
                .list_subscriptions(tenant)
                .await?
                .iter()
                .any(|v| {
                    v.subscription.member_id == tenant.actor_id && v.subscription.status.is_active()
                })
        {
            return Err(ApplicationError::Conflict(
                "you are already on a plan".to_owned(),
            ));
        }

        // A member must actually belong to this gym. Without the check a
        // manager could bill a stranger by pasting a user id. Holding no
        // capacities here IS "not a member" (ADR-0014).
        if self
            .users
            .capabilities_in(cmd.member_id, tenant.gym_id)
            .await?
            .is_empty()
        {
            return Err(ApplicationError::NotFound { entity: "member" });
        }

        let subscription = MemberSubscription::start(
            SubscriptionId::from(Uuid::now_v7()),
            tenant.gym_id,
            cmd.member_id,
            &plan,
            cmd.started_on,
        )?;

        self.billing
            .insert_subscription(tenant, &subscription)
            .await?;

        let period = match plan.interval {
            BillingInterval::Monthly => subscription
                .next_charge_on
                .map(|next| (cmd.started_on, next.pred_opt().unwrap_or(next))),
            BillingInterval::Once => None,
        };

        let invoice = self
            .issue_invoice_inner(
                tenant,
                IssueInvoiceCommand {
                    member_id: cmd.member_id,
                    subscription_id: Some(subscription.id),
                    description: format!("{} · {}", plan.name, month_label(cmd.started_on)),
                    amount_minor: subscription.price_minor,
                    currency: subscription.currency.clone(),
                    period,
                    // Due the day the membership starts: a gym takes the first
                    // month up front. (`issue_invoice_inner` floors this at
                    // today, so a back-dated start cannot arrive already late.)
                    due_on: cmd.started_on,
                },
            )
            .await?;

        Ok((subscription, invoice))
    }

    /// Cancel a subscription. Access runs to the end of the period already
    /// paid for; billing stops immediately.
    ///
    /// Now that the nightly tick actually recurs, this is not a nicety: a
    /// subscription with no way to stop it bills forever. `next_charge_on` is
    /// cleared as part of the cancellation, which is what takes it out of the
    /// tick's query — the status alone would be enough, but relying on one
    /// condition where two are free is how a filter change becomes a refund.
    pub async fn cancel_subscription(
        &self,
        tenant: &TenantContext,
        id: SubscriptionId,
    ) -> ApplicationResult<MemberSubscription> {
        let mut subscription = self.billing.find_subscription(tenant, id).await?.ok_or(
            ApplicationError::NotFound {
                entity: "subscription",
            },
        )?;

        // A member may end their OWN subscription, any time, without asking.
        //
        // This used to be managers-only, which left the app able to sell a
        // membership self-service but not to stop one: the only way off a plan
        // was to catch a manager. That is not a billing safeguard, it is a
        // retention tactic enforced by a missing button, and it is the reason
        // people distrust gym contracts in the first place.
        //
        // Nothing about the money changes — access still runs to the end of the
        // period already paid for, computed identically below, and no refund is
        // invented. Cancelling is also not leaving the gym: the membership
        // capacity is untouched, so somebody dropping a Coaching plan stays a
        // member and can pick up a solo plan (see `subscribe`, which members
        // may already call for themselves). That "coached -> solo" step is
        // exactly this call followed by that one.
        //
        // Not-found rather than forbidden for somebody else's subscription:
        // whether a given id exists is not a thing to confirm to a stranger.
        if !tenant.capabilities.can_manage_gym() && subscription.member_id != tenant.actor_id {
            return Err(ApplicationError::NotFound {
                entity: "subscription",
            });
        }

        let now = self.clock.now();

        // Access to the end of what they have paid for. `next_charge_on` is
        // the start of the period they have NOT paid for, so the day before it
        // is the last covered day. With no next charge — a one-off — access
        // ends today, because there is no paid period running.
        let ends_on = subscription
            .next_charge_on
            .and_then(|next| next.pred_opt())
            .unwrap_or_else(|| now.date_naive());

        subscription.cancel(now, ends_on)?;

        self.billing
            .save_cancelled_subscription(tenant, &subscription)
            .await?;

        Ok(subscription)
    }

    // ------------------------------------------------------------- invoices

    /// Invoices the caller may see: the gym's for a manager, their own
    /// otherwise. A member seeing another member's invoice would be a leak of
    /// both money and membership.
    pub async fn list_invoices(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<InvoiceView>> {
        let all = self.billing.list_invoices(tenant).await?;
        if tenant.capabilities.can_manage_gym() {
            return Ok(all);
        }
        Ok(all
            .into_iter()
            .filter(|v| v.invoice.member_id == tenant.actor_id)
            .collect())
    }

    pub async fn issue_invoice(
        &self,
        tenant: &TenantContext,
        cmd: IssueInvoiceCommand,
    ) -> ApplicationResult<Invoice> {
        self.ensure_manager(tenant)?;
        self.issue_invoice_inner(tenant, cmd).await
    }

    async fn issue_invoice_inner(
        &self,
        tenant: &TenantContext,
        cmd: IssueInvoiceCommand,
    ) -> ApplicationResult<Invoice> {
        let today = self.clock.now().date_naive();
        let number = self
            .billing
            .allocate_invoice_number(tenant, today.year())
            .await?;
        let reference = format!("INV-{}-{number:04}", today.year());

        let invoice = Invoice::issue(
            InvoiceId::from(Uuid::now_v7()),
            tenant.gym_id,
            cmd.member_id,
            cmd.subscription_id,
            &reference,
            &cmd.description,
            cmd.amount_minor,
            &cmd.currency,
            cmd.period,
            today,
            cmd.due_on.max(today),
        )?;

        self.billing.insert_invoice(tenant, &invoice).await?;
        Ok(invoice)
    }

    /// Void an invoice issued in error. Never a way to fix a paid one — that
    /// is a refund, which is another payment row.
    pub async fn void_invoice(
        &self,
        tenant: &TenantContext,
        id: InvoiceId,
        reason: Option<String>,
    ) -> ApplicationResult<Invoice> {
        self.ensure_manager(tenant)?;

        let mut invoice = self
            .billing
            .find_invoice(tenant, id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "invoice" })?;

        invoice.void(self.clock.now(), tenant.actor_id, reason)?;
        self.billing
            .save_invoice_state(tenant, &invoice, "invoice.voided")
            .await?;
        Ok(invoice)
    }

    // ------------------------------------------------------------- payments

    /// Everything received against one invoice.
    ///
    /// Scoped by the caller: a member may read the payments on their own
    /// invoice — it is their receipt — and nobody else's.
    pub async fn payments_for(
        &self,
        tenant: &TenantContext,
        invoice_id: InvoiceId,
    ) -> ApplicationResult<Vec<Payment>> {
        let invoice = self
            .billing
            .find_invoice(tenant, invoice_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "invoice" })?;

        if !tenant.capabilities.can_manage_gym() && invoice.member_id != tenant.actor_id {
            return Err(ApplicationError::NotFound { entity: "invoice" });
        }

        self.billing.payments_for(tenant, invoice_id).await
    }

    /// Record money received. When it covers the balance the invoice settles
    /// in the same transaction as the payment lands.
    pub async fn record_payment(
        &self,
        tenant: &TenantContext,
        cmd: RecordPaymentCommand,
    ) -> ApplicationResult<Payment> {
        self.ensure_manager(tenant)?;

        let invoice = self
            .billing
            .find_invoice(tenant, cmd.invoice_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "invoice" })?;

        let payment = Payment::record(
            PaymentId::from(Uuid::now_v7()),
            tenant.gym_id,
            &invoice,
            cmd.amount_minor,
            cmd.provider,
            cmd.provider_ref,
            cmd.received_on,
            tenant.actor_id,
            cmd.note,
        )?;

        // Settle only if this payment takes the running total to the amount
        // owed — part-payments are real, and a gym that took £40 of £120 must
        // not see the invoice go quiet.
        let already: i64 = self
            .billing
            .payments_for(tenant, invoice.id)
            .await?
            .iter()
            .map(|p| p.amount_minor)
            .sum();
        let settles = matches!(invoice.status, InvoiceStatus::Due)
            && already + payment.amount_minor >= invoice.amount_minor;

        self.billing
            .insert_payment(tenant, &payment, settles)
            .await?;
        Ok(payment)
    }

    // ------------------------------------------------------------ self-pay

    /// Start a hosted checkout for an invoice's outstanding balance.
    ///
    /// The payer themselves, or a manager on their behalf, may call this —
    /// same "own invoice or manage the gym" rule as reading one
    /// (`list_invoices`). Nothing about the invoice changes yet: that only
    /// happens once the processor confirms payment, via
    /// `apply_gateway_payment`, never from this call directly.
    pub async fn create_checkout_session(
        &self,
        tenant: &TenantContext,
        invoice_id: InvoiceId,
        success_url: String,
        cancel_url: String,
    ) -> ApplicationResult<CheckoutSession> {
        let invoice = self
            .billing
            .find_invoice(tenant, invoice_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "invoice" })?;

        // Same visibility rule as list_invoices: a member may act on their own
        // invoice, nobody else's — and returning NotFound rather than Forbidden
        // avoids confirming the invoice exists to someone it is not theirs.
        if !tenant.capabilities.can_manage_gym() && invoice.member_id != tenant.actor_id {
            return Err(ApplicationError::NotFound { entity: "invoice" });
        }

        if !matches!(invoice.status, InvoiceStatus::Due) {
            return Err(ApplicationError::Conflict(
                "this invoice is not open for payment".to_owned(),
            ));
        }

        let already: i64 = self
            .billing
            .payments_for(tenant, invoice.id)
            .await?
            .iter()
            .map(|p| p.amount_minor)
            .sum();
        let outstanding = invoice.amount_minor - already;
        if outstanding <= 0 {
            return Err(ApplicationError::Conflict(
                "this invoice is already settled".to_owned(),
            ));
        }

        self.gateway
            .create_checkout_session(CheckoutSessionRequest {
                gym_id: tenant.gym_id,
                invoice_id: invoice.id,
                member_id: invoice.member_id,
                amount_minor: outstanding,
                currency: invoice.currency.clone(),
                description: invoice.description.clone(),
                success_url,
                cancel_url,
            })
            .await
    }

    /// Apply a payment the processor has confirmed.
    ///
    /// Reachable only from a route that has already established trust — a
    /// signature-verified Stripe webhook, or the self-hosted card page holding
    /// a signed session token. There is deliberately no route a signed-in user
    /// can call this through, and it deliberately skips `ensure_manager`:
    /// trust comes from the processor, not from `tenant.capabilities`. A member
    /// paying their own invoice by card is not "recording a payment" the way a
    /// manager taking cash is.
    ///
    ///
    /// One path for every processor. Stripe's webhook and the self-hosted card
    /// page both arrive here, so the idempotency rule, the part-payment
    /// arithmetic and the settle-in-the-same-transaction guarantee are written
    /// once and cannot drift between them.
    ///
    /// Deliberately takes no `TenantContext`: the caller is a processor, not a
    /// signed-in person. The member's real capacities are read from the
    /// database to build one, so nothing downstream has to special-case a
    /// request that arrived without a bearer token.
    pub async fn apply_gateway_payment(&self, cmd: GatewayPaymentCommand) -> ApplicationResult<()> {
        let GatewayPaymentCommand {
            gym_id,
            member_id,
            invoice_id,
            amount_minor,
            provider,
            session_id,
            received_on,
        } = cmd;

        let capabilities = self.users.capabilities_in(member_id, gym_id).await?;
        let tenant = TenantContext {
            gym_id,
            actor_id: member_id,
            capabilities,
        };

        let invoice = self
            .billing
            .find_invoice(&tenant, invoice_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "invoice" })?;

        let existing = self.billing.payments_for(&tenant, invoice.id).await?;

        // Idempotent: Stripe redelivers webhooks and a browser resubmits forms,
        // and neither must double-credit the invoice. The session reference is
        // unique per attempt, so its presence already means this one landed.
        if existing
            .iter()
            .any(|p| p.provider_ref.as_deref() == Some(session_id.as_str()))
        {
            return Ok(());
        }

        let payment = Payment::record(
            PaymentId::from(Uuid::now_v7()),
            gym_id,
            &invoice,
            amount_minor,
            provider,
            Some(session_id),
            received_on,
            member_id,
            Some(match provider {
                PaymentProvider::Stripe => "Paid via Stripe Checkout".to_owned(),
                // Says so plainly, in the money table, forever. A row that
                // cannot be told apart from a real payment is a liability.
                PaymentProvider::Dummy => "Paid via the demo card page (no money moved)".to_owned(),
                other => format!("Paid via {}", other.as_str()),
            }),
        )?;

        let already: i64 = existing.iter().map(|p| p.amount_minor).sum();
        let settles = matches!(invoice.status, InvoiceStatus::Due)
            && already + payment.amount_minor >= invoice.amount_minor;

        self.billing
            .insert_payment(&tenant, &payment, settles)
            .await
    }
}

/// "July 2026" — what a monthly invoice covers, in words.
fn month_label(date: NaiveDate) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    format!(
        "{} {}",
        MONTHS[(date.month() as usize).saturating_sub(1).min(11)],
        date.year()
    )
}
