//! Coaching requests and the trainer directory (ADR-0025).
//!
//! Note which of these is open and which is not. The directory is readable by
//! anyone in the gym — it carries only what each coach published about
//! themselves, so it is not the roster and must not be gated like one, or the
//! feature it exists for is impossible. Requests are scoped to the people
//! party to them.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use gym_application::coaching_requests::{
    ProposeCoachCommand, RaiseRequestCommand, RequestDecision,
};
use gym_domain::{CoachingRequestId, coaching_request::RequestStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TrainerDirectoryResponse {
    pub user_id: Uuid,
    pub display_name: String,
    #[schema(nullable)]
    pub headline: Option<String>,
    #[schema(nullable)]
    pub bio: Option<String>,
    pub specialties: Vec<String>,
    pub certifications: Vec<String>,
    /// How many people they coach here today. Shown so a member can tell a
    /// coach with capacity from one with twenty clients.
    pub active_clients: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CoachingRequestResponse {
    pub id: Uuid,
    pub athlete_id: Uuid,
    pub athlete_name: String,
    pub coach_id: Uuid,
    pub coach_name: String,
    pub status: RequestStatus,
    /// Convenience for a list that only ever splits pending from resolved.
    pub is_pending: bool,
    /// True when the GYM proposed this coach, false when the member asked for
    /// them. Decides which inbox it belongs in and who may answer.
    pub is_proposal: bool,
    #[schema(nullable)]
    pub message: Option<String>,
    pub requested_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RaiseRequestBody {
    pub coach_id: Uuid,
    /// Why you are asking, in your own words. Optional.
    #[schema(example = "Getting back to squatting after an ankle injury.")]
    pub message: Option<String>,
}

/// Wire twin of `RequestDecision` — the application layer stays free of HTTP,
/// exactly as it does for `Transition` and `CurationDecision`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnswerBody {
    Accept,
    Decline,
}

impl From<AnswerBody> for RequestDecision {
    fn from(a: AnswerBody) -> Self {
        match a {
            AnswerBody::Accept => Self::Accept,
            AnswerBody::Decline => Self::Decline,
        }
    }
}

/// The gym's coaches, with the profiles they published.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/trainers",
    tag = "coaching",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Coaches at this gym", body = Vec<TrainerDirectoryResponse>),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn directory(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<TrainerDirectoryResponse>>, ApiError> {
    let trainers = state.coaching_requests.directory(&tenant).await?;

    Ok(Json(
        trainers
            .into_iter()
            .map(|t| TrainerDirectoryResponse {
                user_id: t.user_id.into_uuid(),
                display_name: t.display_name,
                headline: t.headline,
                bio: t.bio,
                specialties: t.specialties,
                certifications: t.certifications,
                active_clients: t.active_clients,
            })
            .collect(),
    ))
}

/// Requests you are party to — or all of them, if you manage the gym.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/coaching-requests",
    tag = "coaching",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Requests, pending first", body = Vec<CoachingRequestResponse>),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<CoachingRequestResponse>>, ApiError> {
    let views = state.coaching_requests.list(&tenant).await?;

    Ok(Json(
        views
            .into_iter()
            .map(|v| CoachingRequestResponse {
                id: v.request.id.into_uuid(),
                athlete_id: v.request.athlete_id.into_uuid(),
                athlete_name: v.athlete_name,
                coach_id: v.request.coach_id.into_uuid(),
                coach_name: v.coach_name,
                is_pending: v.request.status.is_pending(),
                is_proposal: v.request.is_proposal(),
                status: v.request.status,
                message: v.request.message,
                requested_at: v.request.requested_at,
            })
            .collect(),
    ))
}

/// Choose a coach. They coach you from that moment (ADR-0031).
///
/// The route keeps its old path and name because it is still "the member acts
/// on a coach"; what changed is that nobody has to answer. The response comes
/// back already `accepted`, and the coaching relationship exists.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/coaching-requests",
    tag = "coaching",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = RaiseRequestBody,
    responses(
        (status = 201, description = "They coach you now", body = CoachingRequestResponse),
        (status = 400, description = "That person does not coach here, or you chose yourself"),
        (status = 404, description = "Coach not found in this gym"),
        (status = 409, description = "Already working together"),
    )
)]
pub async fn raise(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<RaiseRequestBody>,
) -> Result<(StatusCode, Json<CoachingRequestResponse>), ApiError> {
    let request = state
        .coaching_requests
        .choose(
            &tenant,
            RaiseRequestCommand {
                coach_id: gym_domain::UserId::from(body.coach_id),
                message: body.message,
            },
        )
        .await?;

    // Names are not to hand here — this is the row we just wrote, and the
    // caller already knows who they are and who they asked. The list read
    // carries them. Honest nulls beat a second query nobody needs.
    Ok((
        StatusCode::CREATED,
        Json(CoachingRequestResponse {
            id: request.id.into_uuid(),
            athlete_id: request.athlete_id.into_uuid(),
            athlete_name: String::new(),
            coach_id: request.coach_id.into_uuid(),
            coach_name: String::new(),
            is_pending: request.status.is_pending(),
            is_proposal: request.is_proposal(),
            status: request.status,
            message: request.message,
            requested_at: request.requested_at,
        }),
    ))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ProposeCoachBody {
    pub athlete_id: Uuid,
    pub coach_id: Uuid,
    /// A line for the trainer — "Malaak trains Tuesdays and Thursdays".
    #[schema(nullable)]
    pub message: Option<String>,
}

/// Propose a coach for a member. Head coaches and above (ADR-0034).
///
/// This is what replaced pairing them outright. It lands **pending**: the
/// trainer accepts, and accepting is what creates the relationship. The
/// proposer cannot answer their own proposal, so consent is real rather than
/// two taps by the same person.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/coaching-requests/propose",
    tag = "coaching",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = ProposeCoachBody,
    responses(
        (status = 201, description = "Proposed; waiting on the trainer", body = CoachingRequestResponse),
        (status = 400, description = "Proposing yourself, or the same person twice"),
        (status = 403, description = "Only head coaches and above may propose"),
        (status = 404, description = "Coach or member not found in this gym"),
        (status = 409, description = "Already working together, or already proposed"),
    )
)]
pub async fn propose(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<ProposeCoachBody>,
) -> Result<(StatusCode, Json<CoachingRequestResponse>), ApiError> {
    let request = state
        .coaching_requests
        .propose(
            &tenant,
            ProposeCoachCommand {
                athlete_id: gym_domain::UserId::from(body.athlete_id),
                coach_id: gym_domain::UserId::from(body.coach_id),
                message: body.message,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CoachingRequestResponse {
            id: request.id.into_uuid(),
            athlete_id: request.athlete_id.into_uuid(),
            athlete_name: String::new(),
            coach_id: request.coach_id.into_uuid(),
            coach_name: String::new(),
            is_pending: request.status.is_pending(),
            is_proposal: request.is_proposal(),
            status: request.status,
            message: request.message,
            requested_at: request.requested_at,
        }),
    ))
}

/// Accept or decline a request addressed to you.
///
/// Accepting creates the coaching relationship in the same transaction.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/coaching-requests/{request_id}/answer",
    tag = "coaching",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("request_id" = Uuid, Path, description = "Request id"),
    ),
    request_body = AnswerBody,
    responses(
        (status = 200, description = "Answered", body = CoachingRequestResponse),
        (status = 403, description = "Not addressed to you"),
        (status = 404, description = "Request not found"),
        (status = 409, description = "Already answered"),
    )
)]
pub async fn answer(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, request_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<AnswerBody>,
) -> Result<Json<CoachingRequestResponse>, ApiError> {
    let request = state
        .coaching_requests
        .answer(&tenant, CoachingRequestId::from(request_id), body.into())
        .await?;

    Ok(Json(CoachingRequestResponse {
        id: request.id.into_uuid(),
        athlete_id: request.athlete_id.into_uuid(),
        athlete_name: String::new(),
        coach_id: request.coach_id.into_uuid(),
        coach_name: String::new(),
        is_pending: request.status.is_pending(),
        is_proposal: request.is_proposal(),
        status: request.status,
        message: request.message,
        requested_at: request.requested_at,
    }))
}

/// Change your mind before anyone answers.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/coaching-requests/{request_id}/withdraw",
    tag = "coaching",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("request_id" = Uuid, Path, description = "Request id"),
    ),
    responses(
        (status = 200, description = "Withdrawn", body = CoachingRequestResponse),
        (status = 404, description = "Not found, or not yours"),
        (status = 409, description = "Already answered"),
    )
)]
pub async fn withdraw(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, request_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CoachingRequestResponse>, ApiError> {
    let request = state
        .coaching_requests
        .withdraw(&tenant, CoachingRequestId::from(request_id))
        .await?;

    Ok(Json(CoachingRequestResponse {
        id: request.id.into_uuid(),
        athlete_id: request.athlete_id.into_uuid(),
        athlete_name: String::new(),
        coach_id: request.coach_id.into_uuid(),
        coach_name: String::new(),
        is_pending: request.status.is_pending(),
        is_proposal: request.is_proposal(),
        status: request.status,
        message: request.message,
        requested_at: request.requested_at,
    }))
}
