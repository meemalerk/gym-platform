//! Audit logging.
//!
//! The write helper takes the **transaction** rather than the pool on purpose:
//! an audit entry must commit with the change it describes, or a failure between
//! the two leaves a mutation with no trace.

use async_trait::async_trait;
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{AuditEntry, AuditRecord, AuditRepository},
};
use gym_domain::{GymId, TenantContext, UserId};
use sqlx::{Postgres, Transaction};

use crate::db::{DbPool, begin_tenant_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

/// Record an action **inside an existing transaction**.
///
/// Callers already hold a tenant-scoped transaction (which is what satisfies the
/// RLS policy), so this cannot be called without tenant context — by construction.
pub async fn record_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    gym_id: GymId,
    actor_id: UserId,
    action: &str,
    entity_type: &str,
    entity_id: Option<uuid::Uuid>,
    metadata: serde_json::Value,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO audit_log (id, gym_id, actor_id, action, entity_type, entity_id, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        uuid::Uuid::now_v7(),
        gym_id.into_uuid(),
        actor_id.into_uuid(),
        action,
        entity_type,
        entity_id,
        metadata,
    )
    .execute(&mut **tx)
    .await
    .map(|_| ())
}

#[derive(Debug, Clone)]
pub struct PgAuditRepository {
    pool: DbPool,
}

impl PgAuditRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditRepository for PgAuditRepository {
    async fn recent(
        &self,
        tenant: &TenantContext,
        limit: i64,
    ) -> ApplicationResult<Vec<AuditRecord>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT a.id, a.action, a.entity_type, a.entity_id, a.metadata,
                   a.occurred_at, u.display_name AS "actor_name?"
            FROM audit_log a
            LEFT JOIN users u ON u.id = a.actor_id
            WHERE a.gym_id = $1
            ORDER BY a.occurred_at DESC, a.id DESC
            LIMIT $2
            "#,
            tenant.gym_id.into_uuid(),
            limit.clamp(1, 200),
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| AuditRecord {
                id: r.id,
                action: r.action,
                entity_type: r.entity_type,
                entity_id: r.entity_id,
                actor_name: r.actor_name,
                metadata: r.metadata,
                occurred_at: r.occurred_at,
            })
            .collect())
    }

    /// Standalone write, for the rare action with no surrounding transaction.
    ///
    /// Prefer `record_in_tx`: this cannot be atomic with anything.
    async fn record(&self, tenant: &TenantContext, entry: AuditEntry) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            &entry.action,
            &entry.entity_type,
            entry.entity_id,
            entry.metadata,
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }
}
