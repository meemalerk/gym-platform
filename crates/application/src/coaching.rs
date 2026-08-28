//! Coach–athlete relationship use-cases.
//!
//! The visibility rules live in `gym_domain::coaching`, not here. This layer
//! loads what those rules need and persists the outcome — which keeps
//! "who may see whom" testable without a database, and stops the same rule being
//! written twice in slightly different ways.

use std::sync::Arc;

use gym_domain::{
    CoachRelationshipId, TenantContext, UserId,
    coaching::{CoachRelationship, CoachingError, may_view_athlete},
};

use crate::{
    ApplicationError, ApplicationResult,
    ports::{Clock, CoachRelationshipView, CoachRepository, UserRepository},
};

#[derive(Clone)]
pub struct CoachingService {
    pub relationships: Arc<dyn CoachRepository>,
    pub users: Arc<dyn UserRepository>,
    pub clock: Arc<dyn Clock>,
}

impl CoachingService {
    /// The relationships the caller is allowed to see.
    ///
    /// A manager or head coach sees the whole roster; anyone else sees only the
    /// relationships they are personally part of. Filtered here rather than in
    /// SQL so the rule stays in one place — see the note on `CoachRepository::list`.
    pub async fn list(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<CoachRelationshipView>> {
        let all = self.relationships.list(tenant).await?;

        if tenant.capabilities.can_manage_catalogue() {
            return Ok(all);
        }

        Ok(all
            .into_iter()
            .filter(|v| {
                v.relationship.coach_id == tenant.actor_id
                    || v.relationship.athlete_id == tenant.actor_id
            })
            .collect())
    }

    // `assign` — a manager creating the pairing outright — is GONE (ADR-0034).
    //
    // A coaching relationship grants a trainer access to that member's whole
    // training history, and the trainer was never asked. That is the one-sided
    // consent this codebase already refused for a trainer pairing themselves;
    // it was simply never applied to the manager doing it for them.
    //
    // The decision is unchanged — which trainer works with which member is gym
    // management — but it now lands as a pending proposal the trainer answers:
    // `CoachingRequestService::propose`, then `answer`, which creates the
    // relationship in the same transaction as the acceptance.

    /// End a relationship. The row survives; only its status changes.
    pub async fn end(
        &self,
        tenant: &TenantContext,
        id: CoachRelationshipId,
    ) -> ApplicationResult<CoachRelationship> {
        if !CoachRelationship::may_assign(&tenant.capabilities) {
            return Err(ApplicationError::Forbidden);
        }

        let mut relationship =
            self.relationships
                .find(tenant, id)
                .await?
                .ok_or(ApplicationError::NotFound {
                    entity: "coach relationship",
                })?;

        relationship.end(tenant.actor_id, self.clock.now())?;
        self.relationships.save_ended(tenant, &relationship).await?;

        Ok(relationship)
    }

    /// May the caller read this athlete's personal data?
    ///
    /// The gate every per-athlete endpoint should call before returning anything
    /// — programmes, measurements, goals, guidance.
    pub async fn may_view(
        &self,
        tenant: &TenantContext,
        athlete: UserId,
    ) -> ApplicationResult<bool> {
        // Cheap answers first: your own data, or you manage the gym. Neither
        // needs a query, and together they cover most requests.
        if tenant.actor_id == athlete || tenant.capabilities.can_manage_catalogue() {
            return Ok(true);
        }

        let relationships = self
            .relationships
            .active_for_user(tenant, tenant.actor_id)
            .await?;

        Ok(may_view_athlete(
            tenant.actor_id,
            &tenant.capabilities,
            athlete,
            &relationships,
        ))
    }
}

impl From<CoachingError> for ApplicationError {
    /// Coaching errors are the caller asking for something invalid, not a
    /// failure — 4xx, with a message safe to show.
    fn from(err: CoachingError) -> Self {
        match err {
            CoachingError::NotPermitted => Self::Forbidden,

            // 409, not 400: the request was well-formed and would have been
            // valid a moment earlier. The distinction matters to a client
            // deciding whether to fix the request or simply refresh — and it
            // matches what the repository's compare-and-swap returns when two
            // requests race, so the same situation gets the same status
            // whichever path detects it.
            CoachingError::AlreadyEnded => Self::Conflict("this coaching relationship".to_owned()),

            other => Self::Domain(gym_domain::DomainError::Invalid(other.to_string())),
        }
    }
}
