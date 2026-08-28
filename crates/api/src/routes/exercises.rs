//! Exercise catalogue endpoints — the first tenant-scoped resource.
//!
//! Routes are nested under `/gyms/{gym_id}/` so the tenant is explicit in the URL
//! and `TenantScope` can resolve the caller's role before the handler runs.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use gym_application::exercises::{CreateExerciseCommand, CurationDecision};
use gym_domain::{
    ExerciseId,
    exercise::{CatalogueStatus, Exercise, Modality},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateExerciseRequest {
    #[schema(example = "Back Squat")]
    pub name: String,
    pub modality: Modality,
    #[schema(example = "Keep the bar over mid-foot.")]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ExerciseResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub modality: Modality,
    pub notes: Option<String>,
    /// `proposed` / `approved` / `retired` (ADR-0024). A client shows a
    /// proposal with a badge rather than hiding it — the coach who raised it
    /// is using it right now.
    pub status: CatalogueStatus,
    /// Who raised it. Null for entries predating catalogue curation.
    #[schema(nullable)]
    pub proposed_by: Option<uuid::Uuid>,
}

impl From<Exercise> for ExerciseResponse {
    fn from(e: Exercise) -> Self {
        // Note: `gym_id` is deliberately not exposed — it is implied by the URL.
        let proposed_by = e.proposed_by.into_uuid();
        Self {
            id: e.id.into_uuid(),
            name: e.name,
            modality: e.modality,
            notes: e.notes,
            status: e.status,
            // The nil UUID is how a pre-curation row's missing author survives
            // the domain's non-optional UserId; it becomes null again here
            // rather than leaking all-zeroes to a client.
            proposed_by: (!proposed_by.is_nil()).then_some(proposed_by),
        }
    }
}

/// List the gym's exercise catalogue. Any member may read it.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/exercises",
    tag = "exercises",
    security(("bearer" = [])),
    params(("gym_id" = uuid::Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Catalogue", body = Vec<ExerciseResponse>),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<ExerciseResponse>>, ApiError> {
    let exercises = state.exercises.list(&tenant).await?;
    Ok(Json(exercises.into_iter().map(Into::into).collect()))
}

/// Add an exercise. Anyone who coaches may (ADR-0024).
///
/// What the caller's standing decides is the *state* it lands in, not whether
/// it is created: a catalogue manager's entry is approved outright, a
/// trainer's is a proposal — usable by them immediately, and queued for
/// curation so duplicates are caught before they split anyone's history.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/exercises",
    tag = "exercises",
    security(("bearer" = [])),
    params(("gym_id" = uuid::Uuid, Path, description = "Gym id")),
    request_body = CreateExerciseRequest,
    responses(
        (status = 201, description = "Created", body = ExerciseResponse),
        (status = 400, description = "Invalid input"),
        (status = 403, description = "Caller does not coach here"),
        (status = 404, description = "Gym not found or caller is not a member"),
        (status = 409, description = "An exercise with that name already exists"),
    )
)]
pub async fn create(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<CreateExerciseRequest>,
) -> Result<(StatusCode, Json<ExerciseResponse>), ApiError> {
    let exercise = state
        .exercises
        .create(
            &tenant,
            CreateExerciseCommand {
                name: body.name,
                modality: body.modality,
                notes: body.notes,
            },
        )
        .await?;

    Ok((StatusCode::CREATED, Json(exercise.into())))
}

/// Wire twin of `CurationDecision`, mirroring how `TransitionRequest` shadows
/// `Transition`: the application layer stays free of HTTP concerns.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CurateDecisionRequest {
    Approve,
    Retire,
    Reinstate,
}

impl From<CurateDecisionRequest> for CurationDecision {
    fn from(d: CurateDecisionRequest) -> Self {
        match d {
            CurateDecisionRequest::Approve => Self::Approve,
            CurateDecisionRequest::Retire => Self::Retire,
            CurateDecisionRequest::Reinstate => Self::Reinstate,
        }
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CurateRequest {
    pub decision: CurateDecisionRequest,
}

/// The proposals waiting on a curator.
///
/// A separate route rather than `?status=proposed` on the list: this is a
/// manager's work queue, and someone without standing should be refused, not
/// handed an empty array that reads as "nothing to do".
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/exercises/pending",
    tag = "exercises",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Awaiting curation", body = Vec<ExerciseResponse>),
        (status = 403, description = "Caller may not curate the catalogue"),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn pending(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<ExerciseResponse>>, ApiError> {
    let exercises = state.exercises.pending_curation(&tenant).await?;
    Ok(Json(exercises.into_iter().map(Into::into).collect()))
}

/// Approve, retire or reinstate a movement.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/exercises/{exercise_id}/curate",
    tag = "exercises",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("exercise_id" = Uuid, Path, description = "Exercise id"),
    ),
    request_body = CurateRequest,
    responses(
        (status = 200, description = "Decided", body = ExerciseResponse),
        (status = 400, description = "Not a legal move from the current status"),
        (status = 403, description = "Caller may not curate the catalogue"),
        (status = 404, description = "Exercise not found"),
    )
)]
pub async fn curate(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, exercise_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<CurateRequest>,
) -> Result<Json<ExerciseResponse>, ApiError> {
    let exercise = state
        .exercises
        .curate(&tenant, ExerciseId::from(exercise_id), body.decision.into())
        .await?;

    Ok(Json(exercise.into()))
}
