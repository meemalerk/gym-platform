//! Postgres adapter for coaching requests (ADR-0025).
//!
//! The interesting method is `save_decision`: accepting a request and creating
//! the coaching relationship happen in ONE transaction. A two-call version
//! would leave a window where the member has been told yes and their coach
//! still cannot see them — and the failure would be silent, because both calls
//! individually succeeded. Same reasoning as the audit log being written in the
//! transaction it describes.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{
        CoachingRequestRepository, CoachingRequestView, TrainerDirectoryEntry,
        TrainerDirectoryRepository,
    },
};
use gym_domain::{
    CoachingRequestId, GymId, TenantContext, UserId,
    coaching::CoachRelationship,
    coaching_request::{CoachingRequest, RequestStatus},
};
use uuid::Uuid;

use crate::{audit::record_in_tx, db::begin_tenant_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

#[derive(Debug, Clone)]
pub struct PgCoachingRequestRepository {
    pool: crate::DbPool,
}

impl PgCoachingRequestRepository {
    #[must_use]
    pub const fn new(pool: crate::DbPool) -> Self {
        Self { pool }
    }
}

/// Rebuild the status enum from its columns.
///
/// The CHECK constraint guarantees the evidence is present, so a gap here means
/// the constraint was bypassed. That is corruption, and it is reported rather
/// than quietly downgraded to `Pending` — which would resurrect a request the
/// coach had already declined.
fn status_from_row(
    status: &str,
    decided_at: Option<DateTime<Utc>>,
    decided_by: Option<Uuid>,
) -> ApplicationResult<RequestStatus> {
    let corrupt = |what: &str| {
        ApplicationError::Internal(
            format!("coaching request is '{what}' but has no decision evidence").into(),
        )
    };

    match status {
        "pending" => Ok(RequestStatus::Pending),
        "accepted" => match (decided_at, decided_by) {
            (Some(decided_at), Some(decided_by)) => Ok(RequestStatus::Accepted {
                decided_at,
                decided_by: UserId::from(decided_by),
            }),
            _ => Err(corrupt("accepted")),
        },
        "declined" => match (decided_at, decided_by) {
            (Some(decided_at), Some(decided_by)) => Ok(RequestStatus::Declined {
                decided_at,
                decided_by: UserId::from(decided_by),
            }),
            _ => Err(corrupt("declined")),
        },
        // No decided_by: the athlete is already named by athlete_id.
        "withdrawn" => decided_at
            .map(|decided_at| RequestStatus::Withdrawn { decided_at })
            .ok_or_else(|| corrupt("withdrawn")),
        other => Err(ApplicationError::Internal(
            format!("unknown coaching request status '{other}'").into(),
        )),
    }
}

/// The decision columns, in the shape the UPDATE wants them.
fn decision_columns(status: &RequestStatus) -> (Option<DateTime<Utc>>, Option<Uuid>) {
    match status {
        RequestStatus::Pending => (None, None),
        RequestStatus::Accepted {
            decided_at,
            decided_by,
        }
        | RequestStatus::Declined {
            decided_at,
            decided_by,
        } => (Some(*decided_at), Some(decided_by.into_uuid())),
        RequestStatus::Withdrawn { decided_at } => (Some(*decided_at), None),
    }
}

#[async_trait]
impl CoachingRequestRepository for PgCoachingRequestRepository {
    async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<CoachingRequestView>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT r.id, r.gym_id, r.athlete_id, r.coach_id, r.raised_by, r.status, r.message,
                   r.requested_at, r.decided_at, r.decided_by,
                   a.display_name AS athlete_name,
                   c.display_name AS coach_name
            FROM coaching_requests r
            JOIN users a ON a.id = r.athlete_id
            JOIN users c ON c.id = r.coach_id
            WHERE r.gym_id = $1
            -- Pending first: this list is a work queue before it is a history.
            ORDER BY (r.status = 'pending') DESC, r.requested_at DESC
            "#,
            tenant.gym_id.into_uuid(),
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        rows.into_iter()
            .map(|r| {
                Ok(CoachingRequestView {
                    request: CoachingRequest {
                        id: CoachingRequestId::from(r.id),
                        gym_id: GymId::from(r.gym_id),
                        athlete_id: UserId::from(r.athlete_id),
                        raised_by: UserId::from(r.raised_by),
                        coach_id: UserId::from(r.coach_id),
                        status: status_from_row(&r.status, r.decided_at, r.decided_by)?,
                        message: r.message,
                        requested_at: r.requested_at,
                    },
                    athlete_name: r.athlete_name,
                    coach_name: r.coach_name,
                })
            })
            .collect()
    }

    async fn find(
        &self,
        tenant: &TenantContext,
        id: CoachingRequestId,
    ) -> ApplicationResult<Option<CoachingRequest>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT id, gym_id, athlete_id, coach_id, raised_by, status, message,
                   requested_at, decided_at, decided_by
            FROM coaching_requests
            WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid(),
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        row.map(|r| {
            Ok(CoachingRequest {
                id: CoachingRequestId::from(r.id),
                gym_id: GymId::from(r.gym_id),
                athlete_id: UserId::from(r.athlete_id),
                coach_id: UserId::from(r.coach_id),
                raised_by: UserId::from(r.raised_by),
                status: status_from_row(&r.status, r.decided_at, r.decided_by)?,
                message: r.message,
                requested_at: r.requested_at,
            })
        })
        .transpose()
    }

    async fn insert(
        &self,
        tenant: &TenantContext,
        request: &CoachingRequest,
    ) -> ApplicationResult<()> {
        debug_assert_eq!(request.gym_id, tenant.gym_id);

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO coaching_requests
                (id, gym_id, athlete_id, coach_id, raised_by, status, message, requested_at)
            VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)
            "#,
            request.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            request.athlete_id.into_uuid(),
            request.coach_id.into_uuid(),
            request.raised_by.into_uuid(),
            request.message.as_deref(),
            request.requested_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            // The partial unique index on pending rows. Surfaced as a conflict
            // the caller can act on, not a 500.
            sqlx::Error::Database(db) if db.is_unique_violation() => ApplicationError::Conflict(
                "you already have a request outstanding with this coach".to_owned(),
            ),
            _ => db_err(e),
        })?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "coaching_request.raised",
            "coaching_request",
            Some(request.id.into_uuid()),
            serde_json::json!({ "coach_id": request.coach_id.into_uuid() }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn insert_chosen(
        &self,
        tenant: &TenantContext,
        request: &CoachingRequest,
        pairing: &CoachRelationship,
    ) -> ApplicationResult<()> {
        debug_assert_eq!(request.gym_id, tenant.gym_id);
        debug_assert_eq!(pairing.gym_id, tenant.gym_id);

        let (decided_at, decided_by) = decision_columns(&request.status);

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // The request lands already accepted. It is written at all so the
        // member's note survives beside the pairing it explains.
        sqlx::query!(
            r#"
            INSERT INTO coaching_requests
                (id, gym_id, athlete_id, coach_id, raised_by, status, message,
                 requested_at, decided_at, decided_by)
            VALUES ($1, $2, $3, $4, $5, 'accepted', $6, $7, $8, $9)
            "#,
            request.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            request.athlete_id.into_uuid(),
            request.coach_id.into_uuid(),
            request.raised_by.into_uuid(),
            request.message.as_deref(),
            request.requested_at,
            decided_at,
            decided_by,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        // Same transaction as the request, for the same reason acceptance
        // always was: a record saying "accepted" beside a coach who cannot see
        // their client is the one state this must never be in.
        sqlx::query!(
            r#"
            INSERT INTO coach_relationships
                (id, gym_id, coach_id, athlete_id, status, started_at, created_by)
            VALUES ($1, $2, $3, $4, 'active', $5, $6)
            "#,
            pairing.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            pairing.coach_id.into_uuid(),
            pairing.athlete_id.into_uuid(),
            pairing.started_at,
            pairing.created_by.into_uuid(),
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            // A manager paired them a moment ago. Nothing is wrong with the
            // world; the thing the caller wanted is already true.
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                ApplicationError::Conflict("they already coach you".to_owned())
            }
            _ => db_err(e),
        })?;

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            "coach_relationship.created",
            "coach_relationship",
            Some(pairing.id.into_uuid()),
            serde_json::json!({
                "coach_id": pairing.coach_id.into_uuid(),
                "athlete_id": pairing.athlete_id.into_uuid(),
                "reason": "chosen_by_athlete",
            }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }

    async fn save_decision(
        &self,
        tenant: &TenantContext,
        request: &CoachingRequest,
        pairing: Option<&CoachRelationship>,
        action: &'static str,
    ) -> ApplicationResult<()> {
        debug_assert_eq!(request.gym_id, tenant.gym_id);

        let (decided_at, decided_by) = decision_columns(&request.status);

        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Guarded on `status = 'pending'` rather than trusting the read that
        // preceded it: two coaches (or a coach and a manager) can answer the
        // same request at the same moment, and only one of them may win.
        let updated = sqlx::query!(
            r#"
            UPDATE coaching_requests
               SET status = $3, decided_at = $4, decided_by = $5
             WHERE gym_id = $1 AND id = $2 AND status = 'pending'
            "#,
            tenant.gym_id.into_uuid(),
            request.id.into_uuid(),
            request.status.as_str(),
            decided_at,
            decided_by,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        if updated.rows_affected() == 0 {
            return Err(ApplicationError::Conflict(
                "this request has already been answered".to_owned(),
            ));
        }

        // The whole point of doing this here: acceptance and the access grant
        // it creates land together or not at all.
        if let Some(pairing) = pairing {
            sqlx::query!(
                r#"
                INSERT INTO coach_relationships
                    (id, gym_id, coach_id, athlete_id, status, started_at, created_by)
                VALUES ($1, $2, $3, $4, 'active', $5, $6)
                "#,
                pairing.id.into_uuid(),
                tenant.gym_id.into_uuid(),
                pairing.coach_id.into_uuid(),
                pairing.athlete_id.into_uuid(),
                pairing.started_at,
                pairing.created_by.into_uuid(),
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| match &e {
                // They were paired by a manager between the ask and the answer.
                // The request is still answered; the pairing simply already
                // exists, so this is a conflict rather than a failure.
                sqlx::Error::Database(db) if db.is_unique_violation() => {
                    ApplicationError::Conflict("they are already your client".to_owned())
                }
                _ => db_err(e),
            })?;

            record_in_tx(
                &mut tx,
                tenant.gym_id,
                tenant.actor_id,
                "coach_relationship.created",
                "coach_relationship",
                Some(pairing.id.into_uuid()),
                serde_json::json!({
                    "coach_id": pairing.coach_id.into_uuid(),
                    "athlete_id": pairing.athlete_id.into_uuid(),
                    "via": "coaching_request",
                }),
            )
            .await
            .map_err(db_err)?;
        }

        record_in_tx(
            &mut tx,
            tenant.gym_id,
            tenant.actor_id,
            action,
            "coaching_request",
            Some(request.id.into_uuid()),
            serde_json::json!({
                "athlete_id": request.athlete_id.into_uuid(),
                "coach_id": request.coach_id.into_uuid(),
            }),
        )
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(())
    }
}

// ------------------------------------------------------------------ directory

#[derive(Debug, Clone)]
pub struct PgTrainerDirectoryRepository {
    pool: crate::DbPool,
}

impl PgTrainerDirectoryRepository {
    #[must_use]
    pub const fn new(pool: crate::DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TrainerDirectoryRepository for PgTrainerDirectoryRepository {
    async fn trainers(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<TrainerDirectoryEntry>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Everyone in this gym who holds a coaching capacity, with the profile
        // they published and how many people they currently coach.
        //
        // `trainer_profiles` is person-owned and has no gym_id (ADR-0014), so
        // the LEFT JOIN is unscoped by design — a coach's biography follows the
        // person. What is gym-scoped is who appears at all, which comes from
        // gym_capacities, and the client count, which is filtered to this gym.
        //
        // Emails are not selected. This is a professional directory, not the
        // roster, and the distinction is the reason it can be open to members
        // at all (ADR-0025).
        let rows = sqlx::query!(
            r#"
            SELECT u.id,
                   u.display_name,
                   p.headline,
                   p.bio,
                   p.specialties AS "specialties?",
                   p.certifications AS "certifications?",
                   (
                     SELECT count(*)
                     FROM coach_relationships cr
                     WHERE cr.gym_id = $1
                       AND cr.coach_id = u.id
                       AND cr.status = 'active'
                   ) AS "active_clients!"
            FROM users u
            JOIN gym_capacities gc ON gc.user_id = u.id AND gc.gym_id = $1
            LEFT JOIN trainer_profiles p ON p.user_id = u.id
            -- `trainer` alone since ADR-0036. The list used to read
            -- ('trainer', 'head_coach') and the line it drew was COACHING rungs
            -- rather than every rung that can coach: owner and admin were
            -- already excluded, because a directory a member picks a coach from
            -- should not be padded with whoever runs the place. Removing
            -- head_coach leaves that intent expressed by one value.
            --
            -- An owner who genuinely coaches holds `trainer` as well — the
            -- capacity SET is what makes that expressible (ADR-0014) — and
            -- appears here on that basis rather than by being senior.
            WHERE gc.capacity = 'trainer'
              -- Revoked standing is not standing. Missing before, and it meant
              -- an ex-trainer stayed browsable in the directory for ever.
              AND gc.revoked_at IS NULL
            GROUP BY u.id, u.display_name, p.headline, p.bio, p.specialties, p.certifications
            ORDER BY u.display_name
            "#,
            tenant.gym_id.into_uuid(),
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| TrainerDirectoryEntry {
                user_id: UserId::from(r.id),
                display_name: r.display_name,
                headline: r.headline,
                bio: r.bio,
                specialties: r.specialties.unwrap_or_default(),
                certifications: r.certifications.unwrap_or_default(),
                active_clients: r.active_clients,
            })
            .collect())
    }
}
