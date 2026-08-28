//! Postgres repository adapters.
//!
//! Every tenant-owned query filters on `gym_id` from the `TenantContext` — the
//! application-level half of ADR-0004's defence in depth (RLS is the other half).

use async_trait::async_trait;
use gym_domain::{
    Capabilities, Capacity, ExerciseId, GymId, MembershipId, SessionId, TenantContext, UserId,
    exercise::{CatalogueStatus, Exercise, Modality},
    gym::Gym,
    user::{Email, User},
};

use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{
        ExerciseRepository, GymMember, GymRepository, Membership, NewSession, SessionRecord,
        SessionRepository, StoredUser, UserRepository,
    },
};

use crate::audit::record_in_tx;
use crate::db::{DbPool, begin_provisioning_tx, begin_tenant_tx, begin_user_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

// ---------------------------------------------------------------- exercises

#[derive(Debug, Clone)]
pub struct PgExerciseRepository {
    pool: DbPool,
}

impl PgExerciseRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

/// Row shape as stored. Kept private so DB representation never leaks to the API.
struct ExerciseRow {
    id: uuid::Uuid,
    gym_id: uuid::Uuid,
    name: String,
    modality: String,
    notes: Option<String>,
    status: String,
    /// NULL for rows written before ADR-0024 — see the migration's comment on
    /// why that is not back-filled.
    proposed_by: Option<uuid::Uuid>,
}

impl TryFrom<ExerciseRow> for Exercise {
    type Error = ApplicationError;

    fn try_from(row: ExerciseRow) -> Result<Self, Self::Error> {
        let modality = Modality::parse(&row.modality).ok_or_else(|| {
            ApplicationError::Internal(
                format!("unknown modality in database: {}", row.modality).into(),
            )
        })?;

        let status = CatalogueStatus::parse(&row.status).ok_or_else(|| {
            ApplicationError::Internal(
                format!("unknown catalogue status in database: {}", row.status).into(),
            )
        })?;

        Ok(Self {
            id: ExerciseId::from(row.id),
            gym_id: GymId::from(row.gym_id),
            name: row.name,
            modality,
            notes: row.notes,
            status,
            // A pre-ADR-0024 row has no recorded author. The domain wants a
            // UserId, so use the nil UUID and let the curation queue read it
            // as "nobody to chase" — which is the truth, not a placeholder
            // standing in for a person we could have named.
            proposed_by: row
                .proposed_by
                .map_or_else(|| UserId::from(uuid::Uuid::nil()), UserId::from),
        })
    }
}

#[async_trait]
impl ExerciseRepository for PgExerciseRepository {
    async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<Exercise>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;
        let rows = sqlx::query_as!(
            ExerciseRow,
            r#"
            SELECT id, gym_id, name, modality, notes, status, proposed_by
            FROM exercises
            WHERE gym_id = $1
            ORDER BY name
            "#,
            tenant.gym_id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        rows.into_iter().map(Exercise::try_from).collect()
    }

    async fn find(
        &self,
        tenant: &TenantContext,
        id: ExerciseId,
    ) -> ApplicationResult<Option<Exercise>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;
        let row = sqlx::query_as!(
            ExerciseRow,
            r#"
            SELECT id, gym_id, name, modality, notes, status, proposed_by
            FROM exercises
            WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        row.map(Exercise::try_from).transpose()
    }

    async fn insert(&self, tenant: &TenantContext, exercise: &Exercise) -> ApplicationResult<()> {
        // Guard against a caller building an Exercise for a different tenant.
        debug_assert_eq!(exercise.gym_id, tenant.gym_id);

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;
        sqlx::query!(
            r#"
            INSERT INTO exercises (id, gym_id, name, modality, notes, status, proposed_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            exercise.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            exercise.name,
            exercise.modality.as_str(),
            exercise.notes.as_deref(),
            exercise.status.as_str(),
            exercise.proposed_by.into_uuid(),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                ApplicationError::Conflict(format!("exercise '{}'", exercise.name))
            }
            _ => db_err(e),
        })?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "exercise.created",
            "exercise",
            Some(exercise.id.into_uuid()),
            serde_json::json!({ "name": exercise.name, "status": exercise.status.as_str() }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(())
    }

    async fn save_status(
        &self,
        tenant: &TenantContext,
        exercise: &Exercise,
        action: &'static str,
    ) -> ApplicationResult<()> {
        debug_assert_eq!(exercise.gym_id, tenant.gym_id);

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Status only. Name and modality are deliberately NOT updatable here:
        // renaming a movement athletes already have history against is a
        // different, riskier operation than curating one, and folding it into
        // this call would make it accidental.
        sqlx::query!(
            r#"
            UPDATE exercises
               SET status = $3
             WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            exercise.id.into_uuid(),
            exercise.status.as_str(),
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            action,
            "exercise",
            Some(exercise.id.into_uuid()),
            serde_json::json!({ "name": exercise.name, "status": exercise.status.as_str() }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(())
    }
}

// -------------------------------------------------------------------- users

#[derive(Debug, Clone)]
pub struct PgUserRepository {
    pool: DbPool,
}

impl PgUserRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

fn build_user(
    id: uuid::Uuid,
    email: &str,
    display_name: String,
    email_verified_at: Option<chrono::DateTime<chrono::Utc>>,
) -> ApplicationResult<User> {
    Ok(User {
        id: UserId::from(id),
        email: Email::parse(email)
            .map_err(|_| ApplicationError::Internal("invalid email stored in database".into()))?,
        display_name,
        email_verified_at,
    })
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn find_by_email(&self, email: &Email) -> ApplicationResult<Option<StoredUser>> {
        let row = sqlx::query!(
            r#"
            SELECT id, email, display_name, password_hash, email_verified_at
            FROM users
            WHERE email = $1
            "#,
            email.as_str()
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        row.map(|r| {
            Ok(StoredUser {
                user: build_user(r.id, &r.email, r.display_name, r.email_verified_at)?,
                password_hash: r.password_hash,
            })
        })
        .transpose()
    }

    async fn find_by_id(&self, id: UserId) -> ApplicationResult<Option<User>> {
        let row = sqlx::query!(
            r#"SELECT id, email, display_name, email_verified_at FROM users WHERE id = $1"#,
            id.into_uuid()
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        row.map(|r| build_user(r.id, &r.email, r.display_name, r.email_verified_at))
            .transpose()
    }

    async fn set_password_hash(&self, user: UserId, password_hash: &str) -> ApplicationResult<()> {
        sqlx::query!(
            r#"UPDATE users SET password_hash = $2 WHERE id = $1"#,
            user.into_uuid(),
            password_hash,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)
        .map(|_| ())
    }

    async fn mark_email_verified(
        &self,
        user: UserId,
        at: chrono::DateTime<chrono::Utc>,
    ) -> ApplicationResult<()> {
        // COALESCE, so confirming twice keeps the FIRST date — that is when it
        // actually happened, and overwriting it would quietly rewrite history
        // every time someone clicked an old link.
        sqlx::query!(
            r#"UPDATE users SET email_verified_at = COALESCE(email_verified_at, $2) WHERE id = $1"#,
            user.into_uuid(),
            at,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)
        .map(|_| ())
    }

    async fn insert(&self, user: &User, password_hash: &str) -> ApplicationResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO users (id, email, display_name, password_hash)
            VALUES ($1, $2, $3, $4)
            "#,
            user.id.into_uuid(),
            user.email.as_str(),
            user.display_name,
            password_hash,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                ApplicationError::Conflict("email".to_owned())
            }
            _ => db_err(e),
        })?;

        Ok(())
    }

    async fn memberships(&self, user: UserId) -> ApplicationResult<Vec<Membership>> {
        let mut tx = begin_user_tx(&self.pool, user).await.map_err(db_err)?;
        // Still queries potentially many rows even under a single-gym deployment
        // (ADR-0023): `SINGLE_GYM_MODE` caps gym CREATION, not how many existing
        // gyms an account can hold capacities in, and the verification suites
        // rely on that to prove tenant isolation. One row per gym, capacities
        // aggregated — a person may hold several in the same gym (ADR-0014).
        // Joins `gyms` so a client can render a switcher without N extra
        // round-trips. Readable across gyms thanks to the `gyms_read_member`
        // policy — membership is what grants it.
        let rows = sqlx::query!(
            r#"
            SELECT c.gym_id,
                   g.name AS gym_name,
                   g.is_personal,
                   array_agg(c.capacity ORDER BY c.capacity) AS "capacities!: Vec<String>"
            FROM gym_capacities c
            JOIN gyms g ON g.id = c.gym_id
            WHERE c.user_id = $1 AND c.revoked_at IS NULL
            GROUP BY c.gym_id, g.name, g.is_personal
            ORDER BY g.is_personal, g.name
            "#,
            user.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(Membership {
                    gym_id: GymId::from(r.gym_id),
                    gym_name: r.gym_name,
                    is_personal: r.is_personal,
                    capabilities: parse_capacities(&r.capacities)?,
                })
            })
            .collect()
    }

    async fn capabilities_in(&self, user: UserId, gym: GymId) -> ApplicationResult<Capabilities> {
        // User context, not tenant context: this call is what ESTABLISHES the
        // tenant, so it cannot presuppose it. The gym_capacities read policy
        // permits `user_id = app_current_user()` exactly for this.
        let mut tx = begin_user_tx(&self.pool, user).await.map_err(db_err)?;
        let rows = sqlx::query_scalar!(
            r#"
            SELECT capacity
            FROM gym_capacities
            WHERE user_id = $1 AND gym_id = $2 AND revoked_at IS NULL
            "#,
            user.into_uuid(),
            gym.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        parse_capacities(&rows)
    }

    async fn rename(&self, user: UserId, display_name: &str) -> ApplicationResult<()> {
        // Person-scoped write; the service validated the name. No audit row —
        // this is the account's own name, not a tenant mutation, and it would
        // need a gym to attribute the entry to.
        let mut tx = begin_user_tx(&self.pool, user).await.map_err(db_err)?;
        sqlx::query!(
            "UPDATE users SET display_name = $2 WHERE id = $1",
            user.into_uuid(),
            display_name,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn roster(&self, tenant: &TenantContext) -> ApplicationResult<Vec<GymMember>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Capacities aggregated in SQL rather than one query per person: a gym
        // roster is exactly where an N+1 turns a fast screen into a slow one, and
        // it grows with the gym.
        let rows = sqlx::query!(
            r#"
            SELECT u.id, u.display_name,
                   array_agg(c.capacity ORDER BY c.capacity) AS "capacities!: Vec<String>"
            FROM gym_capacities c
            JOIN users u ON u.id = c.user_id
            WHERE c.gym_id = $1 AND c.revoked_at IS NULL
            GROUP BY u.id, u.display_name
            ORDER BY u.display_name
            "#,
            tenant.gym_id.into_uuid()
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(GymMember {
                    user_id: UserId::from(r.id),
                    display_name: r.display_name,
                    capabilities: parse_capacities(&r.capacities)?,
                })
            })
            .collect()
    }

    async fn set_capacities(
        &self,
        tenant: &TenantContext,
        target: UserId,
        capacities: &[Capacity],
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let wanted: Vec<String> = capacities.iter().map(|c| c.as_str().to_owned()).collect();

        // Revoke first, then grant, in one transaction. Anything they hold
        // that is not in the new set is stamped rather than deleted — an audit
        // trail that cannot answer "what could they do last Tuesday" is not
        // much of an audit trail.
        sqlx::query!(
            r#"
            UPDATE gym_capacities
            SET revoked_at = now()
            WHERE gym_id = $1 AND user_id = $2 AND revoked_at IS NULL
              AND capacity <> ALL($3)
            "#,
            tenant.gym_id.into_uuid(),
            target.into_uuid(),
            &wanted,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        for capacity in capacities {
            // Un-revoke rather than insert a second row when they held this
            // before: otherwise re-granting a capacity somebody once had
            // silently accumulates rows that all read as "granted now".
            let restored = sqlx::query!(
                r#"
                UPDATE gym_capacities
                SET revoked_at = NULL, granted_by = $4
                WHERE gym_id = $1 AND user_id = $2 AND capacity = $3
                "#,
                tenant.gym_id.into_uuid(),
                target.into_uuid(),
                capacity.as_str(),
                tenant.actor_id.into_uuid(),
            )
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;

            if restored.rows_affected() == 0 {
                sqlx::query!(
                    r#"
                    INSERT INTO gym_capacities (id, gym_id, user_id, capacity, granted_by)
                    VALUES ($1, $2, $3, $4, $5)
                    ON CONFLICT DO NOTHING
                    "#,
                    MembershipId::new().into_uuid(),
                    tenant.gym_id.into_uuid(),
                    target.into_uuid(),
                    capacity.as_str(),
                    tenant.actor_id.into_uuid(),
                )
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
            }
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "capacity.granted",
            "user",
            Some(target.into_uuid()),
            serde_json::json!({ "capacities": wanted, "reason": "standing_changed" }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn insert_staff(
        &self,
        tenant: &TenantContext,
        user: &User,
        password_hash: &str,
        capacities: &[Capacity],
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO users (id, email, display_name, password_hash)
            VALUES ($1, $2, $3, $4)
            "#,
            user.id.into_uuid(),
            user.email.as_str(),
            user.display_name,
            password_hash,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            // The unique index on the address. Surfaced as a conflict so the
            // caller can say "that person already has an account — have them
            // join, then promote from the roster" rather than a 500.
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                ApplicationError::Conflict("email".to_owned())
            }
            _ => db_err(e),
        })?;

        for capacity in capacities {
            sqlx::query!(
                r#"
                INSERT INTO gym_capacities (id, gym_id, user_id, capacity, granted_by)
                VALUES ($1, $2, $3, $4, $5)
                "#,
                MembershipId::new().into_uuid(),
                tenant.gym_id.into_uuid(),
                user.id.into_uuid(),
                capacity.as_str(),
                tenant.actor_id.into_uuid(),
            )
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }

        let granted: Vec<&str> = capacities.iter().map(|c| c.as_str()).collect();

        // Two rows, because two things happened and they answer different
        // questions later: "where did this account come from" and "why can
        // this person publish programmes".
        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "staff.created",
            "user",
            Some(user.id.into_uuid()),
            serde_json::json!({ "display_name": user.display_name, "capacities": granted }),
        )
        .await
        .map_err(db_err)?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "capacity.granted",
            "user",
            Some(user.id.into_uuid()),
            serde_json::json!({ "capacities": granted, "reason": "staff_created" }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn owners_other_than(
        &self,
        tenant: &TenantContext,
        excluding: UserId,
    ) -> ApplicationResult<usize> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT count(*) AS "count!"
            FROM gym_capacities
            WHERE gym_id = $1 AND capacity = 'owner'
              AND revoked_at IS NULL AND user_id <> $2
            "#,
            tenant.gym_id.into_uuid(),
            excluding.into_uuid(),
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(usize::try_from(row.count).unwrap_or(0))
    }
}

/// A capacity string the database accepted but we cannot parse means the CHECK
/// constraint and this enum have drifted — an internal fault, never a 403.
fn parse_capacities(raw: &[String]) -> ApplicationResult<Capabilities> {
    raw.iter()
        .map(|c| {
            Capacity::parse(c).ok_or_else(|| {
                ApplicationError::Internal(format!("unknown capacity in database: {c}").into())
            })
        })
        .collect::<ApplicationResult<Vec<_>>>()
        .map(Capabilities::new)
}

// --------------------------------------------------------------------- gyms

#[derive(Debug, Clone)]
pub struct PgGymRepository {
    pool: DbPool,
    /// ADR-0023: whether `gyms_singleton` (migration `20260823000019`) should
    /// actually reject a second gym. See `Config::single_gym_mode`'s doc for
    /// why this is a deployment choice, not a property of the pool.
    single_gym_mode: bool,
}

impl PgGymRepository {
    #[must_use]
    pub const fn new(pool: DbPool, single_gym_mode: bool) -> Self {
        Self {
            pool,
            single_gym_mode,
        }
    }
}

#[async_trait]
impl GymRepository for PgGymRepository {
    /// Gym + owner capacity in a single transaction. The account already exists.
    async fn create_with_owner(&self, gym: &Gym, owner: UserId) -> ApplicationResult<()> {
        let mut tx = begin_provisioning_tx(&self.pool, gym.id, owner)
            .await
            .map_err(db_err)?;

        // Tells `gyms_singleton` whether to actually enforce the cap — a plain
        // (non-macro) query, so it needs no `.sqlx` cache entry.
        sqlx::query("SELECT set_config('app.single_gym_mode', $1, true)")
            .bind(if self.single_gym_mode {
                "true"
            } else {
                "false"
            })
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;

        sqlx::query!(
            r#"INSERT INTO gyms (id, name, slug, is_personal) VALUES ($1, $2, $3, $4)"#,
            gym.id.into_uuid(),
            gym.name,
            gym.slug,
            gym.is_personal,
        )
        .execute(&mut *tx)
        .await
        .map_err(single_gym_violation_as_conflict)?;

        sqlx::query!(
            r#"
            INSERT INTO gym_capacities (id, gym_id, user_id, capacity, granted_by)
            VALUES ($1, $2, $3, 'owner', $3)
            "#,
            MembershipId::new().into_uuid(),
            gym.id.into_uuid(),
            owner.into_uuid(),
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        record_in_tx(
            &mut tx,
            gym.id,
            owner,
            "gym.created",
            "gym",
            Some(gym.id.into_uuid()),
            serde_json::json!({ "name": gym.name, "is_personal": gym.is_personal }),
        )
        .await
        .map_err(db_err)?;

        record_in_tx(
            &mut tx,
            gym.id,
            owner,
            "capacity.granted",
            "user",
            Some(owner.into_uuid()),
            serde_json::json!({ "capacities": ["owner"], "reason": "gym_created" }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn find(&self, id: GymId) -> ApplicationResult<Option<Gym>> {
        // gyms_read requires the gym to BE the active tenant, so scope to it.
        // Authorization to reach this call is the caller's responsibility.
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        sqlx::query("SELECT set_config('app.current_gym', $1, true)")
            .bind(id.into_uuid().to_string())
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;

        let row = sqlx::query!(
            r#"SELECT id, name, slug, is_personal, open_registration FROM gyms WHERE id = $1"#,
            id.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;

        Ok(row.map(|r| Gym {
            id: GymId::from(r.id),
            name: r.name,
            slug: r.slug,
            is_personal: r.is_personal,
            open_registration: r.open_registration,
        }))
    }

    async fn open_for_registration(&self) -> ApplicationResult<Vec<Gym>> {
        // Deliberately NOT `begin_tenant_tx`. An account with no membership has
        // no gym to scope by, which is exactly the state this answers for. The
        // `gyms_read` RLS policy would return nothing under an empty context,
        // so this uses a plain connection and relies on the WHERE clause — the
        // filter IS the access rule here: a gym is listed only because its
        // owner opted in to being findable.
        let rows = sqlx::query!(
            r#"
            SELECT id, name, slug, is_personal, open_registration
            FROM gyms
            WHERE open_registration
            ORDER BY name
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| Gym {
                id: GymId::from(r.id),
                name: r.name,
                slug: r.slug,
                is_personal: r.is_personal,
                open_registration: r.open_registration,
            })
            .collect())
    }

    async fn join_as_member(&self, gym: GymId, user: UserId) -> ApplicationResult<()> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;

        // Tenant context for the audit write. Set before the door check so the
        // whole thing is one transaction under one gym.
        sqlx::query("SELECT set_config('app.current_gym', $1, true)")
            .bind(gym.into_uuid().to_string())
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("SELECT set_config('app.current_user', $1, true)")
            .bind(user.into_uuid().to_string())
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;

        // FOR UPDATE, and re-read rather than trusting the caller: an owner
        // closing the door and a stranger walking through it is a real race,
        // and the door has to win it.
        let open = sqlx::query_scalar!(
            r#"SELECT open_registration FROM gyms WHERE id = $1 FOR UPDATE"#,
            gym.into_uuid()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        // Same answer for "no such gym" and "door closed": a closed gym must
        // not be discoverable by watching this endpoint's error change.
        if open != Some(true) {
            return Err(ApplicationError::NotFound { entity: "gym" });
        }

        // `member`, hard-coded. Not a parameter, not derived from the request:
        // the open door admits members and only members, so no input can turn
        // it into a way to grant yourself staff standing.
        let inserted = sqlx::query!(
            r#"
            INSERT INTO gym_capacities (id, gym_id, user_id, capacity, granted_by)
            VALUES ($1, $2, $3, 'member', $3)
            ON CONFLICT DO NOTHING
            "#,
            MembershipId::new().into_uuid(),
            gym.into_uuid(),
            user.into_uuid(),
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        if inserted.rows_affected() == 0 {
            return Err(ApplicationError::Conflict(
                "you are already a member of this gym".to_owned(),
            ));
        }

        record_in_tx(
            &mut tx,
            gym,
            user,
            "capacity.granted",
            "user",
            Some(user.into_uuid()),
            serde_json::json!({ "capacities": ["member"], "reason": "open_registration" }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn set_open_registration(
        &self,
        tenant: &TenantContext,
        open: bool,
    ) -> ApplicationResult<Gym> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            UPDATE gyms SET open_registration = $2
             WHERE id = $1
            RETURNING id, name, slug, is_personal, open_registration
            "#,
            tenant.gym_id.into_uuid(),
            open,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?
        .ok_or(ApplicationError::NotFound { entity: "gym" })?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            if open {
                "gym.registration_opened"
            } else {
                "gym.registration_closed"
            },
            "gym",
            Some(tenant.gym_id.into_uuid()),
            serde_json::json!({ "open_registration": open }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(Gym {
            id: GymId::from(row.id),
            name: row.name,
            slug: row.slug,
            is_personal: row.is_personal,
            open_registration: row.open_registration,
        })
    }
}

fn unique_violation_as(e: sqlx::Error, what: &str) -> ApplicationError {
    match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            ApplicationError::Conflict(what.to_owned())
        }
        _ => db_err(e),
    }
}

/// `gyms_singleton` (migration `20260823000019`) raises SQLSTATE P0001 — the
/// plpgsql default for a bare `RAISE EXCEPTION` — when a second gym is attempted.
/// This is the everyday outcome once a gym exists, not a rare race, so it gets a
/// real 409 rather than falling through to `db_err`'s generic 500.
fn single_gym_violation_as_conflict(e: sqlx::Error) -> ApplicationError {
    match &e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some("P0001") => {
            ApplicationError::Conflict("gym".to_owned())
        }
        _ => unique_violation_as(e, "gym"),
    }
}

// ----------------------------------------------------------------- sessions

#[derive(Debug, Clone)]
pub struct PgSessionRepository {
    pool: DbPool,
}

impl PgSessionRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionRepository for PgSessionRepository {
    async fn create(&self, session: &NewSession) -> ApplicationResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO sessions
                (id, user_id, token_hash, expires_at, rotated_from, device_label)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            session.id.into_uuid(),
            session.user_id.into_uuid(),
            session.token_hash,
            session.expires_at,
            session.rotated_from.map(SessionId::into_uuid),
            session.device_label.as_deref(),
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(())
    }

    async fn find_by_token_hash(&self, hash: &[u8]) -> ApplicationResult<Option<SessionRecord>> {
        let row = sqlx::query!(
            r#"
            SELECT id, user_id, expires_at, revoked_at
            FROM sessions
            WHERE token_hash = $1
            "#,
            hash
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(row.map(|r| SessionRecord {
            id: SessionId::from(r.id),
            user_id: UserId::from(r.user_id),
            expires_at: r.expires_at,
            revoked_at: r.revoked_at,
        }))
    }

    async fn revoke(&self, id: SessionId) -> ApplicationResult<bool> {
        // Single conditional UPDATE: Postgres serialises concurrent writers on the
        // row, so exactly one caller sees rows_affected == 1. This is the
        // compare-and-swap that prevents two concurrent refreshes both winning.
        let result = sqlx::query!(
            r#"UPDATE sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL"#,
            id.into_uuid()
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(result.rows_affected() == 1)
    }

    async fn revoke_all_for_user(&self, user: UserId) -> ApplicationResult<u64> {
        let result = sqlx::query!(
            r#"UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL"#,
            user.into_uuid()
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(result.rows_affected())
    }
}
