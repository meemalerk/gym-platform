//! Postgres adapter for billing.
//!
//! Two things here are load-bearing beyond the usual CRUD.
//!
//! **Every mutation writes its audit row in the same transaction**, like every
//! other repository — but here it matters most: an invoice voided or a payment
//! recorded with no trail is precisely the argument an audit log exists to
//! settle.
//!
//! **Settling an invoice happens in the same transaction as the payment that
//! settles it.** A payment landing while its invoice still reads "due" is an
//! inconsistency about money, which is the worst kind to reconcile later.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{BillingRepository, InvoiceView, PlanView, SubscriptionView},
};
use gym_domain::{
    GymId, InvoiceId, MembershipPlanId, TenantContext, UserId,
    billing::{
        BillingInterval, Invoice, InvoiceStatus, MemberSubscription, MembershipPlan, Payment,
        PaymentProvider, SubscriptionStatus,
    },
    entitlement::Feature,
    ids::{PaymentId, SubscriptionId},
};
use uuid::Uuid;

use crate::{audit::record_in_tx, db::begin_tenant_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

/// The frozen-invoice trigger speaks in sentences; forward the useful one.
fn map_triggers(err: sqlx::Error) -> ApplicationError {
    if let sqlx::Error::Database(db) = &err {
        let m = db.message();
        if m.contains("cannot change state") || m.contains("cannot be rewritten") {
            return ApplicationError::Conflict("this invoice".to_owned());
        }
        if db.is_unique_violation() {
            return ApplicationError::Conflict("this record".to_owned());
        }
    }
    db_err(err)
}

#[derive(Debug, Clone)]
pub struct PgBillingRepository {
    pool: crate::DbPool,
}

impl PgBillingRepository {
    #[must_use]
    pub const fn new(pool: crate::DbPool) -> Self {
        Self { pool }
    }
}

fn invoice_status_from_row(
    status: &str,
    paid_at: Option<DateTime<Utc>>,
    voided_at: Option<DateTime<Utc>>,
    voided_by: Option<Uuid>,
    reason: Option<String>,
) -> ApplicationResult<InvoiceStatus> {
    let corrupt = |what: &str| {
        ApplicationError::Internal(format!("invoice is '{what}' with no evidence").into())
    };

    match status {
        "due" => Ok(InvoiceStatus::Due),
        "paid" => paid_at
            .map(|paid_at| InvoiceStatus::Paid { paid_at })
            .ok_or_else(|| corrupt("paid")),
        "void" => match (voided_at, voided_by) {
            (Some(voided_at), Some(voided_by)) => Ok(InvoiceStatus::Void {
                voided_at,
                voided_by: UserId::from(voided_by),
                reason,
            }),
            _ => Err(corrupt("void")),
        },
        other => Err(ApplicationError::Internal(
            format!("unknown invoice status {other}").into(),
        )),
    }
}

#[async_trait]
impl BillingRepository for PgBillingRepository {
    async fn list_plans(&self, tenant: &TenantContext) -> ApplicationResult<Vec<PlanView>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT p.id, p.gym_id, p.name, p.description, p.price_minor, p.currency,
                   p.interval, p.grants, p.archived_at,
                   (SELECT count(*) FROM member_subscriptions s
                     WHERE s.plan_id = p.id AND s.status = 'active') AS "active_subscribers!"
            FROM membership_plans p
            WHERE p.gym_id = $1
            ORDER BY p.archived_at NULLS FIRST, p.price_minor
            "#,
            tenant.gym_id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(PlanView {
                    plan: MembershipPlan {
                        id: MembershipPlanId::from(r.id),
                        gym_id: GymId::from(r.gym_id),
                        name: r.name,
                        description: r.description,
                        price_minor: r.price_minor,
                        currency: r.currency,
                        interval: BillingInterval::parse(&r.interval)?,
                        grants: r.grants.iter().filter_map(|g| Feature::parse(g)).collect(),
                        archived_at: r.archived_at,
                    },
                    active_subscribers: r.active_subscribers,
                })
            })
            .collect()
    }

    async fn find_plan(
        &self,
        tenant: &TenantContext,
        id: MembershipPlanId,
    ) -> ApplicationResult<Option<MembershipPlan>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT id, gym_id, name, description, price_minor, currency, interval, grants,
                   archived_at
            FROM membership_plans WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        row.map(|r| {
            Ok(MembershipPlan {
                id: MembershipPlanId::from(r.id),
                gym_id: GymId::from(r.gym_id),
                name: r.name,
                description: r.description,
                price_minor: r.price_minor,
                currency: r.currency,
                interval: BillingInterval::parse(&r.interval)?,
                grants: r.grants.iter().filter_map(|g| Feature::parse(g)).collect(),
                archived_at: r.archived_at,
            })
        })
        .transpose()
    }

    async fn insert_plan(
        &self,
        tenant: &TenantContext,
        plan: &MembershipPlan,
    ) -> ApplicationResult<()> {
        debug_assert_eq!(plan.gym_id, tenant.gym_id);
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO membership_plans
                (id, gym_id, name, description, price_minor, currency, interval, grants)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            plan.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            plan.name,
            plan.description,
            plan.price_minor,
            plan.currency,
            plan.interval.as_str(),
            &plan
                .grants
                .iter()
                .map(|f| f.as_str().to_owned())
                .collect::<Vec<_>>(),
        )
        .execute(&mut *tx)
        .await
        .map_err(map_triggers)?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "plan.created",
            "membership_plan",
            Some(plan.id.into_uuid()),
            serde_json::json!({ "name": plan.name, "price_minor": plan.price_minor }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)
    }

    async fn archive_plan(
        &self,
        tenant: &TenantContext,
        id: MembershipPlanId,
    ) -> ApplicationResult<bool> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Compare-and-swap on still-offered: archiving twice is a no-op the
        // caller is told about, not a silent success.
        let updated = sqlx::query!(
            r#"
            UPDATE membership_plans SET archived_at = now()
            WHERE gym_id = $1 AND id = $2 AND archived_at IS NULL
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?
        .rows_affected();

        if updated == 0 {
            tx.rollback().await.map_err(db_err)?;
            return Ok(false);
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "plan.archived",
            "membership_plan",
            Some(id.into_uuid()),
            serde_json::json!({}),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }

    async fn list_subscriptions(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<SubscriptionView>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT s.id, s.gym_id, s.member_id, s.plan_id, s.price_minor, s.currency,
                   s.status, s.started_on, s.next_charge_on, s.cancelled_at, s.ends_on,
                   u.display_name AS member_name,
                   p.name AS plan_name
            FROM member_subscriptions s
            JOIN users u ON u.id = s.member_id
            JOIN membership_plans p ON p.id = s.plan_id
            WHERE s.gym_id = $1
            ORDER BY s.status, u.display_name
            "#,
            tenant.gym_id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                let status = match r.status.as_str() {
                    "active" => SubscriptionStatus::Active,
                    "cancelled" => match (r.cancelled_at, r.ends_on) {
                        (Some(cancelled_at), Some(ends_on)) => SubscriptionStatus::Cancelled {
                            cancelled_at,
                            ends_on,
                        },
                        _ => {
                            return Err(ApplicationError::Internal(
                                "subscription is cancelled with no evidence".into(),
                            ));
                        }
                    },
                    other => {
                        return Err(ApplicationError::Internal(
                            format!("unknown subscription status {other}").into(),
                        ));
                    }
                };

                Ok(SubscriptionView {
                    subscription: MemberSubscription {
                        id: SubscriptionId::from(r.id),
                        gym_id: GymId::from(r.gym_id),
                        member_id: UserId::from(r.member_id),
                        plan_id: MembershipPlanId::from(r.plan_id),
                        price_minor: r.price_minor,
                        currency: r.currency,
                        status,
                        started_on: r.started_on,
                        next_charge_on: r.next_charge_on,
                    },
                    member_name: r.member_name,
                    plan_name: r.plan_name,
                })
            })
            .collect()
    }

    async fn find_subscription(
        &self,
        tenant: &TenantContext,
        id: SubscriptionId,
    ) -> ApplicationResult<Option<MemberSubscription>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT id, gym_id, member_id, plan_id, price_minor, currency,
                   status, started_on, next_charge_on, cancelled_at, ends_on
            FROM member_subscriptions
            WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid(),
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        row.map(|r| {
            let status = match r.status.as_str() {
                "active" => SubscriptionStatus::Active,
                "cancelled" => match (r.cancelled_at, r.ends_on) {
                    (Some(cancelled_at), Some(ends_on)) => SubscriptionStatus::Cancelled {
                        cancelled_at,
                        ends_on,
                    },
                    _ => {
                        return Err(ApplicationError::Internal(
                            "subscription is cancelled with no evidence".into(),
                        ));
                    }
                },
                other => {
                    return Err(ApplicationError::Internal(
                        format!("unknown subscription status '{other}'").into(),
                    ));
                }
            };

            Ok(MemberSubscription {
                id: SubscriptionId::from(r.id),
                gym_id: GymId::from(r.gym_id),
                member_id: UserId::from(r.member_id),
                plan_id: MembershipPlanId::from(r.plan_id),
                price_minor: r.price_minor,
                currency: r.currency,
                status,
                started_on: r.started_on,
                next_charge_on: r.next_charge_on,
            })
        })
        .transpose()
    }

    async fn save_cancelled_subscription(
        &self,
        tenant: &TenantContext,
        subscription: &MemberSubscription,
    ) -> ApplicationResult<()> {
        let (cancelled_at, ends_on) = match &subscription.status {
            SubscriptionStatus::Cancelled {
                cancelled_at,
                ends_on,
            } => (*cancelled_at, *ends_on),
            SubscriptionStatus::Active => {
                return Err(ApplicationError::Internal(
                    "save_cancelled_subscription called with an active subscription".into(),
                ));
            }
        };

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Guarded on `status = 'active'`: two managers cancelling at once must
        // not both succeed and produce two different end dates.
        let updated = sqlx::query!(
            r#"
            UPDATE member_subscriptions
               SET status = 'cancelled', cancelled_at = $3, ends_on = $4, next_charge_on = NULL
             WHERE gym_id = $1 AND id = $2 AND status = 'active'
            "#,
            tenant.gym_id.into_uuid(),
            subscription.id.into_uuid(),
            cancelled_at,
            ends_on,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        if updated.rows_affected() == 0 {
            return Err(ApplicationError::Conflict(
                "this subscription has already been cancelled".to_owned(),
            ));
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "subscription.cancelled",
            "subscription",
            Some(subscription.id.into_uuid()),
            serde_json::json!({
                "member_id": subscription.member_id.into_uuid(),
                "ends_on": ends_on,
            }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn insert_subscription(
        &self,
        tenant: &TenantContext,
        subscription: &MemberSubscription,
    ) -> ApplicationResult<()> {
        debug_assert_eq!(subscription.gym_id, tenant.gym_id);
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO member_subscriptions
                (id, gym_id, member_id, plan_id, price_minor, currency, status,
                 started_on, next_charge_on)
            VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, $8)
            "#,
            subscription.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            subscription.member_id.into_uuid(),
            subscription.plan_id.into_uuid(),
            subscription.price_minor,
            subscription.currency,
            subscription.started_on,
            subscription.next_charge_on,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_triggers)?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "subscription.created",
            "member_subscription",
            Some(subscription.id.into_uuid()),
            serde_json::json!({ "price_minor": subscription.price_minor }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)
    }

    async fn allocate_invoice_number(
        &self,
        tenant: &TenantContext,
        year: i32,
    ) -> ApplicationResult<i32> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // One statement reads and increments, so concurrent callers serialise
        // on the row rather than racing. The first insert stores 2 and hands
        // back 1; every later call hands back what it just consumed.
        let row = sqlx::query!(
            r#"
            INSERT INTO invoice_sequences (gym_id, year, next_number)
            VALUES ($1, $2, 2)
            ON CONFLICT (gym_id, year)
            DO UPDATE SET next_number = invoice_sequences.next_number + 1
            RETURNING next_number - 1 AS "allocated!"
            "#,
            tenant.gym_id.into_uuid(),
            year,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(row.allocated)
    }

    async fn list_invoices(&self, tenant: &TenantContext) -> ApplicationResult<Vec<InvoiceView>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT i.id, i.gym_id, i.member_id, i.subscription_id, i.reference, i.description,
                   i.amount_minor, i.currency, i.period_start, i.period_end, i.issued_on,
                   i.due_on, i.status, i.paid_at, i.voided_at, i.voided_by, i.void_reason,
                   u.display_name AS member_name,
                   -- sum() over BIGINT is NUMERIC in Postgres; cast back or
                   -- sqlx demands a decimal feature for a column of pennies.
                   COALESCE((SELECT sum(p.amount_minor) FROM payments p
                              WHERE p.invoice_id = i.id), 0)::bigint AS "paid_minor!"
            FROM invoices i
            JOIN users u ON u.id = i.member_id
            WHERE i.gym_id = $1
            ORDER BY i.issued_on DESC, i.reference DESC
            LIMIT 500
            "#,
            tenant.gym_id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(InvoiceView {
                    invoice: Invoice {
                        id: InvoiceId::from(r.id),
                        gym_id: GymId::from(r.gym_id),
                        member_id: UserId::from(r.member_id),
                        subscription_id: r.subscription_id.map(SubscriptionId::from),
                        reference: r.reference,
                        description: r.description,
                        amount_minor: r.amount_minor,
                        currency: r.currency,
                        period: match (r.period_start, r.period_end) {
                            (Some(s), Some(e)) => Some((s, e)),
                            _ => None,
                        },
                        issued_on: r.issued_on,
                        due_on: r.due_on,
                        status: invoice_status_from_row(
                            &r.status,
                            r.paid_at,
                            r.voided_at,
                            r.voided_by,
                            r.void_reason,
                        )?,
                    },
                    member_name: r.member_name,
                    paid_minor: r.paid_minor,
                })
            })
            .collect()
    }

    async fn find_invoice(
        &self,
        tenant: &TenantContext,
        id: InvoiceId,
    ) -> ApplicationResult<Option<Invoice>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT id, gym_id, member_id, subscription_id, reference, description,
                   amount_minor, currency, period_start, period_end, issued_on, due_on,
                   status, paid_at, voided_at, voided_by, void_reason
            FROM invoices WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        row.map(|r| {
            Ok(Invoice {
                id: InvoiceId::from(r.id),
                gym_id: GymId::from(r.gym_id),
                member_id: UserId::from(r.member_id),
                subscription_id: r.subscription_id.map(SubscriptionId::from),
                reference: r.reference,
                description: r.description,
                amount_minor: r.amount_minor,
                currency: r.currency,
                period: match (r.period_start, r.period_end) {
                    (Some(s), Some(e)) => Some((s, e)),
                    _ => None,
                },
                issued_on: r.issued_on,
                due_on: r.due_on,
                status: invoice_status_from_row(
                    &r.status,
                    r.paid_at,
                    r.voided_at,
                    r.voided_by,
                    r.void_reason,
                )?,
            })
        })
        .transpose()
    }

    async fn insert_invoice(
        &self,
        tenant: &TenantContext,
        invoice: &Invoice,
    ) -> ApplicationResult<()> {
        debug_assert_eq!(invoice.gym_id, tenant.gym_id);
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO invoices
                (id, gym_id, member_id, subscription_id, reference, description,
                 amount_minor, currency, period_start, period_end, issued_on, due_on, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'due')
            "#,
            invoice.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            invoice.member_id.into_uuid(),
            invoice.subscription_id.map(SubscriptionId::into_uuid),
            invoice.reference,
            invoice.description,
            invoice.amount_minor,
            invoice.currency,
            invoice.period.map(|(s, _)| s),
            invoice.period.map(|(_, e)| e),
            invoice.issued_on,
            invoice.due_on,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_triggers)?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "invoice.issued",
            "invoice",
            Some(invoice.id.into_uuid()),
            serde_json::json!({
                "reference": invoice.reference,
                "amount_minor": invoice.amount_minor,
            }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)
    }

    async fn save_invoice_state(
        &self,
        tenant: &TenantContext,
        invoice: &Invoice,
        action: &'static str,
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let (paid_at, voided_at, voided_by, reason) = match &invoice.status {
            InvoiceStatus::Due => (None, None, None, None),
            InvoiceStatus::Paid { paid_at } => (Some(*paid_at), None, None, None),
            InvoiceStatus::Void {
                voided_at,
                voided_by,
                reason,
            } => (
                None,
                Some(*voided_at),
                Some(voided_by.into_uuid()),
                reason.clone(),
            ),
        };

        // Compare-and-swap on still-due: two managers settling the same
        // invoice at once must not both succeed.
        let updated = sqlx::query!(
            r#"
            UPDATE invoices
               SET status = $3, paid_at = $4, voided_at = $5, voided_by = $6, void_reason = $7
             WHERE gym_id = $1 AND id = $2 AND status = 'due'
            "#,
            tenant.gym_id.into_uuid(),
            invoice.id.into_uuid(),
            invoice.status.as_str(),
            paid_at,
            voided_at,
            voided_by,
            reason,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_triggers)?
        .rows_affected();

        if updated == 0 {
            tx.rollback().await.map_err(db_err)?;
            return Err(ApplicationError::Conflict("this invoice".to_owned()));
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            action,
            "invoice",
            Some(invoice.id.into_uuid()),
            serde_json::json!({ "reference": invoice.reference }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)
    }

    async fn insert_payment(
        &self,
        tenant: &TenantContext,
        payment: &Payment,
        settles: bool,
    ) -> ApplicationResult<()> {
        debug_assert_eq!(payment.gym_id, tenant.gym_id);
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO payments
                (id, gym_id, invoice_id, amount_minor, currency, provider, provider_ref,
                 received_on, recorded_by, note)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            payment.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            payment.invoice_id.into_uuid(),
            payment.amount_minor,
            payment.currency,
            payment.provider.as_str(),
            payment.provider_ref,
            payment.received_on,
            payment.recorded_by.into_uuid(),
            payment.note,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_triggers)?;

        // The invoice settles in THIS transaction or not at all.
        if settles {
            sqlx::query!(
                r#"
                UPDATE invoices SET status = 'paid', paid_at = now()
                 WHERE gym_id = $1 AND id = $2 AND status = 'due'
                "#,
                tenant.gym_id.into_uuid(),
                payment.invoice_id.into_uuid(),
            )
            .execute(&mut *tx)
            .await
            .map_err(map_triggers)?;
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "payment.recorded",
            "payment",
            Some(payment.id.into_uuid()),
            serde_json::json!({
                "amount_minor": payment.amount_minor,
                "provider": payment.provider.as_str(),
                "settled_invoice": settles,
            }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)
    }

    async fn payments_for(
        &self,
        tenant: &TenantContext,
        invoice: InvoiceId,
    ) -> ApplicationResult<Vec<Payment>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT id, gym_id, invoice_id, amount_minor, currency, provider, provider_ref,
                   received_on, recorded_by, note
            FROM payments WHERE gym_id = $1 AND invoice_id = $2
            ORDER BY received_on, created_at
            "#,
            tenant.gym_id.into_uuid(),
            invoice.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(Payment {
                    id: PaymentId::from(r.id),
                    gym_id: GymId::from(r.gym_id),
                    invoice_id: InvoiceId::from(r.invoice_id),
                    amount_minor: r.amount_minor,
                    currency: r.currency,
                    provider: PaymentProvider::parse(&r.provider)?,
                    provider_ref: r.provider_ref,
                    received_on: r.received_on,
                    recorded_by: UserId::from(r.recorded_by),
                    note: r.note,
                })
            })
            .collect()
    }
}
