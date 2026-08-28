//! Workout execution endpoints.
//!
//! The client supplies the ids (ADR-0008): a phone mints UUIDv7s offline and
//! syncs when it can. `POST` with an id the server has seen returns the existing
//! record with 200 instead of 201 — same request, same outcome, however many
//! times it is retried.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use gym_application::execution::{FinishOutcome, LogSetCommand, StartSessionCommand};
use gym_application::ports::SessionView;
use gym_domain::{
    AssignmentId, ExerciseId, PerformedSetId, TemplateExerciseId, WorkoutSessionId,
    WorkoutTemplateId,
    execution::{PerformedSet, PerformedValue, SessionStatus, WorkoutSession},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct StartSessionRequest {
    /// Client-generated UUIDv7. Replaying a known id is a no-op.
    pub id: Uuid,
    /// The assignment being executed. Omit **both** this and
    /// `workout_template_id` to start an unplanned session the member builds
    /// themselves (ADR-0035) — the shape someone on an Open Gym membership
    /// uses, having no coach and so no prescription. One without the other is
    /// a 400.
    #[serde(default)]
    #[schema(nullable)]
    pub assignment_id: Option<Uuid>,
    #[serde(default)]
    #[schema(nullable)]
    pub workout_template_id: Option<Uuid>,
    /// What to call an unplanned session — at most 80 characters, blank for
    /// none. Refused alongside an assignment: a prescribed workout is named by
    /// its template.
    #[serde(default)]
    #[schema(nullable)]
    pub title: Option<String>,
    /// When the workout actually began — client wall-clock, which may be hours
    /// before this request arrives.
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LogSetRequest {
    /// Client-generated, like the session id.
    pub id: Uuid,
    pub exercise_id: Uuid,
    #[schema(nullable)]
    pub template_exercise_id: Option<Uuid>,
    pub set_number: i32,
    pub performed: PerformedValue,
    #[schema(nullable)]
    pub rpe: Option<u8>,
}

/// How the session ended, and when — by the athlete's clock.
///
/// Accepts the bare string (`"completed"`) as well as the object form, so the
/// pre-`ended_at` clients keep working unchanged. Untagged is safe here
/// precisely because the two shapes cannot be confused: one is a string, the
/// other an object.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(untagged)]
pub enum FinishRequest {
    /// The original shape. No end time, so duration falls back to the server's.
    Outcome(FinishOutcomeRequest),
    Detailed {
        outcome: FinishOutcomeRequest,
        /// Client wall-clock, paired with `started_at`. Dropped by the domain
        /// if it cannot be true rather than failing the request — an athlete
        /// with a drifting phone clock must still be able to close a workout.
        #[serde(default)]
        ended_at: Option<DateTime<Utc>>,
    },
}

#[derive(Debug, Clone, Copy, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FinishOutcomeRequest {
    Completed,
    Abandoned,
}

impl FinishRequest {
    const fn parts(&self) -> (FinishOutcomeRequest, Option<DateTime<Utc>>) {
        match self {
            Self::Outcome(outcome) => (*outcome, None),
            Self::Detailed { outcome, ended_at } => (*outcome, *ended_at),
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SessionResponse {
    pub id: Uuid,
    pub athlete_id: Uuid,
    pub athlete_name: Option<String>,
    /// Null for an unplanned session, along with `workout_template_id`,
    /// `workout_name` and `program_name` — there is no plan behind it.
    #[schema(nullable)]
    pub assignment_id: Option<Uuid>,
    #[schema(nullable)]
    pub workout_template_id: Option<Uuid>,
    pub workout_name: Option<String>,
    pub program_name: Option<String>,
    /// What the member called an unplanned session. Null for a prescribed one,
    /// which is named by `workout_name`. Exactly one of the two is ever set,
    /// so a client picks whichever it finds.
    #[schema(nullable)]
    pub title: Option<String>,
    pub started_at: DateTime<Utc>,
    pub status: SessionStatus,
    pub is_open: bool,
    pub set_count: Option<i64>,
    /// When the athlete stopped, by their own clock. Null for open sessions and
    /// for history recorded before this was captured.
    #[schema(nullable)]
    pub ended_at: Option<DateTime<Utc>>,
    /// How long they trained, in seconds.
    ///
    /// Computed here rather than in the client, and computed rather than
    /// stored: it is `ended_at - started_at` with a documented fallback to the
    /// server's completion time for older rows, and putting that rule in one
    /// place is the difference between every surface agreeing and most of them
    /// agreeing. Null while a session is open — that is elapsed time, which is
    /// a different thing.
    #[schema(nullable)]
    pub duration_seconds: Option<i64>,
}

impl SessionResponse {
    /// A session with no display metadata to hand — the responses to *start*
    /// and *finish*, which return the row they just wrote. Clients re-read the
    /// list or the detail for names; these nulls are honest, not an oversight.
    fn from_session(session: WorkoutSession) -> Self {
        Self {
            id: session.id.into_uuid(),
            athlete_id: session.athlete_id.into_uuid(),
            athlete_name: None,
            assignment_id: session.assignment_id.map(AssignmentId::into_uuid),
            workout_template_id: session
                .workout_template_id
                .map(WorkoutTemplateId::into_uuid),
            workout_name: None,
            program_name: None,
            title: session.title.clone(),
            started_at: session.started_at,
            is_open: session.status.is_open(),
            ended_at: session.ended_at,
            duration_seconds: session.duration().map(|d| d.num_seconds()),
            status: session.status,
            set_count: None,
        }
    }

    /// The one place a session with its names becomes a response. Both the list
    /// and the detail endpoint go through it, because they did not: detail built
    /// itself from the bare domain session and so returned `workout_name: null`,
    /// which left the *live logging screen* unable to name what was being
    /// trained.
    fn from_view(v: SessionView) -> Self {
        Self {
            id: v.session.id.into_uuid(),
            athlete_id: v.session.athlete_id.into_uuid(),
            athlete_name: Some(v.athlete_name),
            assignment_id: v.session.assignment_id.map(AssignmentId::into_uuid),
            workout_template_id: v
                .session
                .workout_template_id
                .map(WorkoutTemplateId::into_uuid),
            workout_name: v.workout_name,
            program_name: v.program_name,
            title: v.session.title.clone(),
            started_at: v.session.started_at,
            is_open: v.session.status.is_open(),
            ended_at: v.session.ended_at,
            duration_seconds: v.session.duration().map(|d| d.num_seconds()),
            status: v.session.status,
            set_count: Some(v.set_count),
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PerformedSetResponse {
    pub id: Uuid,
    pub session_id: Uuid,
    pub exercise_id: Uuid,
    pub template_exercise_id: Option<Uuid>,
    pub set_number: i32,
    pub performed: PerformedValue,
    pub rpe: Option<u8>,
}

impl From<PerformedSet> for PerformedSetResponse {
    fn from(s: PerformedSet) -> Self {
        Self {
            id: s.id.into_uuid(),
            session_id: s.session_id.into_uuid(),
            exercise_id: s.exercise_id.into_uuid(),
            template_exercise_id: s.template_exercise_id.map(TemplateExerciseId::into_uuid),
            set_number: s.set_number,
            performed: s.performed,
            rpe: s.rpe,
        }
    }
}

/// A session and every set in it — the read a logging screen sits on.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SessionDetailResponse {
    pub session: SessionResponse,
    pub sets: Vec<PerformedSetResponse>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct HistoryQuery {
    /// Whose history. Omitted means your own; anyone else's needs coaching or
    /// management standing over them.
    pub athlete_id: Option<Uuid>,
}

/// One session's worth of this exercise, oldest first.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ExerciseHistoryEntryResponse {
    pub session_id: Uuid,
    pub started_at: DateTime<Utc>,
    /// `in_progress` / `completed` / `abandoned` — a set that happened is
    /// history regardless of how its session ended, so none are filtered here.
    pub session_status: String,
    pub sets: Vec<PerformedSetResponse>,
}

/// Every set of one exercise for one athlete, grouped by session, oldest first.
///
/// Raw truth only: estimated maxes and trends are computed by the client, never
/// stored — a derived number that lives in two places will disagree eventually.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/exercises/{exercise_id}/history",
    tag = "execution",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("exercise_id" = Uuid, Path, description = "Exercise id"),
        HistoryQuery,
    ),
    responses(
        (status = 200, description = "History", body = Vec<ExerciseHistoryEntryResponse>),
        (status = 404, description = "Not found, or not yours to see"),
    )
)]
pub async fn exercise_history(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, exercise_id)): Path<(Uuid, Uuid)>,
    axum::extract::Query(query): axum::extract::Query<HistoryQuery>,
) -> Result<Json<Vec<ExerciseHistoryEntryResponse>>, ApiError> {
    let athlete = query
        .athlete_id
        .map_or(tenant.actor_id, gym_domain::UserId::from);

    let entries = state
        .execution
        .exercise_history(&tenant, ExerciseId::from(exercise_id), athlete)
        .await?;

    Ok(Json(
        entries
            .into_iter()
            .map(|e| ExerciseHistoryEntryResponse {
                session_id: e.session_id.into_uuid(),
                started_at: e.started_at,
                session_status: e.session_status,
                sets: e.sets.into_iter().map(Into::into).collect(),
            })
            .collect(),
    ))
}

/// Narrowing for the list. Every field optional; omitting all of them is the
/// original behaviour.
#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct SessionListQuery {
    /// One athlete's history. Narrows what you may already see — it cannot
    /// widen it, so asking about a stranger returns nothing rather than 403.
    pub athlete_id: Option<Uuid>,
    /// Inclusive, by UTC date.
    pub from: Option<chrono::NaiveDate>,
    /// Inclusive, by UTC date.
    pub to: Option<chrono::NaiveDate>,
    /// Clamped to 500 server-side.
    pub limit: Option<i64>,
}

/// List sessions the caller may see (manager: the gym's; otherwise your own and
/// your clients'). Newest first.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/workout-sessions",
    tag = "execution",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id"), SessionListQuery),
    responses(
        (status = 200, description = "Sessions", body = Vec<SessionResponse>),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    tenant: TenantScope,
    axum::extract::Query(query): axum::extract::Query<SessionListQuery>,
) -> Result<Json<Vec<SessionResponse>>, ApiError> {
    let views = state
        .execution
        .list(
            &tenant,
            &gym_application::ports::SessionFilter {
                athlete_id: query.athlete_id.map(gym_domain::UserId::from),
                from: query.from,
                to: query.to,
                limit: query.limit,
            },
        )
        .await?;

    Ok(Json(
        views.into_iter().map(SessionResponse::from_view).collect(),
    ))
}

/// Start a workout session against your own assignment.
///
/// Only the athlete starts their own session — a coach decides what you should
/// do; only you say what you did. 201 on creation, 200 on an idempotent replay.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/workout-sessions",
    tag = "execution",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = StartSessionRequest,
    responses(
        (status = 201, description = "Started", body = SessionResponse),
        (status = 200, description = "Already started (idempotent replay)", body = SessionResponse),
        (status = 400, description = "Half a plan link, a title on an assigned session,                                       a workout not in the assigned version, or a bad timestamp"),
        (status = 403, description = "Not your assignment"),
        (status = 404, description = "Assignment or workout not found"),
    )
)]
pub async fn start(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<StartSessionRequest>,
) -> Result<(StatusCode, Json<SessionResponse>), ApiError> {
    let (session, created) = state
        .execution
        .start(
            &tenant,
            StartSessionCommand {
                id: WorkoutSessionId::from(body.id),
                assignment_id: body.assignment_id.map(AssignmentId::from),
                workout_template_id: body.workout_template_id.map(WorkoutTemplateId::from),
                title: body.title,
                started_at: body.started_at,
            },
        )
        .await?;

    let code = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((code, Json(SessionResponse::from_session(session))))
}

/// One session with all its sets.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/workout-sessions/{session_id}",
    tag = "execution",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("session_id" = Uuid, Path, description = "Session id"),
    ),
    responses(
        (status = 200, description = "Session detail", body = SessionDetailResponse),
        (status = 404, description = "Not found, or not yours to see"),
    )
)]
pub async fn detail(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, session_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<SessionDetailResponse>, ApiError> {
    let (view, sets) = state
        .execution
        .session_with_sets(&tenant, WorkoutSessionId::from(session_id))
        .await?;

    Ok(Json(SessionDetailResponse {
        session: SessionResponse::from_view(view),
        sets: sets.into_iter().map(Into::into).collect(),
    }))
}

/// Log one performed set into your open session. Idempotent on the set id.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/workout-sessions/{session_id}/sets",
    tag = "execution",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("session_id" = Uuid, Path, description = "Session id"),
    ),
    request_body = LogSetRequest,
    responses(
        (status = 201, description = "Logged", body = PerformedSetResponse),
        (status = 400, description = "Values out of range, or not your session"),
        (status = 404, description = "Session not found"),
        (status = 409, description = "Session already finished, or set number taken"),
    )
)]
pub async fn log_set(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, session_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<LogSetRequest>,
) -> Result<(StatusCode, Json<PerformedSetResponse>), ApiError> {
    let set = state
        .execution
        .log_set(
            &tenant,
            LogSetCommand {
                id: PerformedSetId::from(body.id),
                session_id: WorkoutSessionId::from(session_id),
                exercise_id: ExerciseId::from(body.exercise_id),
                template_exercise_id: body.template_exercise_id.map(TemplateExerciseId::from),
                set_number: body.set_number,
                performed: body.performed,
                rpe: body.rpe,
            },
        )
        .await?;

    Ok((StatusCode::CREATED, Json(set.into())))
}

/// Finish your session — completed, or honestly abandoned.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/workout-sessions/{session_id}/finish",
    tag = "execution",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("session_id" = Uuid, Path, description = "Session id"),
    ),
    request_body = FinishRequest,
    responses(
        (status = 200, description = "Finished", body = SessionResponse),
        (status = 403, description = "Not your session"),
        (status = 404, description = "Session not found"),
        (status = 409, description = "Already finished"),
    )
)]
pub async fn finish(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, session_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<FinishRequest>,
) -> Result<Json<SessionResponse>, ApiError> {
    let (outcome, ended_at) = body.parts();
    let outcome = match outcome {
        FinishOutcomeRequest::Completed => FinishOutcome::Completed,
        FinishOutcomeRequest::Abandoned => FinishOutcome::Abandoned,
    };

    let session = state
        .execution
        .finish(
            &tenant,
            WorkoutSessionId::from(session_id),
            outcome,
            ended_at,
        )
        .await?;

    Ok(Json(SessionResponse::from_session(session)))
}
