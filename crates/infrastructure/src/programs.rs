//! Postgres adapter for programmes.
//!
//! Two things worth knowing before changing anything here.
//!
//! **Status is a sum type in Rust and columns in SQL.** `VersionStatus` carries
//! each state's evidence in its variant; the table spreads that across nullable
//! `*_at` / `*_by` columns with a CHECK keeping them consistent. The conversion
//! is in one place — `status_from_row` / `save_status` — so the mapping cannot
//! drift across call sites.
//!
//! **Content writes are also guarded by database triggers** (migration 0007).
//! The application checks first so the caller gets a clear message; the trigger
//! exists because this is not the only thing that can reach these tables.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{ProgramRepository, VersionContent},
};
use gym_domain::{
    GymId, ProgramId, ProgramVersionId, ProgramWeekId, TenantContext, UserId, WorkoutTemplateId,
    prescription::ExercisePrescription,
    program::{Program, ProgramVersion, VersionStatus},
    workout::{ProgramWeek, TemplateExercise, WorkoutTemplate},
};
use uuid::Uuid;

use crate::{audit::record_in_tx, db::begin_tenant_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

/// The CHECK constraint guarantees these four values; anything else is drift.
fn parse_focus(raw: &str) -> ApplicationResult<gym_domain::ProgramFocus> {
    gym_domain::ProgramFocus::parse(raw)
        .ok_or_else(|| ApplicationError::Internal(format!("unknown program focus '{raw}'").into()))
}

#[derive(Debug, Clone)]
pub struct PgProgramRepository {
    pool: crate::DbPool,
}

impl PgProgramRepository {
    #[must_use]
    pub const fn new(pool: crate::DbPool) -> Self {
        Self { pool }
    }
}

/// The evidence columns for a status, in table order.
///
/// `status_from_row` and `evidence_of` are inverses over this tuple, which is
/// what keeps the enum↔columns mapping honest: both directions read the same
/// shape, so adding a state to one and forgetting the other does not compile.
type Evidence = (
    Option<DateTime<Utc>>,
    Option<Uuid>,
    Option<DateTime<Utc>>,
    Option<Uuid>,
    Option<DateTime<Utc>>,
    Option<Uuid>,
    Option<DateTime<Utc>>,
    Option<Uuid>,
);

/// Rebuild the status enum from its columns.
///
/// The CHECK constraint guarantees the evidence is present for each status, so a
/// missing timestamp here means the constraint was bypassed — that is corruption,
/// not a user error, and it is reported as an internal failure rather than
/// silently downgraded to `Draft`.
fn status_from_row(status: &str, evidence: Evidence) -> ApplicationResult<VersionStatus> {
    let (
        submitted_at,
        submitted_by,
        approved_at,
        approved_by,
        published_at,
        published_by,
        archived_at,
        archived_by,
    ) = evidence;

    let corrupt = |what: &str| {
        ApplicationError::Internal(
            format!("program version is '{what}' but its evidence columns are null").into(),
        )
    };

    Ok(match status {
        "draft" => VersionStatus::Draft,
        "in_review" => VersionStatus::InReview {
            submitted_at: submitted_at.ok_or_else(|| corrupt("in_review"))?,
            submitted_by: UserId::from(submitted_by.ok_or_else(|| corrupt("in_review"))?),
        },
        "approved" => VersionStatus::Approved {
            approved_at: approved_at.ok_or_else(|| corrupt("approved"))?,
            approved_by: UserId::from(approved_by.ok_or_else(|| corrupt("approved"))?),
        },
        "published" => VersionStatus::Published {
            published_at: published_at.ok_or_else(|| corrupt("published"))?,
            published_by: UserId::from(published_by.ok_or_else(|| corrupt("published"))?),
        },
        "archived" => VersionStatus::Archived {
            archived_at: archived_at.ok_or_else(|| corrupt("archived"))?,
            archived_by: UserId::from(archived_by.ok_or_else(|| corrupt("archived"))?),
        },
        other => {
            return Err(ApplicationError::Internal(
                format!("unknown program version status '{other}'").into(),
            ));
        }
    })
}

fn evidence_of(status: &VersionStatus) -> Evidence {
    let mut e: Evidence = (None, None, None, None, None, None, None, None);
    match status {
        VersionStatus::Draft => {}
        VersionStatus::InReview {
            submitted_at,
            submitted_by,
        } => {
            e.0 = Some(*submitted_at);
            e.1 = Some(submitted_by.into_uuid());
        }
        VersionStatus::Approved {
            approved_at,
            approved_by,
        } => {
            e.2 = Some(*approved_at);
            e.3 = Some(approved_by.into_uuid());
        }
        VersionStatus::Published {
            published_at,
            published_by,
        } => {
            e.4 = Some(*published_at);
            e.5 = Some(published_by.into_uuid());
        }
        VersionStatus::Archived {
            archived_at,
            archived_by,
        } => {
            e.6 = Some(*archived_at);
            e.7 = Some(archived_by.into_uuid());
        }
    }
    e
}

/// Map the trigger's refusal onto a user-facing error.
///
/// Without this the caller sees "internal error" for what is actually a correct,
/// deliberate refusal to rewrite history — and would reasonably file it as a bug.
fn map_frozen(err: sqlx::Error) -> ApplicationError {
    if let sqlx::Error::Database(db) = &err {
        let message = db.message();
        if message.contains("program version") && message.contains("cannot modify")
            || message.contains("immutable")
        {
            return ApplicationError::Domain(gym_domain::DomainError::Invalid(
                "this programme version is no longer editable — create a new draft from it".into(),
            ));
        }
    }
    db_err(err)
}

#[async_trait]
impl ProgramRepository for PgProgramRepository {
    async fn list(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<(Program, ProgramVersion)>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // One row per programme, carrying its newest version. DISTINCT ON is the
        // Postgres idiom for "latest per group" and avoids the N+1 that listing
        // programmes then querying each one's versions would produce.
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT ON (p.id)
                   p.id AS program_id, p.gym_id, p.name, p.summary, p.focus,
                   p.created_by, p.created_at,
                   v.id AS version_id, v.version_number, v.status,
                   v.submitted_at, v.submitted_by, v.approved_at, v.approved_by,
                   v.published_at, v.published_by, v.archived_at, v.archived_by,
                   v.created_by AS version_created_by, v.created_at AS version_created_at,
                   v.derived_from
            FROM programs p
            JOIN program_versions v ON v.program_id = p.id
            WHERE p.gym_id = $1
            ORDER BY p.id, v.version_number DESC
            "#,
            tenant.gym_id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let program = Program {
                id: ProgramId::from(r.program_id),
                gym_id: GymId::from(r.gym_id),
                name: r.name,
                summary: r.summary,
                focus: parse_focus(&r.focus)?,
                created_by: UserId::from(r.created_by),
                created_at: r.created_at,
            };
            let version = ProgramVersion {
                id: ProgramVersionId::from(r.version_id),
                program_id: program.id,
                gym_id: program.gym_id,
                version_number: r.version_number,
                status: status_from_row(
                    &r.status,
                    (
                        r.submitted_at,
                        r.submitted_by,
                        r.approved_at,
                        r.approved_by,
                        r.published_at,
                        r.published_by,
                        r.archived_at,
                        r.archived_by,
                    ),
                )?,
                created_by: UserId::from(r.version_created_by),
                created_at: r.version_created_at,
                derived_from: r.derived_from.map(ProgramVersionId::from),
            };
            out.push((program, version));
        }

        // Newest programme first — a coach's most recent work is what they want.
        out.sort_by_key(|(program, _)| std::cmp::Reverse(program.created_at));
        Ok(out)
    }

    async fn find(
        &self,
        tenant: &TenantContext,
        id: ProgramId,
    ) -> ApplicationResult<Option<Program>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;
        let row = sqlx::query!(
            r#"
            SELECT id, gym_id, name, summary, focus, created_by, created_at
            FROM programs WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        row.map(|r| {
            Ok(Program {
                id: ProgramId::from(r.id),
                gym_id: GymId::from(r.gym_id),
                name: r.name,
                summary: r.summary,
                focus: parse_focus(&r.focus)?,
                created_by: UserId::from(r.created_by),
                created_at: r.created_at,
            })
        })
        .transpose()
    }

    async fn versions(
        &self,
        tenant: &TenantContext,
        program: ProgramId,
    ) -> ApplicationResult<Vec<ProgramVersion>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;
        let rows = sqlx::query!(
            r#"
            SELECT id, program_id, gym_id, version_number, status,
                   submitted_at, submitted_by, approved_at, approved_by,
                   published_at, published_by, archived_at, archived_by,
                   created_by, created_at, derived_from
            FROM program_versions
            WHERE gym_id = $1 AND program_id = $2
            ORDER BY version_number DESC
            "#,
            tenant.gym_id.into_uuid(),
            program.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(ProgramVersion {
                    id: ProgramVersionId::from(r.id),
                    program_id: ProgramId::from(r.program_id),
                    gym_id: GymId::from(r.gym_id),
                    version_number: r.version_number,
                    status: status_from_row(
                        &r.status,
                        (
                            r.submitted_at,
                            r.submitted_by,
                            r.approved_at,
                            r.approved_by,
                            r.published_at,
                            r.published_by,
                            r.archived_at,
                            r.archived_by,
                        ),
                    )?,
                    created_by: UserId::from(r.created_by),
                    created_at: r.created_at,
                    derived_from: r.derived_from.map(ProgramVersionId::from),
                })
            })
            .collect()
    }

    async fn find_version(
        &self,
        tenant: &TenantContext,
        id: ProgramVersionId,
    ) -> ApplicationResult<Option<ProgramVersion>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;
        let row = sqlx::query!(
            r#"
            SELECT id, program_id, gym_id, version_number, status,
                   submitted_at, submitted_by, approved_at, approved_by,
                   published_at, published_by, archived_at, archived_by,
                   created_by, created_at, derived_from
            FROM program_versions WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        row.map(|r| {
            Ok(ProgramVersion {
                id: ProgramVersionId::from(r.id),
                program_id: ProgramId::from(r.program_id),
                gym_id: GymId::from(r.gym_id),
                version_number: r.version_number,
                status: status_from_row(
                    &r.status,
                    (
                        r.submitted_at,
                        r.submitted_by,
                        r.approved_at,
                        r.approved_by,
                        r.published_at,
                        r.published_by,
                        r.archived_at,
                        r.archived_by,
                    ),
                )?,
                created_by: UserId::from(r.created_by),
                created_at: r.created_at,
                derived_from: r.derived_from.map(ProgramVersionId::from),
            })
        })
        .transpose()
    }

    async fn load_content(
        &self,
        tenant: &TenantContext,
        id: ProgramVersionId,
    ) -> ApplicationResult<Option<VersionContent>> {
        let Some(version) = self.find_version(tenant, id).await? else {
            return Ok(None);
        };

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let weeks = sqlx::query!(
            r#"
            SELECT id, version_id, week_number, label
            FROM program_weeks WHERE gym_id = $1 AND version_id = $2
            ORDER BY week_number
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        let workouts = sqlx::query!(
            r#"
            SELECT t.id, t.week_id, t.day_number, t.name, t.notes
            FROM workout_templates t
            JOIN program_weeks w ON w.id = t.week_id
            WHERE t.gym_id = $1 AND w.version_id = $2
            ORDER BY w.week_number, t.day_number
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        let exercises = sqlx::query!(
            r#"
            SELECT e.id, e.workout_id, e.exercise_id, e.position,
                   e.prescription, e.notes
            FROM workout_template_exercises e
            JOIN workout_templates t ON t.id = e.workout_id
            JOIN program_weeks w ON w.id = t.week_id
            WHERE e.gym_id = $1 AND w.version_id = $2
            ORDER BY w.week_number, t.day_number, e.position
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(Some(VersionContent {
            version,
            weeks: weeks
                .into_iter()
                .map(|r| ProgramWeek {
                    id: ProgramWeekId::from(r.id),
                    version_id: ProgramVersionId::from(r.version_id),
                    week_number: r.week_number,
                    label: r.label,
                })
                .collect(),
            workouts: workouts
                .into_iter()
                .map(|r| WorkoutTemplate {
                    id: WorkoutTemplateId::from(r.id),
                    week_id: ProgramWeekId::from(r.week_id),
                    day_number: r.day_number,
                    name: r.name,
                    notes: r.notes,
                })
                .collect(),
            exercises: exercises
                .into_iter()
                .map(|r| {
                    // The column is JSONB with a CHECK on its `kind`, but the
                    // full shape is only known to serde. A row that will not
                    // parse is corruption, not a user error.
                    let prescription: ExercisePrescription = serde_json::from_value(r.prescription)
                        .map_err(|e| {
                            ApplicationError::Internal(
                                format!("stored prescription could not be read: {e}").into(),
                            )
                        })?;

                    Ok(TemplateExercise {
                        id: gym_domain::TemplateExerciseId::from(r.id),
                        workout_id: WorkoutTemplateId::from(r.workout_id),
                        exercise_id: gym_domain::ExerciseId::from(r.exercise_id),
                        position: r.position,
                        prescription,
                        notes: r.notes,
                    })
                })
                .collect::<ApplicationResult<Vec<_>>>()?,
        }))
    }

    async fn insert_program(
        &self,
        tenant: &TenantContext,
        program: &Program,
        first_version: &ProgramVersion,
    ) -> ApplicationResult<()> {
        debug_assert_eq!(program.gym_id, tenant.gym_id);

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO programs (id, gym_id, name, summary, focus, created_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            program.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            program.name,
            program.summary.as_deref(),
            program.focus.as_str(),
            program.created_by.into_uuid(),
            program.created_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                ApplicationError::Conflict(format!("programme '{}'", program.name))
            }
            _ => db_err(e),
        })?;

        // Same transaction: a programme with no version is a state the rest of
        // the system does not know how to handle.
        sqlx::query!(
            r#"
            INSERT INTO program_versions
                (id, program_id, gym_id, version_number, status, created_by, created_at)
            VALUES ($1, $2, $3, $4, 'draft', $5, $6)
            "#,
            first_version.id.into_uuid(),
            program.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            first_version.version_number,
            first_version.created_by.into_uuid(),
            first_version.created_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "program.created",
            "program",
            Some(program.id.into_uuid()),
            serde_json::json!({ "name": program.name }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn insert_version(
        &self,
        tenant: &TenantContext,
        version: &ProgramVersion,
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO program_versions
                (id, program_id, gym_id, version_number, status, created_by, created_at, derived_from)
            VALUES ($1, $2, $3, $4, 'draft', $5, $6, $7)
            "#,
            version.id.into_uuid(),
            version.program_id.into_uuid(),
            tenant.gym_id.into_uuid(),
            version.version_number,
            version.created_by.into_uuid(),
            version.created_at,
            version.derived_from.map(ProgramVersionId::into_uuid),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            // The partial unique index allows only one open version per
            // programme, so this is "someone already started the next draft".
            sqlx::Error::Database(db) if db.is_unique_violation() => ApplicationError::Conflict(
                "an open draft already exists for this programme".to_owned(),
            ),
            _ => db_err(e),
        })?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "program_version.created",
            "program_version",
            Some(version.id.into_uuid()),
            serde_json::json!({ "version_number": version.version_number }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn save_status(
        &self,
        tenant: &TenantContext,
        version: &ProgramVersion,
        action: &'static str,
    ) -> ApplicationResult<()> {
        let e = evidence_of(&version.status);
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            UPDATE program_versions
            SET status = $3,
                submitted_at = $4, submitted_by = $5,
                approved_at  = $6, approved_by  = $7,
                published_at = $8, published_by = $9,
                archived_at  = $10, archived_by = $11
            WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            version.id.into_uuid(),
            version.status.as_str(),
            e.0,
            e.1,
            e.2,
            e.3,
            e.4,
            e.5,
            e.6,
            e.7,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_frozen)?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            action,
            "program_version",
            Some(version.id.into_uuid()),
            serde_json::json!({ "version_number": version.version_number }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn insert_week(
        &self,
        tenant: &TenantContext,
        week: &ProgramWeek,
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO program_weeks (id, version_id, gym_id, week_number, label)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            week.id.into_uuid(),
            week.version_id.into_uuid(),
            tenant.gym_id.into_uuid(),
            week.week_number,
            week.label.as_deref(),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                ApplicationError::Conflict(format!("week {}", week.week_number))
            }
            _ => map_frozen(e),
        })?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn find_week(
        &self,
        tenant: &TenantContext,
        id: ProgramWeekId,
    ) -> ApplicationResult<Option<ProgramWeek>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;
        let row = sqlx::query!(
            r#"
            SELECT id, version_id, week_number, label
            FROM program_weeks WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        Ok(row.map(|r| ProgramWeek {
            id: ProgramWeekId::from(r.id),
            version_id: ProgramVersionId::from(r.version_id),
            week_number: r.week_number,
            label: r.label,
        }))
    }

    async fn insert_workout(
        &self,
        tenant: &TenantContext,
        workout: &WorkoutTemplate,
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO workout_templates (id, week_id, gym_id, day_number, name, notes)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            workout.id.into_uuid(),
            workout.week_id.into_uuid(),
            tenant.gym_id.into_uuid(),
            workout.day_number,
            workout.name,
            workout.notes.as_deref(),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                ApplicationError::Conflict(format!("day {}", workout.day_number))
            }
            _ => map_frozen(e),
        })?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn find_workout(
        &self,
        tenant: &TenantContext,
        id: WorkoutTemplateId,
    ) -> ApplicationResult<Option<WorkoutTemplate>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;
        let row = sqlx::query!(
            r#"
            SELECT id, week_id, day_number, name, notes
            FROM workout_templates WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        Ok(row.map(|r| WorkoutTemplate {
            id: WorkoutTemplateId::from(r.id),
            week_id: ProgramWeekId::from(r.week_id),
            day_number: r.day_number,
            name: r.name,
            notes: r.notes,
        }))
    }

    async fn insert_template_exercise(
        &self,
        tenant: &TenantContext,
        exercise: &TemplateExercise,
    ) -> ApplicationResult<()> {
        let prescription = serde_json::to_value(&exercise.prescription)
            .map_err(|e| ApplicationError::Internal(Box::new(e)))?;

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO workout_template_exercises
                (id, workout_id, gym_id, exercise_id, position, prescription, notes)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            exercise.id.into_uuid(),
            exercise.workout_id.into_uuid(),
            tenant.gym_id.into_uuid(),
            exercise.exercise_id.into_uuid(),
            exercise.position,
            prescription,
            exercise.notes.as_deref(),
        )
        .execute(&mut *tx)
        .await
        .map_err(map_frozen)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn next_position(
        &self,
        tenant: &TenantContext,
        workout: WorkoutTemplateId,
    ) -> ApplicationResult<i32> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;
        let row = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(position), 0) AS "highest!"
            FROM workout_template_exercises
            WHERE gym_id = $1 AND workout_id = $2
            "#,
            tenant.gym_id.into_uuid(),
            workout.into_uuid()
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        Ok(row.highest.saturating_add(1))
    }

    async fn prescribed_exercise_count(
        &self,
        tenant: &TenantContext,
        version: ProgramVersionId,
    ) -> ApplicationResult<usize> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;
        // Joined down to the prescription rather than counting weeks: the whole
        // point of the gate is that a container with nothing in it is not a plan.
        let row = sqlx::query!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM workout_template_exercises e
            JOIN workout_templates wk ON wk.id = e.workout_id
            JOIN program_weeks w ON w.id = wk.week_id
            WHERE w.gym_id = $1 AND w.version_id = $2
            "#,
            tenant.gym_id.into_uuid(),
            version.into_uuid()
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        Ok(usize::try_from(row.count).unwrap_or(0))
    }
}
