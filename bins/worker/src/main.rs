//! The worker: periodic jobs and the outbox drain (ADR-0027).
//!
//! CLAUDE.md said, for a long time, "no `worker` crate yet — it arrives when
//! there are outbox events to drain". This is that arrival. What forced it was
//! not the outbox but the calendar: `member_subscriptions.next_charge_on` was
//! set at signup and never advanced, so a monthly membership invoiced exactly
//! once, ever. That cannot be fixed inside a request — nobody makes a request
//! on the first of the month.
//!
//! Deliberately a **separate binary**, not a background task inside the API:
//!
//!   * it needs a privileged connection and no tenant context, which is the
//!     precise opposite of what every request handler must have;
//!   * a slow sweep must not compete with request handling for the pool;
//!   * scaling the API to several instances must not multiply the nightly
//!     billing run, and a job that only ever runs in one place is far easier
//!     to reason about than one guarded by an election.
//!
//! Safe to run several copies anyway — `lease_job` and `FOR UPDATE SKIP
//! LOCKED` make concurrency a non-event — but nothing requires it.
//!
//! Run once and exit with `--once`, which is what the verification suite and a
//! cron-style deployment both want.

use std::time::Duration;

use anyhow::Context;
use chrono::Utc;
use gym_infrastructure::{
    billing_cycle::{run_billing_tick, run_idle_client_sweep, run_overdue_sweep},
    outbox::{claim_batch, dead_letters, finish_job, lease_job, mark_failed, mark_processed},
};

/// How often the loop wakes. Not how often jobs run — each job carries its own
/// minimum interval, and the lease enforces it. A short tick just means the
/// worker notices new outbox events quickly.
const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Events per drain. Small enough that one poison event cannot hold a long
/// transaction open, large enough that a backlog clears in reasonable time.
const BATCH_SIZE: i64 = 50;

/// A client is "idle" after this long without a session. Two weeks is long
/// enough not to flag a holiday and short enough to still be actionable.
const IDLE_DAYS: i64 = 14;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,gym_worker=debug".into()),
        )
        .init();

    // The PRIVILEGED url, not APP_DATABASE_URL. The worker sweeps across gyms
    // and has no tenant context, so it cannot run as the RLS-bound app role —
    // every policy keyed on `app.current_gym` would return nothing and the
    // worker would report a clean run having seen no rows at all. That failure
    // is silent, which is why it is worth stating here.
    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set — the worker needs the privileged role")?;

    let pool = gym_infrastructure::connect(&database_url, 4)
        .await
        .context("connecting to the database")?;

    let once = std::env::args().any(|a| a == "--once");

    if once {
        let summary = run_all(&pool).await?;
        println!("{summary}");
        return Ok(());
    }

    tracing::info!("worker started; polling every {}s", POLL_INTERVAL.as_secs());

    loop {
        // A failed cycle must not kill the worker. Log it and try again on the
        // next tick — the alternative is a container that restart-loops on a
        // transient database blip and stops draining anything at all.
        if let Err(error) = run_all(&pool).await {
            tracing::error!(?error, "worker cycle failed");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// One full cycle: the periodic jobs, then the outbox.
async fn run_all(pool: &sqlx::PgPool) -> anyhow::Result<String> {
    let jobs = run_periodic_jobs(pool).await?;
    let drained = drain_outbox(pool).await?;

    let stuck = dead_letters(pool).await?;
    if !stuck.is_empty() {
        // Loud, and every cycle. A dead-lettered event is a thing that
        // happened and was never acted on; it should be annoying.
        tracing::error!(
            count = stuck.len(),
            "events have exhausted their retries and need attention"
        );
    }

    Ok(format!(
        "{jobs}; drained {drained} event(s); {} dead-lettered",
        stuck.len()
    ))
}

/// The clock-driven work.
async fn run_periodic_jobs(pool: &sqlx::PgPool) -> anyhow::Result<String> {
    let now = Utc::now();
    let today = now.date_naive();
    let mut lines = Vec::new();

    // --- billing ----------------------------------------------------------
    //
    // Daily. Everything inside one transaction WITH the lease, so a crash
    // halfway through rolls back the lease too and the next run starts clean
    // rather than having half-billed a gym.
    let mut tx = pool.begin().await?;
    if lease_job(&mut tx, "billing-tick", chrono::Duration::hours(20), now)
        .await?
        .is_some()
    {
        match run_billing_tick(&mut tx, today).await {
            Ok(outcome) => {
                let text = outcome.to_string();
                finish_job(&mut tx, "billing-tick", &text, true, Utc::now()).await?;
                tx.commit().await?;
                tracing::info!(%text, "billing tick");
                lines.push(format!("billing: {text}"));
            }
            Err(error) => {
                // Roll the work back, then record the failure in its own
                // transaction — a failure note written inside the aborted
                // transaction would be rolled back with everything else, and
                // the job would look like it had never run.
                tx.rollback().await?;
                let mut fail_tx = pool.begin().await?;
                finish_job(
                    &mut fail_tx,
                    "billing-tick",
                    &error.to_string(),
                    false,
                    Utc::now(),
                )
                .await?;
                fail_tx.commit().await?;
                tracing::error!(?error, "billing tick failed");
                lines.push("billing: FAILED".to_owned());
            }
        }
    } else {
        tx.rollback().await?;
        lines.push("billing: not due".to_owned());
    }

    // --- overdue notices ---------------------------------------------------
    let mut tx = pool.begin().await?;
    if lease_job(&mut tx, "overdue-sweep", chrono::Duration::hours(20), now)
        .await?
        .is_some()
    {
        let noticed = run_overdue_sweep(&mut tx, today).await?;
        finish_job(
            &mut tx,
            "overdue-sweep",
            &format!("noticed {noticed}"),
            true,
            Utc::now(),
        )
        .await?;
        tx.commit().await?;
        lines.push(format!("overdue: {noticed}"));
    } else {
        tx.rollback().await?;
        lines.push("overdue: not due".to_owned());
    }

    // --- idle clients ------------------------------------------------------
    let mut tx = pool.begin().await?;
    if lease_job(&mut tx, "idle-clients", chrono::Duration::hours(20), now)
        .await?
        .is_some()
    {
        let flagged = run_idle_client_sweep(&mut tx, IDLE_DAYS, now).await?;
        finish_job(
            &mut tx,
            "idle-clients",
            &format!("flagged {flagged}"),
            true,
            Utc::now(),
        )
        .await?;
        tx.commit().await?;
        lines.push(format!("idle clients: {flagged}"));
    } else {
        tx.rollback().await?;
        lines.push("idle clients: not due".to_owned());
    }

    Ok(lines.join("; "))
}

/// Drain what is waiting, one batch, and report how many were handled.
async fn drain_outbox(pool: &sqlx::PgPool) -> anyhow::Result<usize> {
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    let batch = claim_batch(&mut tx, BATCH_SIZE, now).await?;

    if batch.is_empty() {
        tx.rollback().await?;
        return Ok(0);
    }

    let mut handled = 0;

    for event in batch {
        match handle(&event) {
            Ok(()) => {
                mark_processed(&mut tx, event.id, now).await?;
                handled += 1;
            }
            Err(error) => {
                tracing::warn!(
                    event = %event.event_type,
                    attempts = event.attempts,
                    ?error,
                    "event handler failed; will retry with backoff"
                );
                mark_failed(&mut tx, event.id, event.attempts, &error.to_string(), now).await?;
            }
        }
    }

    tx.commit().await?;
    Ok(handled)
}

/// Act on one event.
///
/// Every arm currently logs and succeeds, and that is the honest state of this
/// feature rather than a placeholder to be embarrassed about: the *delivery*
/// mechanisms these events want — email, push — do not exist yet. What matters
/// is that the events are now being produced, recorded and drained, so adding
/// a real handler is a change in one arm here rather than a change to billing.
///
/// An unknown event type is NOT a failure. Rolling out a producer before its
/// consumer is normal, and retrying an event nobody handles until it
/// dead-letters would turn a routine deploy order into an alarm.
fn handle(event: &gym_infrastructure::outbox::OutboxEvent) -> anyhow::Result<()> {
    match event.event_type.as_str() {
        "invoice.issued" => {
            tracing::info!(
                gym = %event.gym_id,
                payload = %event.payload,
                "invoice issued — a receipt would be sent here once email exists"
            );
        }
        "invoice.overdue" => {
            tracing::warn!(
                gym = %event.gym_id,
                payload = %event.payload,
                "invoice overdue — a reminder would be sent here"
            );
        }
        "client.idle" => {
            tracing::info!(
                gym = %event.gym_id,
                payload = %event.payload,
                "client has not trained — their coach would be told here"
            );
        }
        other => {
            tracing::debug!(event = other, "no handler registered; acknowledged");
        }
    }
    Ok(())
}
