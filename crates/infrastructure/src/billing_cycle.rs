//! The nightly billing tick: issue the invoices that have come due.
//!
//! The gap this closes: `member_subscriptions.next_charge_on` was written once
//! at signup and never advanced, and `BillingInterval::next_charge_after` —
//! implemented, unit-tested, correct — had no caller outside its own tests. A
//! monthly membership therefore invoiced exactly once, ever. Everything a
//! member saw on their Membership screen was true and permanently incomplete.
//!
//! Three independent guards against the one outcome that actually matters,
//! which is billing somebody twice:
//!
//!   1. the job lease, so two workers cannot run the tick concurrently;
//!   2. `next_charge_on` advancing in the same transaction as the insert;
//!   3. a **unique index** on `(subscription_id, period_start)` for non-void
//!      invoices — the only one of the three that survives a bug in the other
//!      two, which is why it exists.
//!
//! Runs with a privileged connection and no tenant context, deliberately: it
//! sweeps every gym. That is why it lives here rather than behind
//! `BillingService`, whose every method quite rightly demands a `TenantContext`
//! and a manager.

use chrono::{Datelike, NaiveDate, Utc};
use gym_domain::billing::{BillingInterval, format_money};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::outbox::emit_in_tx;

/// How long after issue an invoice falls due. Fourteen days is the common
/// default for a gym membership and is not load-bearing — a gym that wants
/// different terms needs a plan-level setting, which does not exist yet and is
/// not worth inventing before someone asks.
const PAYMENT_TERMS_DAYS: i64 = 14;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TickOutcome {
    pub issued: u32,
    /// Subscriptions that were due but already had an invoice for the period.
    /// Not an error — it is the idempotency guard doing its job, and it should
    /// be visible rather than silent.
    pub already_billed: u32,
    /// Subscriptions whose interval yields no next date (one-off plans that
    /// somehow carry a charge date). Left alone and reported.
    pub skipped: u32,
}

impl std::fmt::Display for TickOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "issued {}, already billed {}, skipped {}",
            self.issued, self.already_billed, self.skipped
        )
    }
}

/// The most periods one subscription can be caught up in a single run.
///
/// A guard against a data error — a charge date set to 1970 — turning one tick
/// into twelve hundred invoices. Two years of monthly arrears is far more than
/// any real gym will have and still finite.
const MAX_CATCHUP_PERIODS: u32 = 24;

/// Issue every invoice due on or before `today`, across all gyms.
///
/// **Catches a subscription up fully**, not one period per run. A membership
/// three months in arrears owes three invoices, and a job that issued one per
/// night would take three nights to say so — with the member seeing a
/// partial, wrong balance the whole time. The first version of this did
/// exactly that; the verification suite caught it by finding June, July and
/// August invoices appearing across successive runs instead of together.
///
/// Takes the caller's transaction so the whole tick is atomic with the job
/// lease that authorised it: a crash halfway through rolls back both, and the
/// next run starts cleanly rather than half-billing a gym.
pub async fn run_billing_tick(
    tx: &mut Transaction<'_, Postgres>,
    today: NaiveDate,
) -> Result<TickOutcome, sqlx::Error> {
    // `FOR UPDATE SKIP LOCKED` on the subscriptions themselves: if a manager
    // is editing one in another transaction we skip it this run rather than
    // blocking the whole tick behind a UI interaction.
    let due = sqlx::query!(
        r#"
        SELECT s.id, s.gym_id, s.member_id, s.price_minor, s.currency,
               s.next_charge_on AS "next_charge_on!",
               p.name AS plan_name,
               p.interval AS plan_interval
        FROM member_subscriptions s
        JOIN membership_plans p ON p.id = s.plan_id
        WHERE s.status = 'active'
          AND s.next_charge_on IS NOT NULL
          AND s.next_charge_on <= $1
        ORDER BY s.gym_id, s.id
        FOR UPDATE OF s SKIP LOCKED
        "#,
        today,
    )
    .fetch_all(&mut **tx)
    .await?;

    let mut outcome = TickOutcome::default();

    for row in due {
        let Some(interval) = BillingInterval::parse(&row.plan_interval).ok() else {
            outcome.skipped += 1;
            continue;
        };

        // Walk forward through every period that has already come due. One
        // iteration per unpaid month, so arrears are settled in one pass.
        let mut period_start = row.next_charge_on;
        let mut caught_up = 0;

        while period_start <= today && caught_up < MAX_CATCHUP_PERIODS {
            let Some(period_end) = interval.next_charge_after(period_start) else {
                // A one-off plan with a charge date is a data oddity, not a
                // recurring bill. Clear the date so it stops being picked up
                // forever, and count it rather than swallowing it.
                sqlx::query!(
                    r#"UPDATE member_subscriptions SET next_charge_on = NULL WHERE id = $1"#,
                    row.id,
                )
                .execute(&mut **tx)
                .await?;
                outcome.skipped += 1;
                break;
            };

            // Reference numbers are per gym, per year — allocated the same way the
            // manual path does it, so the sequence a gym sees stays continuous
            // whether an invoice was raised by hand or by this job.
            let number = sqlx::query_scalar!(
                r#"
            INSERT INTO invoice_sequences (gym_id, year, next_number)
            VALUES ($1, $2, 2)
            ON CONFLICT (gym_id, year)
            DO UPDATE SET next_number = invoice_sequences.next_number + 1
            RETURNING next_number - 1 AS "number!"
            "#,
                row.gym_id,
                today.year(),
            )
            .fetch_one(&mut **tx)
            .await?;

            let reference = format!("INV-{}-{number:04}", today.year());
            let description = format!("{} · {}", row.plan_name, period_start.format("%b %Y"));

            // ON CONFLICT DO NOTHING against the period unique index. This is the
            // guard that matters: whatever else goes wrong, a member is not billed
            // twice for one month.
            let inserted = sqlx::query!(
                r#"
            INSERT INTO invoices
                (id, gym_id, member_id, subscription_id, reference, description,
                 amount_minor, currency, period_start, period_end, issued_on, due_on, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'due')
            ON CONFLICT DO NOTHING
            "#,
                Uuid::now_v7(),
                row.gym_id,
                row.member_id,
                row.id,
                reference,
                description,
                row.price_minor,
                row.currency,
                period_start,
                period_end,
                today,
                today + chrono::Duration::days(PAYMENT_TERMS_DAYS),
            )
            .execute(&mut **tx)
            .await?;

            // Advance regardless of whether we inserted. If the period was already
            // billed, leaving the date where it is would re-pick the same
            // subscription on every future run — a queue that never drains.
            sqlx::query!(
                r#"UPDATE member_subscriptions SET next_charge_on = $2 WHERE id = $1"#,
                row.id,
                period_end,
            )
            .execute(&mut **tx)
            .await?;

            caught_up += 1;
            period_start = period_end;

            if inserted.rows_affected() == 0 {
                outcome.already_billed += 1;
                continue;
            }

            outcome.issued += 1;

            // The event the rest of the system hangs off: a receipt email, a push,
            // a dunning timer. None of those exist yet; the event does, so they can
            // be added without touching billing again.
            emit_in_tx(
                tx,
                gym_domain::GymId::from(row.gym_id),
                "invoice.issued",
                serde_json::json!({
                    "member_id": row.member_id,
                    "subscription_id": row.id,
                    "reference": reference,
                    "amount": format_money(row.price_minor, &row.currency),
                    "period_start": period_start,
                    "period_end": period_end,
                }),
            )
            .await?;
        }
    }

    Ok(outcome)
}

/// Emit an event for every invoice that has quietly gone past its due date.
///
/// Deliberately does NOT change the invoice. "Overdue" is derived from the due
/// date and today (see the invoices migration) and storing it would create the
/// second source of truth that comment exists to forbid. This only notices,
/// once, so something downstream can act — and the `notified` marker lives on
/// the EVENT, not on the invoice, which is what keeps that promise.
pub async fn run_overdue_sweep(
    tx: &mut Transaction<'_, Postgres>,
    today: NaiveDate,
) -> Result<u32, sqlx::Error> {
    let overdue = sqlx::query!(
        r#"
        SELECT i.id, i.gym_id, i.member_id, i.reference, i.amount_minor, i.currency, i.due_on
        FROM invoices i
        WHERE i.status = 'due'
          AND i.due_on < $1
          -- Once per invoice, ever. The outbox row IS the record that we
          -- noticed, so no column has to be added to invoices to track it.
          AND NOT EXISTS (
                SELECT 1 FROM domain_events e
                WHERE e.event_type = 'invoice.overdue'
                  AND e.payload ->> 'invoice_id' = i.id::text
              )
        ORDER BY i.due_on
        "#,
        today,
    )
    .fetch_all(&mut **tx)
    .await?;

    let count = u32::try_from(overdue.len()).unwrap_or(u32::MAX);

    for row in overdue {
        emit_in_tx(
            tx,
            gym_domain::GymId::from(row.gym_id),
            "invoice.overdue",
            serde_json::json!({
                "invoice_id": row.id,
                "member_id": row.member_id,
                "reference": row.reference,
                "amount": format_money(row.amount_minor, &row.currency),
                "due_on": row.due_on,
                "days_late": (today - row.due_on).num_days(),
            }),
        )
        .await?;
    }

    Ok(count)
}

/// Notice clients who have stopped turning up.
///
/// The coach-facing half of the worker, and the reason the outbox is worth
/// having at all: this is information a trainer cannot get by looking at a
/// screen, because the whole point is that nothing happened.
pub async fn run_idle_client_sweep(
    tx: &mut Transaction<'_, Postgres>,
    idle_days: i64,
    now: chrono::DateTime<Utc>,
) -> Result<u32, sqlx::Error> {
    let cutoff = now - chrono::Duration::days(idle_days);

    let idle = sqlx::query!(
        r#"
        SELECT r.gym_id, r.coach_id, r.athlete_id, u.display_name AS athlete_name,
               (SELECT max(s.started_at)
                  FROM workout_sessions s
                 WHERE s.athlete_id = r.athlete_id AND s.gym_id = r.gym_id) AS last_session
        FROM coach_relationships r
        JOIN users u ON u.id = r.athlete_id
        WHERE r.status = 'active'
          -- They must be ON something. "Hasn't trained" is only a useful
          -- signal for someone who was supposed to be.
          AND EXISTS (
                SELECT 1 FROM program_assignments a
                WHERE a.athlete_id = r.athlete_id
                  AND a.gym_id = r.gym_id
                  AND a.status = 'active'
              )
          AND NOT EXISTS (
                SELECT 1 FROM workout_sessions s
                WHERE s.athlete_id = r.athlete_id
                  AND s.gym_id = r.gym_id
                  AND s.started_at >= $1
              )
          -- Once per athlete per week, so a client who is away for a month
          -- generates four notices rather than thirty.
          AND NOT EXISTS (
                SELECT 1 FROM domain_events e
                WHERE e.event_type = 'client.idle'
                  AND e.payload ->> 'athlete_id' = r.athlete_id::text
                  AND e.occurred_at >= $2
              )
        "#,
        cutoff,
        now - chrono::Duration::days(7),
    )
    .fetch_all(&mut **tx)
    .await?;

    let count = u32::try_from(idle.len()).unwrap_or(u32::MAX);

    for row in idle {
        emit_in_tx(
            tx,
            gym_domain::GymId::from(row.gym_id),
            "client.idle",
            serde_json::json!({
                "coach_id": row.coach_id,
                "athlete_id": row.athlete_id,
                "athlete_name": row.athlete_name,
                "last_session": row.last_session,
                "idle_days": idle_days,
            }),
        )
        .await?;
    }

    Ok(count)
}
