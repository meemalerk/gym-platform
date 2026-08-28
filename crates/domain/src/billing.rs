//! Billing — what a gym charges, who is on what, and what was actually owed.
//!
//! Three rules shape everything here.
//!
//! **Money is minor units in an integer.** Never a float: a currency amount
//! that has been through binary floating point is an amount you cannot defend
//! in an argument with a member, and gym billing is nothing but such arguments.
//!
//! **An issued invoice is a statement of fact.** Its terms never change. A
//! correction is a void plus a new invoice, so the trail shows what happened
//! rather than what someone wishes had happened. Enforced twice, in the
//! lifecycle here and by a database trigger.
//!
//! **"Overdue" is not a state, it is a date passing.** It is `Due` and past its
//! due date, derived on read. Storing it would need a nightly job to keep it
//! true, and that job is a second source of truth that will disagree.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    DomainError,
    entitlement::Feature,
    ids::{GymId, InvoiceId, MembershipPlanId, PaymentId, SubscriptionId, UserId},
};

/// Longest a plan name can be before it stops fitting a row on a phone.
const MAX_NAME: usize = 80;
/// £10,000 a month in minor units. A ceiling that catches a slipped decimal
/// without arguing with a genuinely expensive private-coaching package.
const MAX_PRICE_MINOR: i64 = 1_000_000;

fn currency(code: &str) -> Result<String, DomainError> {
    let upper = code.trim().to_uppercase();
    if upper.len() != 3 || !upper.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(DomainError::Invalid(
            "currency must be a three-letter code, e.g. GBP".to_owned(),
        ));
    }
    Ok(upper)
}

fn price(minor: i64) -> Result<i64, DomainError> {
    if !(0..=MAX_PRICE_MINOR).contains(&minor) {
        return Err(DomainError::Invalid(format!(
            "price must be between 0 and {MAX_PRICE_MINOR} minor units"
        )));
    }
    Ok(minor)
}

fn name(raw: &str) -> Result<String, DomainError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(DomainError::Empty { field: "name" });
    }
    if trimmed.chars().count() > MAX_NAME {
        return Err(DomainError::Invalid(format!(
            "name must be {MAX_NAME} characters or fewer"
        )));
    }
    Ok(trimmed.to_owned())
}

/// How often a plan is charged. `Once` is a drop-in, not a subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BillingInterval {
    Monthly,
    Once,
}

impl BillingInterval {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Monthly => "monthly",
            Self::Once => "once",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "monthly" => Ok(Self::Monthly),
            "once" => Ok(Self::Once),
            other => Err(DomainError::Invalid(format!("unknown interval {other}"))),
        }
    }

    /// When the next charge falls due after `from`. `Once` never recurs.
    pub fn next_charge_after(self, from: NaiveDate) -> Option<NaiveDate> {
        match self {
            Self::Once => None,
            Self::Monthly => from.checked_add_months(chrono::Months::new(1)),
        }
    }
}

/// What a gym sells.
#[derive(Debug, Clone, PartialEq)]
pub struct MembershipPlan {
    pub id: MembershipPlanId,
    pub gym_id: GymId,
    pub name: String,
    pub description: Option<String>,
    pub price_minor: i64,
    pub currency: String,
    pub interval: BillingInterval,
    /// What being on this plan confers. A gym decides what its "Coaching"
    /// includes; the platform only decides which features can be named.
    pub grants: Vec<Feature>,
    /// Archived plans keep their subscribers; they stop being offered.
    pub archived_at: Option<DateTime<Utc>>,
}

impl MembershipPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: MembershipPlanId,
        gym_id: GymId,
        name_raw: &str,
        description: Option<String>,
        price_minor: i64,
        currency_raw: &str,
        interval: BillingInterval,
        grants: Vec<Feature>,
    ) -> Result<Self, DomainError> {
        // A plan that confers nothing is almost certainly a mistake: it takes
        // money and grants no access, which is the one shape a member would
        // rightly complain about.
        if grants.is_empty() {
            return Err(DomainError::Invalid(
                "a plan must grant at least one thing".to_owned(),
            ));
        }

        let mut grants = grants;
        grants.sort();
        grants.dedup();

        Ok(Self {
            id,
            gym_id,
            name: name(name_raw)?,
            description: description.filter(|d| !d.trim().is_empty()),
            price_minor: price(price_minor)?,
            currency: currency(currency_raw)?,
            interval,
            grants,
            archived_at: None,
        })
    }

    pub fn is_offered(&self) -> bool {
        self.archived_at.is_none()
    }
}

/// Whether a member is currently on a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Active,
    /// Carries its evidence: when it was cancelled, and when access actually
    /// ends — which is the end of the period already paid for, not today.
    Cancelled {
        cancelled_at: DateTime<Utc>,
        ends_on: NaiveDate,
    },
}

impl SubscriptionStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// One member on one plan, at the price agreed when they joined it.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberSubscription {
    pub id: SubscriptionId,
    pub gym_id: GymId,
    pub member_id: UserId,
    pub plan_id: MembershipPlanId,
    /// Copied from the plan at signup. A plan's price change must not
    /// re-price the people already on it behind their backs.
    pub price_minor: i64,
    pub currency: String,
    pub status: SubscriptionStatus,
    pub started_on: NaiveDate,
    pub next_charge_on: Option<NaiveDate>,
}

impl MemberSubscription {
    pub fn start(
        id: SubscriptionId,
        gym_id: GymId,
        member_id: UserId,
        plan: &MembershipPlan,
        started_on: NaiveDate,
    ) -> Result<Self, DomainError> {
        if !plan.is_offered() {
            return Err(DomainError::Invalid(
                "that plan is archived and cannot take new members".to_owned(),
            ));
        }
        if plan.gym_id != gym_id {
            return Err(DomainError::Invalid(
                "that plan belongs to another gym".to_owned(),
            ));
        }

        Ok(Self {
            id,
            gym_id,
            member_id,
            plan_id: plan.id,
            price_minor: plan.price_minor,
            currency: plan.currency.clone(),
            status: SubscriptionStatus::Active,
            started_on,
            next_charge_on: plan.interval.next_charge_after(started_on),
        })
    }

    /// Cancel, with access running to the end of the period already paid for.
    pub fn cancel(&mut self, at: DateTime<Utc>, ends_on: NaiveDate) -> Result<(), DomainError> {
        if !self.status.is_active() {
            return Err(DomainError::Invalid(
                "this subscription is already cancelled".to_owned(),
            ));
        }
        self.status = SubscriptionStatus::Cancelled {
            cancelled_at: at,
            ends_on,
        };
        self.next_charge_on = None;
        Ok(())
    }
}

/// Where an invoice is in its life. Terminal states carry their evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum InvoiceStatus {
    /// Issued and unpaid. Whether it is *overdue* depends on today, so it is
    /// asked, never stored — see `InvoiceStatus::is_overdue`.
    Due,
    Paid {
        paid_at: DateTime<Utc>,
    },
    Void {
        voided_at: DateTime<Utc>,
        voided_by: UserId,
        reason: Option<String>,
    },
}

impl InvoiceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Due => "due",
            Self::Paid { .. } => "paid",
            Self::Void { .. } => "void",
        }
    }

    pub fn is_settled(&self) -> bool {
        !matches!(self, Self::Due)
    }
}

/// What was owed, for what, when.
#[derive(Debug, Clone, PartialEq)]
pub struct Invoice {
    pub id: InvoiceId,
    pub gym_id: GymId,
    pub member_id: UserId,
    pub subscription_id: Option<SubscriptionId>,
    pub reference: String,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
    pub period: Option<(NaiveDate, NaiveDate)>,
    pub issued_on: NaiveDate,
    pub due_on: NaiveDate,
    pub status: InvoiceStatus,
}

impl Invoice {
    #[allow(clippy::too_many_arguments)]
    pub fn issue(
        id: InvoiceId,
        gym_id: GymId,
        member_id: UserId,
        subscription_id: Option<SubscriptionId>,
        reference: &str,
        description: &str,
        amount_minor: i64,
        currency_raw: &str,
        period: Option<(NaiveDate, NaiveDate)>,
        issued_on: NaiveDate,
        due_on: NaiveDate,
    ) -> Result<Self, DomainError> {
        if due_on < issued_on {
            return Err(DomainError::Invalid(
                "an invoice cannot fall due before it is issued".to_owned(),
            ));
        }
        if let Some((start, end)) = period
            && end < start
        {
            return Err(DomainError::Invalid(
                "the service period ends before it starts".to_owned(),
            ));
        }

        Ok(Self {
            id,
            gym_id,
            member_id,
            subscription_id,
            reference: name(reference)?,
            description: name(description)?,
            amount_minor: price(amount_minor)?,
            currency: currency(currency_raw)?,
            period,
            issued_on,
            due_on,
            status: InvoiceStatus::Due,
        })
    }

    /// Unpaid and past its date. Derived, because it depends on when you ask.
    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        matches!(self.status, InvoiceStatus::Due) && today > self.due_on
    }

    /// Days late, or zero. Only meaningful while `is_overdue`.
    pub fn days_overdue(&self, today: NaiveDate) -> i64 {
        if self.is_overdue(today) {
            (today - self.due_on).num_days()
        } else {
            0
        }
    }

    pub fn mark_paid(&mut self, at: DateTime<Utc>) -> Result<(), DomainError> {
        match self.status {
            InvoiceStatus::Due => {
                self.status = InvoiceStatus::Paid { paid_at: at };
                Ok(())
            }
            InvoiceStatus::Paid { .. } => Err(DomainError::Invalid(
                "this invoice is already paid".to_owned(),
            )),
            InvoiceStatus::Void { .. } => Err(DomainError::Invalid(
                "a voided invoice cannot be paid".to_owned(),
            )),
        }
    }

    pub fn void(
        &mut self,
        at: DateTime<Utc>,
        by: UserId,
        reason: Option<String>,
    ) -> Result<(), DomainError> {
        match self.status {
            InvoiceStatus::Due => {
                self.status = InvoiceStatus::Void {
                    voided_at: at,
                    voided_by: by,
                    reason: reason.filter(|r| !r.trim().is_empty()),
                };
                Ok(())
            }
            InvoiceStatus::Paid { .. } => Err(DomainError::Invalid(
                "a paid invoice cannot be voided; refund it instead".to_owned(),
            )),
            InvoiceStatus::Void { .. } => Err(DomainError::Invalid(
                "this invoice is already void".to_owned(),
            )),
        }
    }
}

/// How money arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaymentProvider {
    Cash,
    CardTerminal,
    Stripe,
    /// A card page this deployment serves itself, taking no real money.
    ///
    /// Not a mock in the test-double sense: it is a real hosted-checkout
    /// implementation of the same port, with the bank replaced by a rule about
    /// which card numbers succeed. That makes the whole payment path — redirect,
    /// return, settle, idempotency — exercisable without an account anywhere,
    /// which is what a demo and a verification suite both need.
    ///
    /// A payment recorded against it is clearly labelled, because a row in the
    /// money table that cannot be told apart from a real one is a liability.
    Dummy,
}

impl PaymentProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::CardTerminal => "card_terminal",
            Self::Stripe => "stripe",
            Self::Dummy => "dummy",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        match raw {
            "cash" => Ok(Self::Cash),
            "card_terminal" => Ok(Self::CardTerminal),
            "stripe" => Ok(Self::Stripe),
            "dummy" => Ok(Self::Dummy),
            other => Err(DomainError::Invalid(format!("unknown provider {other}"))),
        }
    }
}

/// Money received against an invoice. Append-only by design: a mistake is
/// corrected with another row (a negative amount is a refund), never by
/// editing away the record of what happened.
#[derive(Debug, Clone, PartialEq)]
pub struct Payment {
    pub id: PaymentId,
    pub gym_id: GymId,
    pub invoice_id: InvoiceId,
    pub amount_minor: i64,
    pub currency: String,
    pub provider: PaymentProvider,
    pub provider_ref: Option<String>,
    pub received_on: NaiveDate,
    pub recorded_by: UserId,
    pub note: Option<String>,
}

impl Payment {
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        id: PaymentId,
        gym_id: GymId,
        invoice: &Invoice,
        amount_minor: i64,
        provider: PaymentProvider,
        provider_ref: Option<String>,
        received_on: NaiveDate,
        recorded_by: UserId,
        note: Option<String>,
    ) -> Result<Self, DomainError> {
        if invoice.gym_id != gym_id {
            return Err(DomainError::Invalid(
                "that invoice belongs to another gym".to_owned(),
            ));
        }
        if matches!(invoice.status, InvoiceStatus::Void { .. }) {
            return Err(DomainError::Invalid(
                "a voided invoice cannot take a payment".to_owned(),
            ));
        }
        if amount_minor == 0 {
            return Err(DomainError::Invalid(
                "a payment of nothing is not a payment".to_owned(),
            ));
        }
        // A refund is negative and may exceed nothing; a payment is positive
        // and bounded by the same ceiling as a price.
        if amount_minor > 0 {
            price(amount_minor)?;
        } else if -amount_minor > MAX_PRICE_MINOR {
            return Err(DomainError::Invalid(
                "refund is implausibly large".to_owned(),
            ));
        }

        Ok(Self {
            id,
            gym_id,
            invoice_id: invoice.id,
            amount_minor,
            currency: invoice.currency.clone(),
            provider,
            provider_ref: provider_ref.filter(|r| !r.trim().is_empty()),
            received_on,
            recorded_by,
            note: note.filter(|n| !n.trim().is_empty()),
        })
    }
}

/// Format minor units for display: 12_050 GBP → "£120.50".
///
/// Lives in the domain because "what does this amount say" must match wherever
/// it is written — an invoice, a receipt and a dashboard disagreeing about the
/// same number is exactly the bug this prevents.
pub fn format_money(minor: i64, currency: &str) -> String {
    let symbol = match currency {
        "GBP" => "£",
        "USD" => "$",
        "EUR" => "€",
        _ => "",
    };
    let negative = minor < 0;
    let abs = minor.abs();
    let body = format!("{}{}.{:02}", symbol, abs / 100, abs % 100);
    if negative { format!("-{body}") } else { body }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gym() -> GymId {
        GymId::from(uuid::Uuid::nil())
    }
    fn user() -> UserId {
        UserId::from(uuid::Uuid::nil())
    }
    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }
    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn plan(interval: BillingInterval) -> MembershipPlan {
        MembershipPlan::new(
            MembershipPlanId::from(uuid::Uuid::nil()),
            gym(),
            "Coaching",
            None,
            12_000,
            "gbp",
            interval,
            vec![Feature::GymAccess],
        )
        .unwrap()
    }

    fn invoice() -> Invoice {
        Invoice::issue(
            InvoiceId::from(uuid::Uuid::nil()),
            gym(),
            user(),
            None,
            "INV-2026-0001",
            "Coaching membership · July",
            12_000,
            "GBP",
            Some((day(2026, 7, 1), day(2026, 7, 31))),
            day(2026, 7, 1),
            day(2026, 7, 8),
        )
        .unwrap()
    }

    #[test]
    fn currency_is_normalised_and_validated() {
        assert_eq!(plan(BillingInterval::Monthly).currency, "GBP");
        assert!(
            MembershipPlan::new(
                MembershipPlanId::from(uuid::Uuid::nil()),
                gym(),
                "X",
                None,
                1,
                "pounds",
                BillingInterval::Monthly,
                vec![Feature::GymAccess],
            )
            .is_err()
        );
    }

    #[test]
    fn a_price_beyond_the_ceiling_is_a_slipped_decimal() {
        assert!(
            MembershipPlan::new(
                MembershipPlanId::from(uuid::Uuid::nil()),
                gym(),
                "X",
                None,
                MAX_PRICE_MINOR + 1,
                "GBP",
                BillingInterval::Monthly,
                vec![Feature::GymAccess],
            )
            .is_err()
        );
    }

    #[test]
    fn monthly_recurs_and_one_off_does_not() {
        assert_eq!(
            BillingInterval::Monthly.next_charge_after(day(2026, 1, 31)),
            Some(day(2026, 2, 28)),
            "month-end must clamp, not overflow into March"
        );
        assert_eq!(
            BillingInterval::Once.next_charge_after(day(2026, 1, 1)),
            None
        );
    }

    #[test]
    fn an_archived_plan_takes_no_new_members() {
        let mut p = plan(BillingInterval::Monthly);
        p.archived_at = Some(now());
        assert!(
            MemberSubscription::start(
                SubscriptionId::from(uuid::Uuid::nil()),
                gym(),
                user(),
                &p,
                day(2026, 7, 1)
            )
            .is_err()
        );
    }

    #[test]
    fn a_subscription_copies_the_price_so_a_plan_change_cannot_reprice_it() {
        let mut p = plan(BillingInterval::Monthly);
        let sub = MemberSubscription::start(
            SubscriptionId::from(uuid::Uuid::nil()),
            gym(),
            user(),
            &p,
            day(2026, 7, 1),
        )
        .unwrap();
        p.price_minor = 99_900;
        assert_eq!(sub.price_minor, 12_000);
    }

    #[test]
    fn cancelling_twice_is_refused() {
        let mut sub = MemberSubscription::start(
            SubscriptionId::from(uuid::Uuid::nil()),
            gym(),
            user(),
            &plan(BillingInterval::Monthly),
            day(2026, 7, 1),
        )
        .unwrap();
        assert!(sub.cancel(now(), day(2026, 8, 1)).is_ok());
        assert!(
            sub.next_charge_on.is_none(),
            "a cancelled plan stops charging"
        );
        assert!(sub.cancel(now(), day(2026, 8, 1)).is_err());
    }

    #[test]
    fn overdue_is_a_question_about_today_not_a_stored_state() {
        let inv = invoice();
        assert!(
            !inv.is_overdue(day(2026, 7, 8)),
            "due today is not yet late"
        );
        assert!(inv.is_overdue(day(2026, 7, 9)));
        assert_eq!(inv.days_overdue(day(2026, 7, 17)), 9);
        assert_eq!(inv.days_overdue(day(2026, 7, 1)), 0);
    }

    #[test]
    fn a_paid_invoice_is_never_overdue_however_late_it_was() {
        let mut inv = invoice();
        inv.mark_paid(now()).unwrap();
        assert!(!inv.is_overdue(day(2027, 1, 1)));
    }

    #[test]
    fn terminal_invoice_states_are_terminal() {
        let mut paid = invoice();
        paid.mark_paid(now()).unwrap();
        assert!(paid.mark_paid(now()).is_err());
        assert!(
            paid.void(now(), user(), None).is_err(),
            "refund, do not void"
        );

        let mut voided = invoice();
        voided
            .void(now(), user(), Some("issued in error".into()))
            .unwrap();
        assert!(voided.mark_paid(now()).is_err());
        assert!(voided.void(now(), user(), None).is_err());
    }

    #[test]
    fn an_invoice_cannot_fall_due_before_it_is_issued() {
        assert!(
            Invoice::issue(
                InvoiceId::from(uuid::Uuid::nil()),
                gym(),
                user(),
                None,
                "INV-1",
                "X",
                100,
                "GBP",
                None,
                day(2026, 7, 10),
                day(2026, 7, 1),
            )
            .is_err()
        );
    }

    #[test]
    fn a_voided_invoice_takes_no_payment() {
        let mut inv = invoice();
        inv.void(now(), user(), None).unwrap();
        assert!(
            Payment::record(
                PaymentId::from(uuid::Uuid::nil()),
                gym(),
                &inv,
                12_000,
                PaymentProvider::Cash,
                None,
                day(2026, 7, 9),
                user(),
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn a_refund_is_a_negative_payment_and_a_zero_is_nothing() {
        let inv = invoice();
        let refund = Payment::record(
            PaymentId::from(uuid::Uuid::nil()),
            gym(),
            &inv,
            -5_000,
            PaymentProvider::Cash,
            None,
            day(2026, 7, 9),
            user(),
            None,
        );
        assert!(refund.is_ok());
        assert!(
            Payment::record(
                PaymentId::from(uuid::Uuid::nil()),
                gym(),
                &inv,
                0,
                PaymentProvider::Cash,
                None,
                day(2026, 7, 9),
                user(),
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn money_formats_the_same_wherever_it_is_written() {
        assert_eq!(format_money(12_050, "GBP"), "£120.50");
        assert_eq!(format_money(0, "GBP"), "£0.00");
        assert_eq!(format_money(5, "USD"), "$0.05");
        assert_eq!(format_money(-2_500, "EUR"), "-€25.00");
        assert_eq!(
            format_money(100, "SEK"),
            "1.00",
            "unknown codes lose the symbol, not the amount"
        );
    }
}
