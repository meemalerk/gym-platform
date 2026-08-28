//! Postgres adapter for the operating calendar (ADR-0015).
//!
//! Note what this file does NOT contain: the resolution rule. It reads rows;
//! `gym_domain::calendar::resolve_day` decides what they mean. Resolving in SQL
//! would be the second implementation of a rule ADR-0015 insists exists once —
//! and the SQL version would be the one nobody unit-tests.

use async_trait::async_trait;
use chrono::{NaiveDate, NaiveTime};
use gym_application::{ApplicationError, ApplicationResult, ports::CalendarRepository};
use gym_domain::{
    CalendarEntryId, GymId, TenantContext, UserId,
    calendar::{CalendarOverride, TimeSpan, WeeklyHours},
};

use crate::{audit::record_in_tx, db::begin_tenant_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

#[derive(Debug, Clone)]
pub struct PgCalendarRepository {
    pool: crate::DbPool,
}

impl PgCalendarRepository {
    #[must_use]
    pub const fn new(pool: crate::DbPool) -> Self {
        Self { pool }
    }
}

/// Rebuild a span from two columns.
///
/// The CHECK constraint guarantees `closes_at > opens_at`, so a failure here
/// means the constraint was bypassed — corruption, reported as such rather
/// than silently producing a zero-length day nobody can book.
fn span(opens_at: NaiveTime, closes_at: NaiveTime) -> ApplicationResult<TimeSpan> {
    TimeSpan::new(opens_at, closes_at)
        .map_err(|_| ApplicationError::Internal("calendar row has closes_at <= opens_at".into()))
}

#[async_trait]
impl CalendarRepository for PgCalendarRepository {
    async fn opening_hours(&self, tenant: &TenantContext) -> ApplicationResult<Vec<WeeklyHours>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT id, gym_id, weekday, opens_at, closes_at
            FROM gym_opening_hours
            WHERE gym_id = $1
            ORDER BY weekday, opens_at
            "#,
            tenant.gym_id.into_uuid(),
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(WeeklyHours {
                    id: CalendarEntryId::from(r.id),
                    gym_id: GymId::from(r.gym_id),
                    weekday: u8::try_from(r.weekday).unwrap_or(0),
                    span: span(r.opens_at, r.closes_at)?,
                })
            })
            .collect()
    }

    async fn overrides_between(
        &self,
        tenant: &TenantContext,
        from: NaiveDate,
        to: NaiveDate,
    ) -> ApplicationResult<Vec<CalendarOverride>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT id, gym_id, on_date, is_closed, opens_at, closes_at, reason
            FROM gym_calendar_overrides
            WHERE gym_id = $1 AND on_date BETWEEN $2 AND $3
            ORDER BY on_date
            "#,
            tenant.gym_id.into_uuid(),
            from,
            to,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                // The shape CHECK guarantees these agree; if they ever do not,
                // the constraint was bypassed and guessing would be worse than
                // saying so.
                let span = match (r.is_closed, r.opens_at, r.closes_at) {
                    (true, None, None) => None,
                    (false, Some(o), Some(c)) => Some(span(o, c)?),
                    _ => {
                        return Err(ApplicationError::Internal(
                            "calendar override has inconsistent closed/hours state".into(),
                        ));
                    }
                };

                Ok(CalendarOverride {
                    id: CalendarEntryId::from(r.id),
                    gym_id: GymId::from(r.gym_id),
                    on_date: r.on_date,
                    is_closed: r.is_closed,
                    span,
                    reason: r.reason,
                })
            })
            .collect()
    }

    async fn replace_opening_hours(
        &self,
        tenant: &TenantContext,
        hours: &[WeeklyHours],
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Replace wholesale rather than diffing.
        //
        // The pattern is small (a handful of rows) and is edited as a WHOLE —
        // "these are our hours" — so a diff would be more code, more states,
        // and the same result. Inside one transaction, so a reader never sees
        // a gym with no hours at all.
        sqlx::query!(
            r#"DELETE FROM gym_opening_hours WHERE gym_id = $1"#,
            tenant.gym_id.into_uuid(),
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        for entry in hours {
            sqlx::query!(
                r#"
                INSERT INTO gym_opening_hours (id, gym_id, weekday, opens_at, closes_at)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT DO NOTHING
                "#,
                entry.id.into_uuid(),
                tenant.gym_id.into_uuid(),
                i16::from(entry.weekday),
                entry.span.opens_at,
                entry.span.closes_at,
            )
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "gym.hours_changed",
            "gym",
            Some(tenant.gym_id.into_uuid()),
            serde_json::json!({ "spans": hours.len() }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn upsert_override(
        &self,
        tenant: &TenantContext,
        entry: &CalendarOverride,
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // One override per date (the unique index), so setting one twice
        // replaces it rather than failing — a manager correcting a date they
        // already set is the common case, not an error.
        sqlx::query!(
            r#"
            INSERT INTO gym_calendar_overrides
                (id, gym_id, on_date, is_closed, opens_at, closes_at, reason)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (gym_id, on_date) DO UPDATE
                SET is_closed = EXCLUDED.is_closed,
                    opens_at  = EXCLUDED.opens_at,
                    closes_at = EXCLUDED.closes_at,
                    reason    = EXCLUDED.reason
            "#,
            entry.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            entry.on_date,
            entry.is_closed,
            entry.span.map(|s| s.opens_at),
            entry.span.map(|s| s.closes_at),
            entry.reason.as_deref(),
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            if entry.is_closed {
                "gym.closure_set"
            } else {
                "gym.special_hours_set"
            },
            "gym",
            Some(tenant.gym_id.into_uuid()),
            serde_json::json!({ "date": entry.on_date, "reason": entry.reason }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn remove_override(
        &self,
        tenant: &TenantContext,
        on_date: NaiveDate,
    ) -> ApplicationResult<bool> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let deleted = sqlx::query!(
            r#"DELETE FROM gym_calendar_overrides WHERE gym_id = $1 AND on_date = $2"#,
            tenant.gym_id.into_uuid(),
            on_date,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        if deleted.rows_affected() == 0 {
            tx.rollback().await.map_err(db_err)?;
            return Ok(false);
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "gym.override_removed",
            "gym",
            Some(tenant.gym_id.into_uuid()),
            serde_json::json!({ "date": on_date }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }

    async fn trainer_availability(
        &self,
        tenant: &TenantContext,
        trainer: UserId,
    ) -> ApplicationResult<Vec<WeeklyHours>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT id, gym_id, weekday, opens_at, closes_at
            FROM trainer_availability
            WHERE gym_id = $1 AND trainer_id = $2
            ORDER BY weekday, opens_at
            "#,
            tenant.gym_id.into_uuid(),
            trainer.into_uuid(),
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(WeeklyHours {
                    id: CalendarEntryId::from(r.id),
                    gym_id: GymId::from(r.gym_id),
                    weekday: u8::try_from(r.weekday).unwrap_or(0),
                    span: span(r.opens_at, r.closes_at)?,
                })
            })
            .collect()
    }

    async fn replace_trainer_availability(
        &self,
        tenant: &TenantContext,
        trainer: UserId,
        hours: &[WeeklyHours],
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"DELETE FROM trainer_availability WHERE gym_id = $1 AND trainer_id = $2"#,
            tenant.gym_id.into_uuid(),
            trainer.into_uuid(),
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        for entry in hours {
            sqlx::query!(
                r#"
                INSERT INTO trainer_availability
                    (id, gym_id, trainer_id, weekday, opens_at, closes_at)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT DO NOTHING
                "#,
                entry.id.into_uuid(),
                tenant.gym_id.into_uuid(),
                trainer.into_uuid(),
                i16::from(entry.weekday),
                entry.span.opens_at,
                entry.span.closes_at,
            )
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "trainer.availability_changed",
            "user",
            Some(trainer.into_uuid()),
            serde_json::json!({ "spans": hours.len() }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn timezone(&self, tenant: &TenantContext) -> ApplicationResult<String> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;
        let row = sqlx::query_scalar!(
            r#"SELECT timezone FROM gyms WHERE id = $1"#,
            tenant.gym_id.into_uuid(),
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        row.ok_or(ApplicationError::NotFound { entity: "gym" })
    }
}
