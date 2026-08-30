//! Postgres adapter for workout sessions and performed sets.
//!
//! The distinguishing features: inserts are idempotent on the client-generated
//! id (`ON CONFLICT DO NOTHING`, returning whether anything happened), and
//! performed sets have no update or delete path at all — the app role holds
//! INSERT and SELECT only (migration 0011).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{ExecutionRepository, SessionFilter, SessionView},
};
use gym_domain::{
    AssignmentId, ExerciseId, GymId, PerformedSetId, TemplateExerciseId, TenantContext, UserId,
    WorkoutSessionId, WorkoutTemplateId,
    execution::{PerformedSet, PerformedValue, SessionStatus, WorkoutSession},
};

use crate::{audit::record_in_tx, db::begin_tenant_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

#[derive(Debug, Clone)]
pub struct PgExecutionRepository {
    pool: crate::DbPool,
}

impl PgExecutionRepository {
    #[must_use]
    pub const fn new(pool: crate::DbPool) -> Self {
        Self { pool }
    }
}

fn status_from_row(
    status: &str,
    completed_at: Option<DateTime<Utc>>,
    abandoned_at: Option<DateTime<Utc>>,
) -> ApplicationResult<SessionStatus> {
    match status {
        "in_progress" => Ok(SessionStatus::InProgress),
        "completed" => completed_at
            .map(|completed_at| SessionStatus::Completed { completed_at })
            .ok_or_else(|| {
                ApplicationError::Internal("completed session with no timestamp".into())
            }),
        "abandoned" => abandoned_at
            .map(|abandoned_at| SessionStatus::Abandoned { abandoned_at })
            .ok_or_else(|| {
                ApplicationError::Internal("abandoned session with no timestamp".into())
            }),
        other => Err(ApplicationError::Internal(
            format!("unknown session status '{other}'").into(),
        )),
    }
}

/// The validity/frozen triggers speak in sentences; forward the useful ones.
fn map_triggers(err: sqlx::Error) -> ApplicationError {
    if let sqlx::Error::Database(db) = &err {
        let m = db.message();
        if m.contains("cannot log sets into") || m.contains("session cannot be changed") {
            return ApplicationError::Conflict("this session".to_owned());
        }
        if m.contains("does not belong to the assigned")
            || m.contains("does not match the assignment")
            || m.contains("assignment is not active")
        {
            return ApplicationError::Domain(gym_domain::DomainError::Invalid(m.to_owned()));
        }
        if db.is_unique_violation() {
            // (session, exercise, set_number) already logged — the double-tap case.
            return ApplicationError::Conflict("this set number".to_owned());
        }
    }
    db_err(err)
}

#[async_trait]
impl ExecutionRepository for PgExecutionRepository {
    async fn list(
        &self,
        tenant: &TenantContext,
        filter: &SessionFilter,
    ) -> ApplicationResult<Vec<SessionView>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Clamped, not trusted: a client asking for a million rows should get
        // the maximum rather than a timeout, and one asking for zero or a
        // negative almost certainly has a bug rather than an intention.
        let limit = filter.limit.unwrap_or(200).clamp(1, 500);

        // Dates arrive as calendar days and are compared against a timestamptz,
        // so `to` has to mean "the whole of that day". Adding one day and using
        // a strict `<` is the version that does not silently drop everything
        // logged after midnight UTC on the last day of the range.
        let to_exclusive = filter.to.and_then(|d| d.succ_opt());

        let rows = sqlx::query!(
            r#"
            SELECT s.id, s.gym_id, s.athlete_id, s.assignment_id, s.workout_template_id,
                   s.started_at, s.status, s.completed_at, s.abandoned_at, s.ended_at,
                   s.title,
                   u.display_name AS athlete_name,
                   -- LEFT, all three: an unplanned session (ADR-0035) has no
                   -- assignment and no template, and an inner join would drop
                   -- it from every history list in the product rather than
                   -- showing it without a programme name. The nullability is
                   -- forced with `?` because it is the join that makes these
                   -- optional, not the column definitions.
                   t.name AS "workout_name?",
                   p.name AS "program_name?",
                   (SELECT count(*) FROM performed_sets ps WHERE ps.session_id = s.id)
                       AS "set_count!"
            FROM workout_sessions s
            JOIN users u ON u.id = s.athlete_id
            LEFT JOIN workout_templates t ON t.id = s.workout_template_id
            LEFT JOIN program_assignments a ON a.id = s.assignment_id
            LEFT JOIN programs p ON p.id = a.program_id
            WHERE s.gym_id = $1
              AND ($2::uuid IS NULL OR s.athlete_id = $2)
              AND ($3::date IS NULL OR s.started_at >= $3::date)
              AND ($4::date IS NULL OR s.started_at < $4::date)
            ORDER BY s.started_at DESC
            LIMIT $5
            "#,
            tenant.gym_id.into_uuid(),
            filter.athlete_id.map(gym_domain::UserId::into_uuid),
            filter.from,
            to_exclusive,
            limit,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(SessionView {
                    session: WorkoutSession {
                        id: WorkoutSessionId::from(r.id),
                        gym_id: GymId::from(r.gym_id),
                        athlete_id: UserId::from(r.athlete_id),
                        assignment_id: r.assignment_id.map(AssignmentId::from),
                        workout_template_id: r.workout_template_id.map(WorkoutTemplateId::from),
                        title: r.title,
                        started_at: r.started_at,
                        status: status_from_row(&r.status, r.completed_at, r.abandoned_at)?,
                        ended_at: r.ended_at,
                    },
                    athlete_name: r.athlete_name,
                    workout_name: r.workout_name,
                    program_name: r.program_name,
                    set_count: r.set_count,
                })
            })
            .collect()
    }

    async fn find_session_view(
        &self,
        tenant: &TenantContext,
        id: WorkoutSessionId,
    ) -> ApplicationResult<Option<SessionView>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT s.id, s.gym_id, s.athlete_id, s.assignment_id, s.workout_template_id,
                   s.started_at, s.status, s.completed_at, s.abandoned_at, s.ended_at,
                   s.title,
                   u.display_name AS athlete_name,
                   -- LEFT, all three: an unplanned session (ADR-0035) has no
                   -- assignment and no template, and an inner join would drop
                   -- it from every history list in the product rather than
                   -- showing it without a programme name. The nullability is
                   -- forced with `?` because it is the join that makes these
                   -- optional, not the column definitions.
                   t.name AS "workout_name?",
                   p.name AS "program_name?",
                   (SELECT count(*) FROM performed_sets ps WHERE ps.session_id = s.id)
                       AS "set_count!"
            FROM workout_sessions s
            JOIN users u ON u.id = s.athlete_id
            LEFT JOIN workout_templates t ON t.id = s.workout_template_id
            LEFT JOIN program_assignments a ON a.id = s.assignment_id
            LEFT JOIN programs p ON p.id = a.program_id
            WHERE s.gym_id = $1 AND s.id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        row.map(|r| {
            Ok(SessionView {
                session: WorkoutSession {
                    id: WorkoutSessionId::from(r.id),
                    gym_id: GymId::from(r.gym_id),
                    athlete_id: UserId::from(r.athlete_id),
                    assignment_id: r.assignment_id.map(AssignmentId::from),
                    workout_template_id: r.workout_template_id.map(WorkoutTemplateId::from),
                    title: r.title,
                    started_at: r.started_at,
                    status: status_from_row(&r.status, r.completed_at, r.abandoned_at)?,
                    ended_at: r.ended_at,
                },
                athlete_name: r.athlete_name,
                workout_name: r.workout_name,
                program_name: r.program_name,
                set_count: r.set_count,
            })
        })
        .transpose()
    }

    async fn find_session(
        &self,
        tenant: &TenantContext,
        id: WorkoutSessionId,
    ) -> ApplicationResult<Option<WorkoutSession>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT id, gym_id, athlete_id, assignment_id, workout_template_id, title,
                   started_at, status, completed_at, abandoned_at, ended_at
            FROM workout_sessions WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        row.map(|r| {
            Ok(WorkoutSession {
                id: WorkoutSessionId::from(r.id),
                gym_id: GymId::from(r.gym_id),
                athlete_id: UserId::from(r.athlete_id),
                assignment_id: r.assignment_id.map(AssignmentId::from),
                workout_template_id: r.workout_template_id.map(WorkoutTemplateId::from),
                title: r.title,
                started_at: r.started_at,
                status: status_from_row(&r.status, r.completed_at, r.abandoned_at)?,
                ended_at: r.ended_at,
            })
        })
        .transpose()
    }

    async fn sets_of(
        &self,
        tenant: &TenantContext,
        session: WorkoutSessionId,
    ) -> ApplicationResult<Vec<PerformedSet>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT id, session_id, gym_id, exercise_id, template_exercise_id,
                   set_number, performed, rpe
            FROM performed_sets
            WHERE gym_id = $1 AND session_id = $2
            ORDER BY exercise_id, set_number
            "#,
            tenant.gym_id.into_uuid(),
            session.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                let performed: PerformedValue =
                    serde_json::from_value(r.performed).map_err(|e| {
                        ApplicationError::Internal(
                            format!("stored performed value could not be read: {e}").into(),
                        )
                    })?;

                Ok(PerformedSet {
                    id: PerformedSetId::from(r.id),
                    session_id: WorkoutSessionId::from(r.session_id),
                    gym_id: GymId::from(r.gym_id),
                    exercise_id: ExerciseId::from(r.exercise_id),
                    template_exercise_id: r.template_exercise_id.map(TemplateExerciseId::from),
                    set_number: r.set_number,
                    performed,
                    rpe: r.rpe.and_then(|v| u8::try_from(v).ok()),
                })
            })
            .collect()
    }

    async fn insert_session(
        &self,
        tenant: &TenantContext,
        session: &WorkoutSession,
    ) -> ApplicationResult<bool> {
        debug_assert_eq!(session.gym_id, tenant.gym_id);

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let inserted = sqlx::query!(
            r#"
            INSERT INTO workout_sessions
                (id, gym_id, athlete_id, assignment_id, workout_template_id,
                 title, started_at, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'in_progress')
            ON CONFLICT (id) DO NOTHING
            "#,
            session.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            session.athlete_id.into_uuid(),
            session
                .assignment_id
                .map(gym_domain::AssignmentId::into_uuid),
            session
                .workout_template_id
                .map(gym_domain::WorkoutTemplateId::into_uuid),
            session.title.as_deref(),
            session.started_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_triggers)?
        .rows_affected()
            > 0;

        if inserted {
            record_in_tx(
                &mut tx,
                tenant.gym_id,
                tenant.actor_id,
                "workout_session.started",
                "workout_session",
                Some(session.id.into_uuid()),
                serde_json::json!({ "started_at": session.started_at }),
            )
            .await
            .map_err(db_err)?;
        }

        tx.commit().await.map_err(db_err)?;
        Ok(inserted)
    }

    async fn insert_set(
        &self,
        tenant: &TenantContext,
        set: &PerformedSet,
    ) -> ApplicationResult<bool> {
        let performed = serde_json::to_value(&set.performed)
            .map_err(|e| ApplicationError::Internal(Box::new(e)))?;

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // No audit row per set: a workout is one auditable event (its session),
        // and thirty rows of "logged a set" would bury everything else.
        let inserted = sqlx::query!(
            r#"
            INSERT INTO performed_sets
                (id, session_id, gym_id, exercise_id, template_exercise_id,
                 set_number, performed, rpe)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO NOTHING
            "#,
            set.id.into_uuid(),
            set.session_id.into_uuid(),
            tenant.gym_id.into_uuid(),
            set.exercise_id.into_uuid(),
            set.template_exercise_id.map(TemplateExerciseId::into_uuid),
            set.set_number,
            performed,
            set.rpe.map(i16::from),
        )
        .execute(&mut *tx)
        .await
        .map_err(map_triggers)?
        .rows_affected()
            > 0;

        tx.commit().await.map_err(db_err)?;
        Ok(inserted)
    }

    async fn exercise_history(
        &self,
        tenant: &TenantContext,
        exercise: ExerciseId,
        athlete: UserId,
    ) -> ApplicationResult<Vec<gym_application::ports::ExerciseHistoryEntry>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // One query, ordered so the grouping below is a single pass. Sets from
        // abandoned and open sessions are included — a set that happened is
        // history regardless of how the session ended.
        let rows = sqlx::query!(
            r#"
            SELECT s.id AS session_id, s.started_at, s.status AS session_status,
                   ps.id, ps.session_id AS set_session_id, ps.gym_id, ps.exercise_id,
                   ps.template_exercise_id, ps.set_number, ps.performed, ps.rpe
            FROM performed_sets ps
            JOIN workout_sessions s ON s.id = ps.session_id
            WHERE ps.gym_id = $1 AND ps.exercise_id = $2 AND s.athlete_id = $3
            ORDER BY s.started_at ASC, ps.set_number ASC
            "#,
            tenant.gym_id.into_uuid(),
            exercise.into_uuid(),
            athlete.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        let mut entries: Vec<gym_application::ports::ExerciseHistoryEntry> = Vec::new();
        for r in rows {
            let performed: PerformedValue = serde_json::from_value(r.performed).map_err(|e| {
                ApplicationError::Internal(
                    format!("stored performed value could not be read: {e}").into(),
                )
            })?;
            let set = PerformedSet {
                id: PerformedSetId::from(r.id),
                session_id: WorkoutSessionId::from(r.set_session_id),
                gym_id: GymId::from(r.gym_id),
                exercise_id: ExerciseId::from(r.exercise_id),
                template_exercise_id: r.template_exercise_id.map(TemplateExerciseId::from),
                set_number: r.set_number,
                performed,
                rpe: r.rpe.and_then(|v| u8::try_from(v).ok()),
            };

            let session_id = WorkoutSessionId::from(r.session_id);
            match entries.last_mut() {
                Some(last) if last.session_id == session_id => last.sets.push(set),
                _ => entries.push(gym_application::ports::ExerciseHistoryEntry {
                    session_id,
                    started_at: r.started_at,
                    session_status: r.session_status,
                    sets: vec![set],
                }),
            }
        }

        Ok(entries)
    }

    async fn save_finished(
        &self,
        tenant: &TenantContext,
        session: &WorkoutSession,
        action: &'static str,
    ) -> ApplicationResult<()> {
        let (completed_at, abandoned_at) = match &session.status {
            SessionStatus::Completed { completed_at } => (Some(*completed_at), None),
            SessionStatus::Abandoned { abandoned_at } => (None, Some(*abandoned_at)),
            SessionStatus::InProgress => {
                return Err(ApplicationError::Internal(
                    "save_finished called with an open session".into(),
                ));
            }
        };

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let updated = sqlx::query!(
            r#"
            UPDATE workout_sessions
            SET status = $3, completed_at = $4, abandoned_at = $5, ended_at = $6
            WHERE gym_id = $1 AND id = $2 AND status = 'in_progress'
            "#,
            tenant.gym_id.into_uuid(),
            session.id.into_uuid(),
            session.status.as_str(),
            completed_at,
            abandoned_at,
            session.ended_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_triggers)?;

        if updated.rows_affected() == 0 {
            return Err(ApplicationError::Conflict(
                "this session has already been finished".to_owned(),
            ));
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            action,
            "workout_session",
            Some(session.id.into_uuid()),
            serde_json::json!({}),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }
}
