//! The transactional outbox and the periodic-job ledger (ADR-0027).
//!
//! Written against a privileged connection, not `gym_app`: the worker sweeps
//! across gyms and has no tenant context, which is exactly the situation RLS
//! exists to prevent a *request* from being in. That is why the app role can
//! INSERT events and nothing else — a handler must never be able to mark the
//! worker's queue as consumed.
//!
//! No apalis, no pgmq, no Redis. `SELECT … FOR UPDATE SKIP LOCKED` is the
//! textbook Postgres queue: it is a dozen lines, it is correct under
//! concurrency, and it needs no infrastructure this project does not already
//! run. CLAUDE.md's "don't reach for heavy infra early" applies with force
//! here — the day there are genuinely independent consumers is the day to
//! revisit it, and that day is not today.

use chrono::{DateTime, Utc};
use gym_domain::GymId;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::DbPool;

/// One thing that happened, waiting to be acted on.
#[derive(Debug, Clone)]
pub struct OutboxEvent {
    pub id: Uuid,
    pub gym_id: GymId,
    pub event_type: String,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
    pub attempts: i32,
}

/// After this many failures an event stops being retried automatically.
///
/// Not deleted, not hidden: it stays in the table with its `last_error` so
/// somebody can look at it. A queue that silently drops what it cannot handle
/// is worse than one that visibly stalls.
pub const MAX_ATTEMPTS: i32 = 8;

/// Emit an event **inside the caller's transaction**.
///
/// This is the entire point of an outbox and the reason it is not a queue
/// client: the event and the state change it describes commit together or not
/// at all. A separate `publish()` after the commit can fail on its own, and
/// then the thing happened but nobody was told — which is the exact failure
/// the audit log is written transactionally to avoid.
pub async fn emit_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    gym_id: GymId,
    event_type: &str,
    payload: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO domain_events (id, gym_id, event_type, payload)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::now_v7(),
        gym_id.into_uuid(),
        event_type,
        payload,
    )
    .execute(&mut **tx)
    .await
    .map(|_| ())
}

/// Claim a batch of pending events for this worker.
///
/// `FOR UPDATE SKIP LOCKED` is what makes several workers safe: each takes a
/// disjoint set and nobody waits. The rows stay locked until the returned
/// transaction ends, so a worker that dies mid-batch releases them for someone
/// else rather than stranding them.
pub async fn claim_batch(
    tx: &mut Transaction<'_, Postgres>,
    limit: i64,
    now: DateTime<Utc>,
) -> Result<Vec<OutboxEvent>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT id, gym_id, event_type, payload, occurred_at, attempts
        FROM domain_events
        WHERE processed_at IS NULL
          AND attempts < $3
          AND (next_attempt_at IS NULL OR next_attempt_at <= $2)
        ORDER BY occurred_at
        LIMIT $1
        FOR UPDATE SKIP LOCKED
        "#,
        limit,
        now,
        MAX_ATTEMPTS,
    )
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| OutboxEvent {
            id: r.id,
            gym_id: GymId::from(r.gym_id),
            event_type: r.event_type,
            payload: r.payload,
            occurred_at: r.occurred_at,
            attempts: r.attempts,
        })
        .collect())
}

/// Mark an event done. Idempotent by construction — `processed_at` is only
/// ever set, never cleared.
pub async fn mark_processed(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE domain_events SET processed_at = $2, last_error = NULL WHERE id = $1"#,
        id,
        now,
    )
    .execute(&mut **tx)
    .await
    .map(|_| ())
}

/// Record a failure and schedule the retry.
///
/// Exponential backoff, capped: 2^attempts seconds up to an hour. Without the
/// cap a handful of failures pushes the next attempt into next week; without
/// the backoff a broken handler spins the worker at full speed against a
/// database that is probably already unhappy.
pub async fn mark_failed(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    attempts: i32,
    error: &str,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let delay_seconds = 2_i64.saturating_pow(attempts.clamp(0, 12) as u32).min(3600);
    let next = now + chrono::Duration::seconds(delay_seconds);

    sqlx::query!(
        r#"
        UPDATE domain_events
           SET attempts = attempts + 1,
               last_error = $2,
               next_attempt_at = $3
         WHERE id = $1
        "#,
        id,
        // Truncated: a Rust error chain can be enormous and this column is for
        // a human reading a log, not for a full backtrace.
        error.chars().take(500).collect::<String>(),
        next,
    )
    .execute(&mut **tx)
    .await
    .map(|_| ())
}

/// Events that have exhausted their retries and need a person.
pub async fn dead_letters(pool: &DbPool) -> Result<Vec<OutboxEvent>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT id, gym_id, event_type, payload, occurred_at, attempts
        FROM domain_events
        WHERE processed_at IS NULL AND attempts >= $1
        ORDER BY occurred_at
        "#,
        MAX_ATTEMPTS,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| OutboxEvent {
            id: r.id,
            gym_id: GymId::from(r.gym_id),
            event_type: r.event_type,
            payload: r.payload,
            occurred_at: r.occurred_at,
            attempts: r.attempts,
        })
        .collect())
}

// ------------------------------------------------------------- periodic jobs

/// A job's row, held under `FOR UPDATE` for the duration of a run.
///
/// The lock IS the concurrency control. Two workers starting at the same
/// second both try to take this row; one gets it and runs, the other blocks
/// and then sees a fresh `last_finished_at` and declines. No leader election,
/// no distributed lock service, no clock synchronisation between hosts.
pub struct JobLease {
    pub name: String,
    pub last_finished_at: Option<DateTime<Utc>>,
}

/// Take the lease for `name`, if it is due.
///
/// Returns `None` when another worker has run it within `min_interval` — which
/// is also what makes a crash-restart loop safe: a worker that dies and comes
/// back up in ten seconds does not re-run a daily job.
pub async fn lease_job(
    tx: &mut Transaction<'_, Postgres>,
    name: &str,
    min_interval: chrono::Duration,
    now: DateTime<Utc>,
) -> Result<Option<JobLease>, sqlx::Error> {
    // Create the row on first sight, then lock it. ON CONFLICT DO NOTHING plus
    // a separate locking SELECT, rather than an upsert, because we want to
    // hold the lock across the whole run and an upsert releases immediately.
    sqlx::query!(
        r#"INSERT INTO job_runs (name) VALUES ($1) ON CONFLICT (name) DO NOTHING"#,
        name,
    )
    .execute(&mut **tx)
    .await?;

    let row = sqlx::query!(
        r#"SELECT name, last_finished_at FROM job_runs WHERE name = $1 FOR UPDATE"#,
        name,
    )
    .fetch_one(&mut **tx)
    .await?;

    if row
        .last_finished_at
        .is_some_and(|finished| now.signed_duration_since(finished) < min_interval)
    {
        return Ok(None);
    }

    sqlx::query!(
        r#"UPDATE job_runs SET last_started_at = $2 WHERE name = $1"#,
        name,
        now,
    )
    .execute(&mut **tx)
    .await?;

    Ok(Some(JobLease {
        name: row.name,
        last_finished_at: row.last_finished_at,
    }))
}

/// Release a lease, recording what the run did.
pub async fn finish_job(
    tx: &mut Transaction<'_, Postgres>,
    name: &str,
    outcome: &str,
    succeeded: bool,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        UPDATE job_runs
           SET last_finished_at = $2,
               last_outcome = $3,
               failures = CASE WHEN $4 THEN 0 ELSE failures + 1 END
         WHERE name = $1
        "#,
        name,
        now,
        outcome.chars().take(500).collect::<String>(),
        succeeded,
    )
    .execute(&mut **tx)
    .await
    .map(|_| ())
}
