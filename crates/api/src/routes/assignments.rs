//! Programme assignment endpoints.
//!
//! `POST` requires a **version id**, not a programme id — the caller chooses
//! exactly which published snapshot the athlete is put on (ADR-0006). There is
//! no "assign latest": publishing a new version must never silently move anyone.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::NaiveDate;
use gym_application::assignments::AssignProgramCommand;
use gym_domain::{
    AssignmentId, ProgramVersionId, UserId,
    assignment::{AssignmentStatus, ProgramAssignment},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AssignProgramRequest {
    pub athlete_id: Uuid,
    pub program_version_id: Uuid,
    /// The athlete's first day, e.g. `2026-07-21`. Sent by the client rather
    /// than defaulted server-side, because "today" is a timezone question and
    /// the coach is the one looking at a calendar.
    #[schema(example = "2026-07-21")]
    pub start_date: NaiveDate,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AssignmentResponse {
    pub id: Uuid,
    pub athlete_id: Uuid,
    pub athlete_name: Option<String>,
    pub program_id: Uuid,
    pub program_version_id: Uuid,
    pub program_name: Option<String>,
    pub version_number: Option<i32>,
    pub start_date: NaiveDate,
    pub status: AssignmentStatus,
    pub is_active: bool,
}

impl AssignmentResponse {
    fn from_parts(
        assignment: ProgramAssignment,
        athlete_name: Option<String>,
        program_name: Option<String>,
        version_number: Option<i32>,
    ) -> Self {
        Self {
            id: assignment.id.into_uuid(),
            athlete_id: assignment.athlete_id.into_uuid(),
            athlete_name,
            program_id: assignment.program_id.into_uuid(),
            program_version_id: assignment.program_version_id.into_uuid(),
            program_name,
            version_number,
            start_date: assignment.start_date,
            is_active: assignment.status.is_active(),
            status: assignment.status,
        }
    }
}

/// List assignments the caller may see.
///
/// A manager sees the gym's; a trainer sees their clients' and their own; a
/// member sees their own. The server scopes — the client renders what it gets.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/program-assignments",
    tag = "assignments",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Assignments", body = Vec<AssignmentResponse>),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<AssignmentResponse>>, ApiError> {
    let views = state.assignments.list(&tenant).await?;

    Ok(Json(
        views
            .into_iter()
            .map(|v| {
                AssignmentResponse::from_parts(
                    v.assignment,
                    Some(v.athlete_name),
                    Some(v.program_name),
                    Some(v.version_number),
                )
            })
            .collect(),
    ))
}

/// Assign a published programme version to an athlete.
///
/// Managers may assign to anyone; a trainer only to their own clients — the
/// coach–athlete relationship is the authority, not the `trainer` capacity.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/program-assignments",
    tag = "assignments",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = AssignProgramRequest,
    responses(
        (status = 201, description = "Assigned", body = AssignmentResponse),
        (status = 400, description = "Version is not published, or start date unreasonable"),
        (status = 403, description = "Caller does not coach this athlete"),
        (status = 404, description = "Athlete or version not found in this gym"),
        (status = 409, description = "Already actively assigned to this programme"),
    )
)]
pub async fn assign(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<AssignProgramRequest>,
) -> Result<(StatusCode, Json<AssignmentResponse>), ApiError> {
    let assignment = state
        .assignments
        .assign(
            &tenant,
            AssignProgramCommand {
                athlete_id: UserId::from(body.athlete_id),
                program_version_id: ProgramVersionId::from(body.program_version_id),
                start_date: body.start_date,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(AssignmentResponse::from_parts(assignment, None, None, None)),
    ))
}

/// Take an athlete off a programme. The record survives.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/program-assignments/{assignment_id}/withdraw",
    tag = "assignments",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("assignment_id" = Uuid, Path, description = "Assignment id"),
    ),
    responses(
        (status = 200, description = "Withdrawn", body = AssignmentResponse),
        (status = 403, description = "Caller does not coach this athlete"),
        (status = 404, description = "Assignment not found in this gym"),
        (status = 409, description = "Already ended"),
    )
)]
pub async fn withdraw(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, assignment_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<AssignmentResponse>, ApiError> {
    let assignment = state
        .assignments
        .withdraw(&tenant, AssignmentId::from(assignment_id))
        .await?;

    Ok(Json(AssignmentResponse::from_parts(
        assignment, None, None, None,
    )))
}
