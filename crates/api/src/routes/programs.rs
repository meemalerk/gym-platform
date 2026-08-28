//! Programme authoring endpoints.
//!
//! Nested under `/gyms/{gym_id}/` like every other tenant-scoped resource, so
//! `TenantScope` resolves the caller's capacities before any handler runs.
//!
//! Note what is missing: there is no "update a published version" route, and
//! there never will be. Changing a published programme is expressed as
//! `POST /programs/{id}/versions`, which creates a new draft (ADR-0006).

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use gym_application::programs::{
    AddWeekCommand, AddWorkoutCommand, CreateProgramCommand, PrescribeExerciseCommand, Transition,
};
use gym_domain::{
    ExerciseId, ProgramId, ProgramVersionId, ProgramWeekId, WorkoutTemplateId,
    prescription::ExercisePrescription,
    program::{Program, ProgramFocus, ProgramVersion, VersionStatus},
    workout::{ProgramWeek, TemplateExercise, WorkoutTemplate},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

// ------------------------------------------------------------------ responses

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProgramVersionResponse {
    pub id: Uuid,
    pub program_id: Uuid,
    pub version_number: i32,
    pub status: VersionStatus,
    /// Convenience for clients: whether content may still be changed.
    pub is_editable: bool,
    /// Whether a member could be assigned this version.
    pub is_assignable: bool,
    pub derived_from: Option<Uuid>,
}

impl From<ProgramVersion> for ProgramVersionResponse {
    fn from(v: ProgramVersion) -> Self {
        Self {
            id: v.id.into_uuid(),
            program_id: v.program_id.into_uuid(),
            version_number: v.version_number,
            is_editable: v.status.is_mutable(),
            is_assignable: v.status.is_assignable(),
            derived_from: v.derived_from.map(ProgramVersionId::into_uuid),
            status: v.status,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ProgramResponse {
    pub id: Uuid,
    pub name: String,
    pub summary: Option<String>,
    pub focus: ProgramFocus,
    /// The newest version, which is what a list view wants to show.
    pub latest_version: ProgramVersionResponse,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WeekResponse {
    pub id: Uuid,
    pub week_number: i32,
    pub label: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WorkoutResponse {
    pub id: Uuid,
    pub week_id: Uuid,
    pub day_number: i32,
    pub name: String,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PrescribedExerciseResponse {
    pub id: Uuid,
    pub workout_id: Uuid,
    pub exercise_id: Uuid,
    /// Resolved from the catalogue so a client can render a plan without a
    /// second request and a join. The prescription is the snapshot; the name is
    /// display metadata and may legitimately drift if the catalogue is renamed.
    pub exercise_name: String,
    pub position: i32,
    pub prescription: ExercisePrescription,
    pub notes: Option<String>,
}

/// A version and everything in it — one round trip, because a plan is read whole.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct VersionContentResponse {
    pub version: ProgramVersionResponse,
    pub weeks: Vec<WeekResponse>,
    pub workouts: Vec<WorkoutResponse>,
    pub exercises: Vec<PrescribedExerciseResponse>,
}

// ------------------------------------------------------------------- requests

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateProgramRequest {
    #[schema(example = "Beginner Strength")]
    pub name: String,
    pub summary: Option<String>,
    /// What the programme is for. Defaults to `general`, which recommends
    /// nothing — honest until the coach says otherwise.
    #[serde(default = "default_focus")]
    pub focus: ProgramFocus,
}

const fn default_focus() -> ProgramFocus {
    ProgramFocus::General
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AddWeekRequest {
    #[schema(example = 1)]
    pub week_number: i32,
    #[schema(example = "Accumulation")]
    pub label: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AddWorkoutRequest {
    #[schema(example = 1)]
    pub day_number: i32,
    #[schema(example = "Upper A")]
    pub name: String,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PrescribeExerciseRequest {
    pub exercise_id: Uuid,
    pub prescription: ExercisePrescription,
    pub notes: Option<String>,
}

/// The lifecycle move to apply.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransitionRequest {
    SubmitForReview,
    Approve,
    ReturnToDraft,
    Publish,
    Archive,
}

impl From<TransitionRequest> for Transition {
    fn from(t: TransitionRequest) -> Self {
        match t {
            TransitionRequest::SubmitForReview => Self::SubmitForReview,
            TransitionRequest::Approve => Self::Approve,
            TransitionRequest::ReturnToDraft => Self::ReturnToDraft,
            TransitionRequest::Publish => Self::Publish,
            TransitionRequest::Archive => Self::Archive,
        }
    }
}

fn to_program_response(program: Program, latest: ProgramVersion) -> ProgramResponse {
    ProgramResponse {
        id: program.id.into_uuid(),
        name: program.name,
        summary: program.summary,
        focus: program.focus,
        latest_version: latest.into(),
    }
}

// ------------------------------------------------------------------- handlers

/// List the gym's programmes with their newest version.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/programs",
    tag = "programs",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Programmes", body = Vec<ProgramResponse>),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<ProgramResponse>>, ApiError> {
    let programs = state.programs.list(&tenant).await?;
    Ok(Json(
        programs
            .into_iter()
            .map(|(p, v)| to_program_response(p, v))
            .collect(),
    ))
}

/// Create a programme. Its first draft version is created with it.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/programs",
    tag = "programs",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = CreateProgramRequest,
    responses(
        (status = 201, description = "Created", body = ProgramResponse),
        (status = 400, description = "Invalid input"),
        (status = 403, description = "Caller may not manage the catalogue"),
        (status = 409, description = "A programme with that name already exists"),
    )
)]
pub async fn create(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<CreateProgramRequest>,
) -> Result<(StatusCode, Json<ProgramResponse>), ApiError> {
    let (program, version) = state
        .programs
        .create(
            &tenant,
            CreateProgramCommand {
                name: body.name,
                summary: body.summary,
                focus: body.focus,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(to_program_response(program, version)),
    ))
}

/// Every version of a programme, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/programs/{program_id}/versions",
    tag = "programs",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("program_id" = Uuid, Path, description = "Programme id"),
    ),
    responses((status = 200, description = "Versions", body = Vec<ProgramVersionResponse>))
)]
pub async fn versions(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, program_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<ProgramVersionResponse>>, ApiError> {
    let versions = state
        .programs
        .versions(&tenant, ProgramId::from(program_id))
        .await?;
    Ok(Json(versions.into_iter().map(Into::into).collect()))
}

/// Start a new draft from the newest published version.
///
/// This is how a published programme is "edited": the published version is left
/// exactly as it was, and members already assigned to it are unaffected.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/programs/{program_id}/versions",
    tag = "programs",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("program_id" = Uuid, Path, description = "Programme id"),
    ),
    responses(
        (status = 201, description = "New draft", body = ProgramVersionResponse),
        (status = 400, description = "No published version to branch from"),
        (status = 403, description = "Caller may not manage the catalogue"),
        (status = 409, description = "An open draft already exists"),
    )
)]
pub async fn new_draft(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, program_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<ProgramVersionResponse>), ApiError> {
    let draft = state
        .programs
        .new_draft(&tenant, ProgramId::from(program_id))
        .await?;
    Ok((StatusCode::CREATED, Json(draft.into())))
}

/// A version and all of its content.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/program-versions/{version_id}",
    tag = "programs",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("version_id" = Uuid, Path, description = "Version id"),
    ),
    responses(
        (status = 200, description = "Version content", body = VersionContentResponse),
        (status = 404, description = "Not found"),
    )
)]
pub async fn content(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, version_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<VersionContentResponse>, ApiError> {
    let content = state
        .programs
        .content(&tenant, ProgramVersionId::from(version_id))
        .await?;

    // One catalogue read to name every prescribed exercise. The catalogue is
    // tenant-scoped and readable by any member, so this leaks nothing the
    // caller could not already see via /exercises.
    let names: std::collections::HashMap<Uuid, String> = state
        .exercises
        .list(&tenant)
        .await?
        .into_iter()
        .map(|e| (e.id.into_uuid(), e.name))
        .collect();

    Ok(Json(VersionContentResponse {
        version: content.version.into(),
        weeks: content
            .weeks
            .into_iter()
            .map(|w: ProgramWeek| WeekResponse {
                id: w.id.into_uuid(),
                week_number: w.week_number,
                label: w.label,
            })
            .collect(),
        workouts: content
            .workouts
            .into_iter()
            .map(|w: WorkoutTemplate| WorkoutResponse {
                id: w.id.into_uuid(),
                week_id: w.week_id.into_uuid(),
                day_number: w.day_number,
                name: w.name,
                notes: w.notes,
            })
            .collect(),
        exercises: content
            .exercises
            .into_iter()
            .map(|e: TemplateExercise| PrescribedExerciseResponse {
                id: e.id.into_uuid(),
                workout_id: e.workout_id.into_uuid(),
                // The FK guarantees the id exists; the fallback only guards a
                // catalogue read racing a concurrent rename-and-delete.
                exercise_name: names
                    .get(&e.exercise_id.into_uuid())
                    .cloned()
                    .unwrap_or_else(|| "Exercise".to_owned()),
                exercise_id: e.exercise_id.into_uuid(),
                position: e.position,
                prescription: e.prescription,
                notes: e.notes,
            })
            .collect(),
    }))
}

/// Add a week to a draft version.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/program-versions/{version_id}/weeks",
    tag = "programs",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("version_id" = Uuid, Path, description = "Version id"),
    ),
    request_body = AddWeekRequest,
    responses(
        (status = 201, description = "Created", body = WeekResponse),
        (status = 400, description = "Invalid input, or the version is no longer editable"),
        (status = 403, description = "Caller may not manage the catalogue"),
        (status = 409, description = "That week already exists"),
    )
)]
pub async fn add_week(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, version_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<AddWeekRequest>,
) -> Result<(StatusCode, Json<WeekResponse>), ApiError> {
    let week = state
        .programs
        .add_week(
            &tenant,
            AddWeekCommand {
                version_id: ProgramVersionId::from(version_id),
                week_number: body.week_number,
                label: body.label,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(WeekResponse {
            id: week.id.into_uuid(),
            week_number: week.week_number,
            label: week.label,
        }),
    ))
}

/// Add a workout to a week.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/program-weeks/{week_id}/workouts",
    tag = "programs",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("week_id" = Uuid, Path, description = "Week id"),
    ),
    request_body = AddWorkoutRequest,
    responses(
        (status = 201, description = "Created", body = WorkoutResponse),
        (status = 400, description = "Invalid input, or the version is no longer editable"),
        (status = 409, description = "That day already exists in this week"),
    )
)]
pub async fn add_workout(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, week_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<AddWorkoutRequest>,
) -> Result<(StatusCode, Json<WorkoutResponse>), ApiError> {
    let workout = state
        .programs
        .add_workout(
            &tenant,
            AddWorkoutCommand {
                week_id: ProgramWeekId::from(week_id),
                day_number: body.day_number,
                name: body.name,
                notes: body.notes,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(WorkoutResponse {
            id: workout.id.into_uuid(),
            week_id: workout.week_id.into_uuid(),
            day_number: workout.day_number,
            name: workout.name,
            notes: workout.notes,
        }),
    ))
}

/// Prescribe an exercise inside a workout.
///
/// The prescription must match how the exercise is measured — reps cannot be
/// prescribed for something measured in metres.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/workout-templates/{workout_id}/exercises",
    tag = "programs",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("workout_id" = Uuid, Path, description = "Workout id"),
    ),
    request_body = PrescribeExerciseRequest,
    responses(
        (status = 201, description = "Prescribed", body = PrescribedExerciseResponse),
        (status = 400, description = "Prescription does not suit the exercise, or version frozen"),
        (status = 404, description = "Workout or exercise not found in this gym"),
    )
)]
pub async fn prescribe(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, workout_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<PrescribeExerciseRequest>,
) -> Result<(StatusCode, Json<PrescribedExerciseResponse>), ApiError> {
    let (prescribed, exercise_name) = state
        .programs
        .prescribe(
            &tenant,
            PrescribeExerciseCommand {
                workout_id: WorkoutTemplateId::from(workout_id),
                exercise_id: ExerciseId::from(body.exercise_id),
                prescription: body.prescription,
                notes: body.notes,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(PrescribedExerciseResponse {
            id: prescribed.id.into_uuid(),
            workout_id: prescribed.workout_id.into_uuid(),
            exercise_id: prescribed.exercise_id.into_uuid(),
            exercise_name,
            position: prescribed.position,
            prescription: prescribed.prescription,
            notes: prescribed.notes,
        }),
    ))
}

/// Move a version through the lifecycle.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/program-versions/{version_id}/transition",
    tag = "programs",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("version_id" = Uuid, Path, description = "Version id"),
    ),
    request_body = TransitionRequest,
    responses(
        (status = 200, description = "New state", body = ProgramVersionResponse),
        (status = 400, description = "Not a legal move from the current state"),
        (status = 403, description = "Caller may not manage the catalogue"),
        (status = 404, description = "Version not found"),
    )
)]
pub async fn transition(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, version_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<TransitionRequest>,
) -> Result<Json<ProgramVersionResponse>, ApiError> {
    let version = state
        .programs
        .transition(&tenant, ProgramVersionId::from(version_id), body.into())
        .await?;
    Ok(Json(version.into()))
}
