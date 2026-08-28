//! Check-ins: append-only, exactly like `audit_log` and `performed_sets` — a
//! scan is written once and never revised.

use async_trait::async_trait;
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{CheckInRepository, CheckInView},
};
use gym_domain::{CheckInId, TenantContext, UserId, checkin::CheckIn};

use crate::db::{DbPool, begin_tenant_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

#[derive(Debug, Clone)]
pub struct PgCheckInRepository {
    pool: DbPool,
}

impl PgCheckInRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CheckInRepository for PgCheckInRepository {
    async fn insert(&self, tenant: &TenantContext, checkin: &CheckIn) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO gym_checkins
                (id, gym_id, member_id, scanned_by, allowed, reason, scanned_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            checkin.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            checkin.member_id.into_uuid(),
            checkin.scanned_by.into_uuid(),
            checkin.allowed,
            checkin.reason,
            checkin.scanned_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)
    }

    async fn recent(&self, tenant: &TenantContext) -> ApplicationResult<Vec<CheckInView>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT c.id, c.member_id, c.scanned_by, c.allowed, c.reason, c.scanned_at,
                   u.display_name AS member_name
            FROM gym_checkins c
            JOIN users u ON u.id = c.member_id
            WHERE c.gym_id = $1
            ORDER BY c.scanned_at DESC, c.id DESC
            LIMIT 50
            "#,
            tenant.gym_id.into_uuid(),
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(CheckInView {
                    checkin: CheckIn::new(
                        CheckInId::from(r.id),
                        tenant.gym_id,
                        UserId::from(r.member_id),
                        UserId::from(r.scanned_by),
                        r.allowed,
                        &r.reason,
                        r.scanned_at,
                    )
                    .map_err(ApplicationError::Domain)?,
                    member_name: r.member_name,
                })
            })
            .collect()
    }
}
