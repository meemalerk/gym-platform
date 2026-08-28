//! Exercise catalogue use-cases.
//!
//! Every handler follows the same order (docs/authorization-model.md):
//!   1. authorization — "may this actor attempt this?"
//!   2. domain policy — "is this action valid?"
//!   3. persist
//!
//! Keeping these separate is deliberate: role checks never live in the domain
//! constructor, and validation never lives in the authorization layer.

use std::sync::Arc;

use gym_domain::{
    ExerciseId, TenantContext,
    exercise::{CatalogueStatus, Exercise, Modality},
};

use crate::{ApplicationError, ApplicationResult, ports::ExerciseRepository};

#[derive(Debug, Clone)]
pub struct CreateExerciseCommand {
    pub name: String,
    pub modality: Modality,
    pub notes: Option<String>,
}

#[derive(Clone)]
pub struct ExerciseService {
    pub exercises: Arc<dyn ExerciseRepository>,
}

impl ExerciseService {
    /// Any member of the gym may read the catalogue.
    pub async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<Exercise>> {
        self.exercises.list(tenant).await
    }

    pub async fn get(&self, tenant: &TenantContext, id: ExerciseId) -> ApplicationResult<Exercise> {
        self.exercises
            .find(tenant, id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "exercise" })
    }

    /// The proposals waiting on a curator.
    ///
    /// Not a filter the client applies to `list`: the queue is a manager's
    /// work list, and a member asking for it should be refused rather than
    /// handed an empty array that implies they simply have nothing to do.
    pub async fn pending_curation(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<Exercise>> {
        if !tenant.capabilities.can_curate_catalogue() {
            return Err(ApplicationError::Forbidden);
        }

        Ok(self
            .exercises
            .list(tenant)
            .await?
            .into_iter()
            .filter(|e| e.status == CatalogueStatus::Proposed)
            .collect())
    }

    /// Add a movement to the catalogue.
    ///
    /// Anyone who coaches may do this (ADR-0024) — a trainer who cannot name
    /// the movement they are prescribing cannot write a programme at all. What
    /// their standing decides is not *whether* the entry is created but what
    /// state it lands in: a curator's entry is approved, a coach's is a
    /// proposal, and `Exercise::proposed_by` makes that the constructor's
    /// decision rather than something a route could get wrong.
    pub async fn create(
        &self,
        tenant: &TenantContext,
        cmd: CreateExerciseCommand,
    ) -> ApplicationResult<Exercise> {
        // 1. Authorization.
        if !tenant.capabilities.can_propose_exercises() {
            return Err(ApplicationError::Forbidden);
        }

        // 2. Domain validation — the constructor enforces the invariants.
        let exercise = Exercise::proposed_by(
            tenant.gym_id,
            &cmd.name,
            cmd.modality,
            cmd.notes.as_deref(),
            tenant.actor_id,
            tenant.capabilities.can_curate_catalogue(),
        )?;

        // 3. Persist.
        self.exercises.insert(tenant, &exercise).await?;
        Ok(exercise)
    }

    /// Promote a proposal, retire a movement, or bring a retired one back.
    ///
    /// One method rather than three routes because they are one decision —
    /// "what standing should this movement have?" — and splitting them would
    /// scatter the authorization check the way ADR-0013 warns against.
    pub async fn curate(
        &self,
        tenant: &TenantContext,
        id: ExerciseId,
        decision: CurationDecision,
    ) -> ApplicationResult<Exercise> {
        if !tenant.capabilities.can_curate_catalogue() {
            return Err(ApplicationError::Forbidden);
        }

        let mut exercise = self.get(tenant, id).await?;

        let action = match decision {
            CurationDecision::Approve => {
                exercise.approve()?;
                "exercise.approved"
            }
            CurationDecision::Retire => {
                exercise.retire()?;
                "exercise.retired"
            }
            CurationDecision::Reinstate => {
                exercise.reinstate()?;
                "exercise.reinstated"
            }
        };

        self.exercises
            .save_status(tenant, &exercise, action)
            .await?;
        Ok(exercise)
    }
}

/// What a curator decided about a movement.
///
/// No `utoipa::ToSchema` here on purpose: this crate knows nothing about HTTP,
/// and the API layer keeps its own wire twin, exactly as it does for
/// `Transition`. The mapping is one `From` impl and it keeps the schema
/// generator out of the use-case layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurationDecision {
    Approve,
    Retire,
    Reinstate,
}
