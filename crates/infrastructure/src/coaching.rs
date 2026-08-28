//! Postgres adapter for coach–athlete relationships.
//!
//! Note what is absent: there is no delete. Ending a relationship is an UPDATE,
//! and the app role has no DELETE grant on this table (migration 0009) — the
//! record of who was accountable for past coaching decisions has to survive the
//! relationship itself.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{CoachRelationshipView, CoachRepository},
};
use gym_domain::{
    CoachRelationshipId, GymId, TenantContext, UserId,
    coaching::{CoachRelationship, RelationshipStatus},
};
use uuid::Uuid;

use crate::{audit::record_in_tx, db::begin_tenant_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

#[derive(Debug, Clone)]
pub struct PgCoachRepository {
    pool: crate::DbPool,
}

impl PgCoachRepository {
    #[must_use]
    pub const fn new(pool: crate::DbPool) -> Self {
        Self { pool }
    }
}

/// Rebuild the status enum from its columns.
///
/// The CHECK constraint guarantees the evidence is present, so missing data here
/// means the constraint was bypassed — corruption, not a user error, and it is
/// reported as such rather than silently downgraded to `Active`, which would
/// resurrect access that had been revoked.
fn status_from_row(
    status: &str,
    ended_at: Option<DateTime<Utc>>,
    ended_by: Option<Uuid>,
) -> ApplicationResult<RelationshipStatus> {
    match status {
        "active" => Ok(RelationshipStatus::Active),
        "ended" => match (ended_at, ended_by) {
            (Some(ended_at), Some(ended_by)) => Ok(RelationshipStatus::Ended {
                ended_at,
                ended_by: UserId::from(ended_by),
            }),
            _ => Err(ApplicationError::Internal(
                "coach relationship is 'ended' but has no end evidence".into(),
            )),
        },
        other => Err(ApplicationError::Internal(
            format!("unknown coach relationship status '{other}'").into(),
        )),
    }
}

/// Turn the membership trigger's refusal into something a caller can act on.
fn map_membership(err: sqlx::Error) -> ApplicationError {
    if let sqlx::Error::Database(db) = &err {
        let message = db.message();
        if message.contains("does not belong to this gym") {
            return ApplicationError::NotFound { entity: "member" };
        }
        if db.is_unique_violation() {
            return ApplicationError::Conflict(
                "these two already have an active coaching relationship".to_owned(),
            );
        }
    }
    db_err(err)
}

#[async_trait]
impl CoachRepository for PgCoachRepository {
    async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<CoachRelationshipView>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Names joined here rather than fetched per row: a client list is the
        // archetypal N+1, and 12 clients would otherwise be 13 round trips.
        let rows = sqlx::query!(
            r#"
            SELECT r.id, r.gym_id, r.coach_id, r.athlete_id, r.status,
                   r.started_at, r.ended_at, r.ended_by, r.created_by,
                   c.display_name AS coach_name,
                   a.display_name AS athlete_name
            FROM coach_relationships r
            JOIN users c ON c.id = r.coach_id
            JOIN users a ON a.id = r.athlete_id
            WHERE r.gym_id = $1
            ORDER BY (r.status = 'active') DESC, r.started_at DESC
            "#,
            tenant.gym_id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(CoachRelationshipView {
                    relationship: CoachRelationship {
                        id: CoachRelationshipId::from(r.id),
                        gym_id: GymId::from(r.gym_id),
                        coach_id: UserId::from(r.coach_id),
                        athlete_id: UserId::from(r.athlete_id),
                        status: status_from_row(&r.status, r.ended_at, r.ended_by)?,
                        started_at: r.started_at,
                        created_by: UserId::from(r.created_by),
                    },
                    coach_name: r.coach_name,
                    athlete_name: r.athlete_name,
                })
            })
            .collect()
    }

    async fn active_for_user(
        &self,
        tenant: &TenantContext,
        user: UserId,
    ) -> ApplicationResult<Vec<CoachRelationship>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT id, gym_id, coach_id, athlete_id, started_at, created_by
            FROM coach_relationships
            WHERE gym_id = $1 AND status = 'active'
              AND (coach_id = $2 OR athlete_id = $2)
            "#,
            tenant.gym_id.into_uuid(),
            user.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| CoachRelationship {
                id: CoachRelationshipId::from(r.id),
                gym_id: GymId::from(r.gym_id),
                coach_id: UserId::from(r.coach_id),
                athlete_id: UserId::from(r.athlete_id),
                // The WHERE clause already restricted to active rows.
                status: RelationshipStatus::Active,
                started_at: r.started_at,
                created_by: UserId::from(r.created_by),
            })
            .collect())
    }

    async fn find(
        &self,
        tenant: &TenantContext,
        id: CoachRelationshipId,
    ) -> ApplicationResult<Option<CoachRelationship>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT id, gym_id, coach_id, athlete_id, status,
                   started_at, ended_at, ended_by, created_by
            FROM coach_relationships WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        row.map(|r| {
            Ok(CoachRelationship {
                id: CoachRelationshipId::from(r.id),
                gym_id: GymId::from(r.gym_id),
                coach_id: UserId::from(r.coach_id),
                athlete_id: UserId::from(r.athlete_id),
                status: status_from_row(&r.status, r.ended_at, r.ended_by)?,
                started_at: r.started_at,
                created_by: UserId::from(r.created_by),
            })
        })
        .transpose()
    }

    async fn insert(
        &self,
        tenant: &TenantContext,
        relationship: &CoachRelationship,
    ) -> ApplicationResult<()> {
        debug_assert_eq!(relationship.gym_id, tenant.gym_id);

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO coach_relationships
                (id, gym_id, coach_id, athlete_id, status, started_at, created_by)
            VALUES ($1, $2, $3, $4, 'active', $5, $6)
            "#,
            relationship.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            relationship.coach_id.into_uuid(),
            relationship.athlete_id.into_uuid(),
            relationship.started_at,
            relationship.created_by.into_uuid(),
        )
        .execute(&mut *tx)
        .await
        .map_err(map_membership)?;

        // Who may see whose data is exactly the kind of change an audit trail
        // exists for, so it is recorded in the same transaction.
        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "coach_relationship.created",
            "coach_relationship",
            Some(relationship.id.into_uuid()),
            serde_json::json!({
                "coach_id": relationship.coach_id.into_uuid(),
                "athlete_id": relationship.athlete_id.into_uuid(),
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
        relationship: &CoachRelationship,
    ) -> ApplicationResult<()> {
        let (ended_at, ended_by) = match &relationship.status {
            RelationshipStatus::Ended { ended_at, ended_by } => (*ended_at, ended_by.into_uuid()),
            RelationshipStatus::Active => {
                return Err(ApplicationError::Internal(
                    "save_ended called with an active relationship".into(),
                ));
            }
        };

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Conditional on still being active, so two concurrent "end" requests
        // cannot both claim to have done it — the same compare-and-swap shape as
        // refresh-token revocation.
        let updated = sqlx::query!(
            r#"
            UPDATE coach_relationships
            SET status = 'ended', ended_at = $3, ended_by = $4
            WHERE gym_id = $1 AND id = $2 AND status = 'active'
            "#,
            tenant.gym_id.into_uuid(),
            relationship.id.into_uuid(),
            ended_at,
            ended_by,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        if updated.rows_affected() == 0 {
            return Err(ApplicationError::Conflict(
                "this coaching relationship has already ended".to_owned(),
            ));
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "coach_relationship.ended",
            "coach_relationship",
            Some(relationship.id.into_uuid()),
            serde_json::json!({
                "coach_id": relationship.coach_id.into_uuid(),
                "athlete_id": relationship.athlete_id.into_uuid(),
            }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }
}
