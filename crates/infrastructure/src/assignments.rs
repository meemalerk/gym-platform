//! Postgres adapter for programme assignments.
//!
//! Same conventions as the other adapters: audit in the mutation's own
//! transaction, endings as compare-and-swap updates, no delete path at all —
//! performed workouts will reference these rows, and history that can vanish is
//! not history.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{AssignmentRepository, AssignmentView},
};
use gym_domain::{
    AssignmentId, GymId, ProgramId, ProgramVersionId, TenantContext, UserId,
    assignment::{AssignmentStatus, ProgramAssignment},
};
use uuid::Uuid;

use crate::{audit::record_in_tx, db::begin_tenant_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

#[derive(Debug, Clone)]
pub struct PgAssignmentRepository {
    pool: crate::DbPool,
}

impl PgAssignmentRepository {
    #[must_use]
    pub const fn new(pool: crate::DbPool) -> Self {
        Self { pool }
    }
}

/// Rebuild the status enum from its columns. Missing evidence means the CHECK
/// was bypassed — corruption, reported as such, never downgraded to `Active`
/// (which would resurrect a withdrawn assignment).
fn status_from_row(
    status: &str,
    completed_at: Option<DateTime<Utc>>,
    withdrawn_at: Option<DateTime<Utc>>,
    withdrawn_by: Option<Uuid>,
) -> ApplicationResult<AssignmentStatus> {
    let corrupt = |what: &str| {
        ApplicationError::Internal(
            format!("assignment is '{what}' but its evidence columns are null").into(),
        )
    };

    match status {
        "active" => Ok(AssignmentStatus::Active),
        "completed" => Ok(AssignmentStatus::Completed {
            completed_at: completed_at.ok_or_else(|| corrupt("completed"))?,
        }),
        "withdrawn" => match (withdrawn_at, withdrawn_by) {
            (Some(withdrawn_at), Some(withdrawn_by)) => Ok(AssignmentStatus::Withdrawn {
                withdrawn_at,
                withdrawn_by: UserId::from(withdrawn_by),
            }),
            _ => Err(corrupt("withdrawn")),
        },
        other => Err(ApplicationError::Internal(
            format!("unknown assignment status '{other}'").into(),
        )),
    }
}

/// Map the validity trigger's refusals onto errors a caller can act on.
fn map_validity(err: sqlx::Error) -> ApplicationError {
    if let sqlx::Error::Database(db) = &err {
        let message = db.message();
        if message.contains("only a published program version") {
            return ApplicationError::Domain(gym_domain::DomainError::Invalid(
                "only a published programme version can be assigned".into(),
            ));
        }
        if message.contains("does not belong to this gym") || message.contains("not found") {
            return ApplicationError::NotFound { entity: "member" };
        }
        if db.is_unique_violation() {
            return ApplicationError::Conflict(
                "this athlete is already on an active assignment of this programme".to_owned(),
            );
        }
    }
    db_err(err)
}

#[async_trait]
impl AssignmentRepository for PgAssignmentRepository {
    async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<AssignmentView>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Names and programme metadata joined here — an assignment list is read
        // far more often than it is written, and per-row lookups would be the
        // same N+1 the other list queries avoid.
        let rows = sqlx::query!(
            r#"
            SELECT a.id, a.gym_id, a.athlete_id, a.program_id, a.program_version_id,
                   a.assigned_by, a.start_date, a.status,
                   a.completed_at, a.withdrawn_at, a.withdrawn_by, a.created_at,
                   u.display_name AS athlete_name,
                   p.name AS program_name,
                   v.version_number
            FROM program_assignments a
            JOIN users u ON u.id = a.athlete_id
            JOIN programs p ON p.id = a.program_id
            JOIN program_versions v ON v.id = a.program_version_id
            WHERE a.gym_id = $1
            ORDER BY (a.status = 'active') DESC, a.created_at DESC
            "#,
            tenant.gym_id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(AssignmentView {
                    assignment: ProgramAssignment {
                        id: AssignmentId::from(r.id),
                        gym_id: GymId::from(r.gym_id),
                        athlete_id: UserId::from(r.athlete_id),
                        program_id: ProgramId::from(r.program_id),
                        program_version_id: ProgramVersionId::from(r.program_version_id),
                        assigned_by: UserId::from(r.assigned_by),
                        start_date: r.start_date,
                        status: status_from_row(
                            &r.status,
                            r.completed_at,
                            r.withdrawn_at,
                            r.withdrawn_by,
                        )?,
                        created_at: r.created_at,
                    },
                    athlete_name: r.athlete_name,
                    program_name: r.program_name,
                    version_number: r.version_number,
                })
            })
            .collect()
    }

    async fn find(
        &self,
        tenant: &TenantContext,
        id: AssignmentId,
    ) -> ApplicationResult<Option<ProgramAssignment>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT id, gym_id, athlete_id, program_id, program_version_id,
                   assigned_by, start_date, status,
                   completed_at, withdrawn_at, withdrawn_by, created_at
            FROM program_assignments WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        row.map(|r| {
            Ok(ProgramAssignment {
                id: AssignmentId::from(r.id),
                gym_id: GymId::from(r.gym_id),
                athlete_id: UserId::from(r.athlete_id),
                program_id: ProgramId::from(r.program_id),
                program_version_id: ProgramVersionId::from(r.program_version_id),
                assigned_by: UserId::from(r.assigned_by),
                start_date: r.start_date,
                status: status_from_row(&r.status, r.completed_at, r.withdrawn_at, r.withdrawn_by)?,
                created_at: r.created_at,
            })
        })
        .transpose()
    }

    async fn insert(
        &self,
        tenant: &TenantContext,
        assignment: &ProgramAssignment,
    ) -> ApplicationResult<()> {
        debug_assert_eq!(assignment.gym_id, tenant.gym_id);

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO program_assignments
                (id, gym_id, athlete_id, program_id, program_version_id,
                 assigned_by, start_date, status, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'active', $8)
            "#,
            assignment.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            assignment.athlete_id.into_uuid(),
            assignment.program_id.into_uuid(),
            assignment.program_version_id.into_uuid(),
            assignment.assigned_by.into_uuid(),
            assignment.start_date,
            assignment.created_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_validity)?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "program.assigned",
            "program_assignment",
            Some(assignment.id.into_uuid()),
            serde_json::json!({
                "athlete_id": assignment.athlete_id.into_uuid(),
                "program_version_id": assignment.program_version_id.into_uuid(),
                "start_date": assignment.start_date,
            }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn save_ended(
        &self,
        tenant: &TenantContext,
        assignment: &ProgramAssignment,
        action: &'static str,
    ) -> ApplicationResult<()> {
        let (completed_at, withdrawn_at, withdrawn_by) = match &assignment.status {
            AssignmentStatus::Completed { completed_at } => (Some(*completed_at), None, None),
            AssignmentStatus::Withdrawn {
                withdrawn_at,
                withdrawn_by,
            } => (None, Some(*withdrawn_at), Some(withdrawn_by.into_uuid())),
            AssignmentStatus::Active => {
                return Err(ApplicationError::Internal(
                    "save_ended called with an active assignment".into(),
                ));
            }
        };

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Conditional on still being active — two concurrent withdrawals cannot
        // both claim to have done it. Same shape as refresh-token revocation.
        let updated = sqlx::query!(
            r#"
            UPDATE program_assignments
            SET status = $3, completed_at = $4, withdrawn_at = $5, withdrawn_by = $6
            WHERE gym_id = $1 AND id = $2 AND status = 'active'
            "#,
            tenant.gym_id.into_uuid(),
            assignment.id.into_uuid(),
            assignment.status.as_str(),
            completed_at,
            withdrawn_at,
            withdrawn_by,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        if updated.rows_affected() == 0 {
            return Err(ApplicationError::Conflict(
                "this assignment has already ended".to_owned(),
            ));
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            action,
            "program_assignment",
            Some(assignment.id.into_uuid()),
            serde_json::json!({ "athlete_id": assignment.athlete_id.into_uuid() }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }
}
