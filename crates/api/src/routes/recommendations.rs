//! Recommendation endpoint — deterministic suggestions with reasons attached.

use axum::{Json, extract::State};
use gym_domain::ProgramFocus;
use serde::Serialize;
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProgramSuggestionResponse {
    pub program_id: Uuid,
    pub name: String,
    pub focus: ProgramFocus,
    pub version_id: Uuid,
    /// Why this is here, in words: "Matches your goal to lift 100 kg".
    pub because: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TrainerSuggestionResponse {
    pub user_id: Uuid,
    pub display_name: String,
    pub headline: Option<String>,
    pub specialties: Vec<String>,
    pub because: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RecommendationsResponse {
    pub programs: Vec<ProgramSuggestionResponse>,
    pub trainers: Vec<TrainerSuggestionResponse>,
}

/// Suggestions for the caller, derived from their active goals by a rule small
/// enough to print: goal → focus → published programmes with that focus you are
/// not on, and coaches whose stated specialties speak to it. Every suggestion
/// says why. Empty when you have no active goals.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/recommendations",
    tag = "recommendations",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Suggestions", body = RecommendationsResponse),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn for_me(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<RecommendationsResponse>, ApiError> {
    let recs = state.recommendations.for_me(&tenant).await?;

    Ok(Json(RecommendationsResponse {
        programs: recs
            .programs
            .into_iter()
            .map(|p| ProgramSuggestionResponse {
                program_id: p.program_id.into_uuid(),
                name: p.name,
                focus: p.focus,
                version_id: p.version_id.into_uuid(),
                because: p.because,
            })
            .collect(),
        trainers: recs
            .trainers
            .into_iter()
            .map(|t| TrainerSuggestionResponse {
                user_id: t.user_id.into_uuid(),
                display_name: t.display_name,
                headline: t.headline,
                specialties: t.specialties,
                because: t.because,
            })
            .collect(),
    }))
}
