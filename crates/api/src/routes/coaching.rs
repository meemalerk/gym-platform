//! Coach–athlete relationship endpoints.
//!
//! Note there is no DELETE. Ending a relationship is
//! `POST /coach-relationships/{id}/end`, because the record survives — a coach
//! who stops working with an athlete leaves behind programmes they wrote, and
//! that work has to stay attributable to them.

use axum::{
    Json,
    extract::{Path, State},
};
use gym_domain::{
    CoachRelationshipId,
    coaching::{CoachRelationship, RelationshipStatus},
};
use serde::Serialize;
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CoachRelationshipResponse {
    pub id: Uuid,
    pub coach_id: Uuid,
    /// Resolved for display, so a client list needs no second request per row.
    pub coach_name: Option<String>,
    pub athlete_id: Uuid,
    pub athlete_name: Option<String>,
    pub status: RelationshipStatus,
    pub is_active: bool,
}

impl CoachRelationshipResponse {
    fn from_parts(
        relationship: CoachRelationship,
        coach_name: Option<String>,
        athlete_name: Option<String>,
    ) -> Self {
        Self {
            id: relationship.id.into_uuid(),
            coach_id: relationship.coach_id.into_uuid(),
            coach_name,
            athlete_id: relationship.athlete_id.into_uuid(),
            athlete_name,
            is_active: relationship.status.is_active(),
            status: relationship.status,
        }
    }
}

/// List coaching relationships the caller may see.
///
/// A manager or head coach sees the whole roster. Anyone else sees only the
/// relationships they are personally part of — a trainer's clients, or a
/// member's coaches. Being able to coach *someone* never implies seeing
/// *everyone*.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/coach-relationships",
    tag = "coaching",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Relationships", body = Vec<CoachRelationshipResponse>),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<CoachRelationshipResponse>>, ApiError> {
    let views = state.coaching.list(&tenant).await?;

    Ok(Json(
        views
            .into_iter()
            .map(|v| {
                CoachRelationshipResponse::from_parts(
                    v.relationship,
                    Some(v.coach_name),
                    Some(v.athlete_name),
                )
            })
            .collect(),
    ))
}

/// End a coaching relationship. The record survives; only its status changes.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/coach-relationships/{relationship_id}/end",
    tag = "coaching",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("relationship_id" = Uuid, Path, description = "Relationship id"),
    ),
    responses(
        (status = 200, description = "Ended", body = CoachRelationshipResponse),
        (status = 403, description = "Only head coaches and above may end relationships"),
        (status = 404, description = "Relationship not found in this gym"),
        (status = 409, description = "Already ended"),
    )
)]
pub async fn end(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, relationship_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CoachRelationshipResponse>, ApiError> {
    let relationship = state
        .coaching
        .end(&tenant, CoachRelationshipId::from(relationship_id))
        .await?;

    Ok(Json(CoachRelationshipResponse::from_parts(
        relationship,
        None,
        None,
    )))
}
