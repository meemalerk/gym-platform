//! Postgres adapter for goals. The metric is JSONB, exactly as the domain enum
//! serialises — the prescription pattern, third use.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{GoalRepository, GoalView},
};
use gym_domain::{
    GoalId, GymId, TenantContext, UserId,
    goal::{Goal, GoalMetric, GoalStatus},
};
use uuid::Uuid;

use crate::{audit::record_in_tx, db::begin_tenant_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

#[derive(Debug, Clone)]
pub struct PgGoalRepository {
    pool: crate::DbPool,
}

impl PgGoalRepository {
    #[must_use]
    pub const fn new(pool: crate::DbPool) -> Self {
        Self { pool }
    }
}

fn status_from_row(
    status: &str,
    achieved_at: Option<DateTime<Utc>>,
    confirmed_by: Option<Uuid>,
    abandoned_at: Option<DateTime<Utc>>,
    abandoned_by: Option<Uuid>,
) -> ApplicationResult<GoalStatus> {
    let corrupt = |what: &str| {
        ApplicationError::Internal(format!("goal is '{what}' with no evidence").into())
    };

    match status {
        "active" => Ok(GoalStatus::Active),
        "achieved" => match (achieved_at, confirmed_by) {
            (Some(achieved_at), Some(confirmed_by)) => Ok(GoalStatus::Achieved {
                achieved_at,
                confirmed_by: UserId::from(confirmed_by),
            }),
            _ => Err(corrupt("achieved")),
        },
        "abandoned" => match (abandoned_at, abandoned_by) {
            (Some(abandoned_at), Some(abandoned_by)) => Ok(GoalStatus::Abandoned {
                abandoned_at,
                abandoned_by: UserId::from(abandoned_by),
            }),
            _ => Err(corrupt("abandoned")),
        },
        other => Err(ApplicationError::Internal(
            format!("unknown goal status '{other}'").into(),
        )),
    }
}

fn metric_from_json(value: serde_json::Value) -> ApplicationResult<GoalMetric> {
    serde_json::from_value(value).map_err(|e| {
        ApplicationError::Internal(format!("stored goal metric could not be read: {e}").into())
    })
}

#[async_trait]
impl GoalRepository for PgGoalRepository {
    async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<GoalView>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT g.id, g.gym_id, g.athlete_id, g.set_by, g.metric, g.target_date,
                   g.status, g.achieved_at, g.confirmed_by, g.abandoned_at, g.abandoned_by,
                   g.created_at, u.display_name AS athlete_name
            FROM goals g
            JOIN users u ON u.id = g.athlete_id
            WHERE g.gym_id = $1
            ORDER BY (g.status = 'active') DESC, g.created_at DESC
            "#,
            tenant.gym_id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(GoalView {
                    goal: Goal {
                        id: GoalId::from(r.id),
                        gym_id: GymId::from(r.gym_id),
                        athlete_id: UserId::from(r.athlete_id),
                        set_by: UserId::from(r.set_by),
                        metric: metric_from_json(r.metric)?,
                        target_date: r.target_date,
                        status: status_from_row(
                            &r.status,
                            r.achieved_at,
                            r.confirmed_by,
                            r.abandoned_at,
                            r.abandoned_by,
                        )?,
                        created_at: r.created_at,
                    },
                    athlete_name: r.athlete_name,
                })
            })
            .collect()
    }

    async fn find(&self, tenant: &TenantContext, id: GoalId) -> ApplicationResult<Option<Goal>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT id, gym_id, athlete_id, set_by, metric, target_date,
                   status, achieved_at, confirmed_by, abandoned_at, abandoned_by, created_at
            FROM goals WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        row.map(|r| {
            Ok(Goal {
                id: GoalId::from(r.id),
                gym_id: GymId::from(r.gym_id),
                athlete_id: UserId::from(r.athlete_id),
                set_by: UserId::from(r.set_by),
                metric: metric_from_json(r.metric)?,
                target_date: r.target_date,
                status: status_from_row(
                    &r.status,
                    r.achieved_at,
                    r.confirmed_by,
                    r.abandoned_at,
                    r.abandoned_by,
                )?,
                created_at: r.created_at,
            })
        })
        .transpose()
    }

    async fn insert(&self, tenant: &TenantContext, goal: &Goal) -> ApplicationResult<()> {
        debug_assert_eq!(goal.gym_id, tenant.gym_id);
        let metric = serde_json::to_value(&goal.metric)
            .map_err(|e| ApplicationError::Internal(Box::new(e)))?;

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO goals
                (id, gym_id, athlete_id, set_by, metric, target_date, status, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, 'active', $7)
            "#,
            goal.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            goal.athlete_id.into_uuid(),
            goal.set_by.into_uuid(),
            metric,
            goal.target_date,
            goal.created_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "goal.created",
            "goal",
            Some(goal.id.into_uuid()),
            serde_json::json!({ "athlete_id": goal.athlete_id.into_uuid() }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn save_closed(
        &self,
        tenant: &TenantContext,
        goal: &Goal,
        action: &'static str,
    ) -> ApplicationResult<()> {
        let (achieved_at, confirmed_by, abandoned_at, abandoned_by) = match &goal.status {
            GoalStatus::Achieved {
                achieved_at,
                confirmed_by,
            } => (
                Some(*achieved_at),
                Some(confirmed_by.into_uuid()),
                None,
                None,
            ),
            GoalStatus::Abandoned {
                abandoned_at,
                abandoned_by,
            } => (
                None,
                None,
                Some(*abandoned_at),
                Some(abandoned_by.into_uuid()),
            ),
            GoalStatus::Active => {
                return Err(ApplicationError::Internal(
                    "save_closed called with an active goal".into(),
                ));
            }
        };

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let updated = sqlx::query!(
            r#"
            UPDATE goals
            SET status = $3, achieved_at = $4, confirmed_by = $5,
                abandoned_at = $6, abandoned_by = $7
            WHERE gym_id = $1 AND id = $2 AND status = 'active'
            "#,
            tenant.gym_id.into_uuid(),
            goal.id.into_uuid(),
            goal.status.as_str(),
            achieved_at,
            confirmed_by,
            abandoned_at,
            abandoned_by,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        if updated.rows_affected() == 0 {
            return Err(ApplicationError::Conflict("this goal".to_owned()));
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            action,
            "goal",
            Some(goal.id.into_uuid()),
            serde_json::json!({ "athlete_id": goal.athlete_id.into_uuid() }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }
}
