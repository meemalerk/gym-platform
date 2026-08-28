//! Database connection, migrations, and tenant-scoped transactions.

use std::time::Duration;

use gym_domain::{TenantContext, UserId};
use sqlx::{
    Postgres, Transaction,
    postgres::{PgPoolOptions, Postgres as Pg},
};

pub type DbPool = sqlx::Pool<Pg>;
pub type DbTx<'a> = Transaction<'a, Postgres>;

/// Connect with a bounded pool and sane timeouts.
pub async fn connect(database_url: &str, max_connections: u32) -> Result<DbPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .connect(database_url)
        .await
}

/// Apply migrations embedded at compile time from `/migrations`.
///
/// Run with a **privileged** connection (the table owner). The runtime pool uses
/// the unprivileged `gym_app` role instead — see `connect_app`.
pub async fn run_migrations(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}

/// Begin a transaction carrying **tenant** context for RLS.
///
/// Two things make this correct, and both are easy to get wrong:
///
/// 1. `set_config(..., true)` is **transaction-scoped**. A session-level `SET`
///    would leak the tenant to the next request that borrows this pooled
///    connection — the classic multi-tenant data leak.
/// 2. It only has any effect because the pool connects as a **non-owner** role.
///    Table owners and superusers bypass RLS silently.
///
/// The caller must `commit()` (or drop to roll back).
pub async fn begin_tenant_tx<'a>(
    pool: &DbPool,
    tenant: &TenantContext,
) -> Result<DbTx<'a>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query("SELECT set_config('app.current_gym', $1, true)")
        .bind(tenant.gym_id.into_uuid().to_string())
        .execute(&mut *tx)
        .await?;

    sqlx::query("SELECT set_config('app.current_user', $1, true)")
        .bind(tenant.actor_id.into_uuid().to_string())
        .execute(&mut *tx)
        .await?;

    Ok(tx)
}

/// Begin a transaction carrying only **user** context.
///
/// Needed for the cross-gym queries that run *before* a tenant is known — e.g.
/// resolving which gyms you belong to, which is exactly what `TenantScope` must
/// do to establish tenant context in the first place.
pub async fn begin_user_tx<'a>(pool: &DbPool, user: UserId) -> Result<DbTx<'a>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query("SELECT set_config('app.current_user', $1, true)")
        .bind(user.into_uuid().to_string())
        .execute(&mut *tx)
        .await?;

    Ok(tx)
}

/// Begin a transaction that pins tenant context to a gym being **created**.
///
/// Sign-up is the one flow with no pre-existing tenant: the gym does not exist
/// until this transaction inserts it, yet the capacity insert is checked against
/// `app.current_gym`. Setting the context to the gym we are creating is correct
/// and stays inside the policy rather than working around it.
pub async fn begin_provisioning_tx<'a>(
    pool: &DbPool,
    gym: gym_domain::GymId,
    owner: UserId,
) -> Result<DbTx<'a>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query("SELECT set_config('app.current_gym', $1, true)")
        .bind(gym.into_uuid().to_string())
        .execute(&mut *tx)
        .await?;

    sqlx::query("SELECT set_config('app.current_user', $1, true)")
        .bind(owner.into_uuid().to_string())
        .execute(&mut *tx)
        .await?;

    Ok(tx)
}
