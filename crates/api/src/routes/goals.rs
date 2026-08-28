//! Goal endpoints.
//!
//! The one place self-service is the point: a member sets their own goals, a
//! coach sets goals for clients, and either closes them. Progress never appears
//! in these responses — it is computed by the client against the live series
//! (measurements, exercise history), so it cannot go stale here.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, NaiveDate, Utc};
use gym_application::goals::{CreateGoalCommand, GoalOutcome};
use gym_domain::{
    GoalId, UserId,
    goal::{Goal, GoalMetric, GoalStatus},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateGoalRequest {
    pub athlete_id: Uuid,
    pub metric: GoalMetric,
    #[schema(nullable)]
    pub target_date: Option<NaiveDate>,
}

/// How the goal closes.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CloseGoalRequest {
    Achieved,
    Abandoned,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct GoalResponse {
    pub id: Uuid,
    pub athlete_id: Uuid,
    pub athlete_name: Option<String>,
    pub set_by: Uuid,
    pub metric: GoalMetric,
    pub target_date: Option<NaiveDate>,
    pub status: GoalStatus,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

impl GoalResponse {
    fn from_goal(goal: Goal, athlete_name: Option<String>) -> Self {
        Self {
            id: goal.id.into_uuid(),
            athlete_id: goal.athlete_id.into_uuid(),
            athlete_name,
            set_by: goal.set_by.into_uuid(),
            metric: goal.metric,
            target_date: goal.target_date,
            is_active: goal.status.is_active(),
            status: goal.status,
            created_at: goal.created_at,
        }
    }
}

/// Goals the caller may see: the gym's for a manager, their clients' and their
/// own for everyone else.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/goals",
    tag = "goals",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Goals", body = Vec<GoalResponse>),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<GoalResponse>>, ApiError> {
    let views = state.goals.list(&tenant).await?;
    Ok(Json(
        views
            .into_iter()
            .map(|v| GoalResponse::from_goal(v.goal, Some(v.athlete_name)))
            .collect(),
    ))
}

/// Set a goal — your own, or a client's. The metric carries its baseline,
/// captured now; without it progress has no denominator.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/goals",
    tag = "goals",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = CreateGoalRequest,
    responses(
        (status = 201, description = "Set", body = GoalResponse),
        (status = 400, description = "Implausible numbers, or a dream-length deadline"),
        (status = 403, description = "Not your goal to set"),
        (status = 404, description = "Athlete or exercise not found in this gym"),
    )
)]
pub async fn create(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<CreateGoalRequest>,
) -> Result<(StatusCode, Json<GoalResponse>), ApiError> {
    let goal = state
        .goals
        .create(
            &tenant,
            CreateGoalCommand {
                athlete_id: UserId::from(body.athlete_id),
                metric: body.metric,
                target_date: body.target_date,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(GoalResponse::from_goal(goal, None)),
    ))
}

/// Close a goal — achieved (a human confirms; the data only suggests) or
/// abandoned. Either way the record survives.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/goals/{goal_id}/close",
    tag = "goals",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("goal_id" = Uuid, Path, description = "Goal id"),
    ),
    request_body = CloseGoalRequest,
    responses(
        (status = 200, description = "Closed", body = GoalResponse),
        (status = 403, description = "Not your goal to close"),
        (status = 404, description = "Goal not found"),
        (status = 409, description = "Already closed"),
    )
)]
pub async fn close(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, goal_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<CloseGoalRequest>,
) -> Result<Json<GoalResponse>, ApiError> {
    let outcome = match body {
        CloseGoalRequest::Achieved => GoalOutcome::Achieved,
        CloseGoalRequest::Abandoned => GoalOutcome::Abandoned,
    };

    let goal = state
        .goals
        .close(&tenant, GoalId::from(goal_id), outcome)
        .await?;
    Ok(Json(GoalResponse::from_goal(goal, None)))
}
