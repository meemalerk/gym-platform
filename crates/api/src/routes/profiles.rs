//! Profile endpoints.
//!
//! `/me/...` routes are person-scoped — `Authenticated`, no gym in the URL —
//! because a profile follows the account between gyms (ADR-0014). The one
//! gym-scoped route is a coach reading their client's athlete profile, which is
//! a relationship question and therefore lives under `/gyms/{id}/`.

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::NaiveDate;
use gym_application::profiles::{UpdateAthleteProfileCommand, UpdateTrainerProfileCommand};
use gym_domain::{
    UserId,
    measurement::BodyMeasurement,
    profile::{AthleteProfile, TrainerProfile},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::ApiError,
    extract::{Authenticated, TenantScope},
    state::AppState,
};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AthleteProfileResponse {
    pub goals: Option<String>,
    pub training_age_months: Option<i32>,
    pub limitations: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub height_cm: Option<i32>,
}

impl From<AthleteProfile> for AthleteProfileResponse {
    fn from(p: AthleteProfile) -> Self {
        Self {
            goals: p.goals,
            training_age_months: p.training_age_months,
            limitations: p.limitations,
            date_of_birth: p.date_of_birth,
            height_cm: p.height_cm,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeasurementResponse {
    pub measured_on: NaiveDate,
    pub weight_kg: Option<f64>,
    pub body_fat_percent: Option<f64>,
    pub waist_cm: Option<f64>,
    pub hip_cm: Option<f64>,
    pub chest_cm: Option<f64>,
    pub arm_cm: Option<f64>,
    pub thigh_cm: Option<f64>,
    pub notes: Option<String>,
}

impl From<BodyMeasurement> for MeasurementResponse {
    fn from(m: BodyMeasurement) -> Self {
        Self {
            measured_on: m.measured_on,
            weight_kg: m.weight_kg,
            body_fat_percent: m.body_fat_percent,
            waist_cm: m.waist_cm,
            hip_cm: m.hip_cm,
            chest_cm: m.chest_cm,
            arm_cm: m.arm_cm,
            thigh_cm: m.thigh_cm,
            notes: m.notes,
        }
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SaveMeasurementRequest {
    pub weight_kg: Option<f64>,
    pub body_fat_percent: Option<f64>,
    pub waist_cm: Option<f64>,
    pub hip_cm: Option<f64>,
    pub chest_cm: Option<f64>,
    pub arm_cm: Option<f64>,
    pub thigh_cm: Option<f64>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TrainerProfileResponse {
    pub headline: Option<String>,
    pub bio: Option<String>,
    pub certifications: Vec<String>,
    pub specialties: Vec<String>,
}

impl From<TrainerProfile> for TrainerProfileResponse {
    fn from(p: TrainerProfile) -> Self {
        Self {
            headline: p.headline,
            bio: p.bio,
            certifications: p.certifications,
            specialties: p.specialties,
        }
    }
}

/// Both profiles at once. `null` means never filled in — an invitation, not an
/// error.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MyProfilesResponse {
    pub athlete: Option<AthleteProfileResponse>,
    pub trainer: Option<TrainerProfileResponse>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateAthleteProfileRequest {
    pub goals: Option<String>,
    pub training_age_months: Option<i32>,
    pub limitations: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub height_cm: Option<i32>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateTrainerProfileRequest {
    pub headline: Option<String>,
    pub bio: Option<String>,
    #[serde(default)]
    pub certifications: Vec<String>,
    #[serde(default)]
    pub specialties: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RenameRequest {
    #[schema(example = "Alex P.")]
    pub display_name: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RenameResponse {
    pub display_name: String,
}

/// The caller's own profiles.
#[utoipa::path(
    get,
    path = "/api/v1/me/profiles",
    tag = "profiles",
    security(("bearer" = [])),
    responses((status = 200, description = "Profiles", body = MyProfilesResponse))
)]
pub async fn my_profiles(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
) -> Result<Json<MyProfilesResponse>, ApiError> {
    let (athlete, trainer) = state.profiles.my_profiles(user_id).await?;
    Ok(Json(MyProfilesResponse {
        athlete: athlete.map(Into::into),
        trainer: trainer.map(Into::into),
    }))
}

/// Replace the caller's athlete profile. A full replace — omitted fields clear.
#[utoipa::path(
    put,
    path = "/api/v1/me/profiles/athlete",
    tag = "profiles",
    security(("bearer" = [])),
    request_body = UpdateAthleteProfileRequest,
    responses(
        (status = 200, description = "Saved", body = AthleteProfileResponse),
        (status = 400, description = "A field is out of bounds"),
    )
)]
pub async fn update_athlete(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
    Json(body): Json<UpdateAthleteProfileRequest>,
) -> Result<Json<AthleteProfileResponse>, ApiError> {
    let profile = state
        .profiles
        .update_athlete(
            user_id,
            UpdateAthleteProfileCommand {
                goals: body.goals,
                training_age_months: body.training_age_months,
                limitations: body.limitations,
                date_of_birth: body.date_of_birth,
                height_cm: body.height_cm,
            },
        )
        .await?;
    Ok(Json(profile.into()))
}

/// Replace the caller's trainer profile.
#[utoipa::path(
    put,
    path = "/api/v1/me/profiles/trainer",
    tag = "profiles",
    security(("bearer" = [])),
    request_body = UpdateTrainerProfileRequest,
    responses(
        (status = 200, description = "Saved", body = TrainerProfileResponse),
        (status = 400, description = "A field is out of bounds"),
    )
)]
pub async fn update_trainer(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
    Json(body): Json<UpdateTrainerProfileRequest>,
) -> Result<Json<TrainerProfileResponse>, ApiError> {
    let profile = state
        .profiles
        .update_trainer(
            user_id,
            UpdateTrainerProfileCommand {
                headline: body.headline,
                bio: body.bio,
                certifications: body.certifications,
                specialties: body.specialties,
            },
        )
        .await?;
    Ok(Json(profile.into()))
}

/// Rename the account. The name appears in rosters, coaching lists and the
/// audit trail, so it is bounded like any other name.
#[utoipa::path(
    patch,
    path = "/api/v1/me",
    tag = "profiles",
    security(("bearer" = [])),
    request_body = RenameRequest,
    responses(
        (status = 200, description = "Renamed", body = RenameResponse),
        (status = 400, description = "Name empty or too long"),
    )
)]
pub async fn rename(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
    Json(body): Json<RenameRequest>,
) -> Result<Json<RenameResponse>, ApiError> {
    let display_name = state.profiles.rename(user_id, &body.display_name).await?;
    Ok(Json(RenameResponse { display_name }))
}

/// A member's athlete profile, read by their coach or a gym manager.
///
/// Limitations and goals are what a coach must know before writing week one —
/// and what a fellow member has no business reading. Anyone without standing
/// gets 404, never confirmation the person trains here.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/members/{user_id}/athlete-profile",
    tag = "profiles",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("user_id" = Uuid, Path, description = "The athlete"),
    ),
    responses(
        (status = 200, description = "Profile (all-null when never filled in)", body = AthleteProfileResponse),
        (status = 404, description = "Not found, or not yours to see"),
    )
)]
pub async fn athlete_profile_of(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, athlete)): Path<(Uuid, Uuid)>,
) -> Result<Json<AthleteProfileResponse>, ApiError> {
    let profile = state
        .profiles
        .athlete_profile_of(&tenant, UserId::from(athlete))
        .await?;

    // Standing but never filled in: an empty profile, honestly rendered, so a
    // coach's screen shows blank fields rather than a spurious error.
    Ok(Json(profile.map(Into::into).unwrap_or(
        AthleteProfileResponse {
            goals: None,
            training_age_months: None,
            limitations: None,
            date_of_birth: None,
            height_cm: None,
        },
    )))
}

/// The caller's measurements, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/me/measurements",
    tag = "profiles",
    security(("bearer" = [])),
    responses((status = 200, description = "Measurements", body = Vec<MeasurementResponse>))
)]
pub async fn my_measurements(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
) -> Result<Json<Vec<MeasurementResponse>>, ApiError> {
    let rows = state.profiles.my_measurements(user_id).await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// Record (or correct) one day's numbers. PUT by date: re-entering the same
/// morning replaces it, which also makes offline re-sync naturally idempotent.
#[utoipa::path(
    put,
    path = "/api/v1/me/measurements/{date}",
    tag = "profiles",
    security(("bearer" = [])),
    params(("date" = String, Path, description = "The day, YYYY-MM-DD")),
    request_body = SaveMeasurementRequest,
    responses(
        (status = 200, description = "Saved", body = MeasurementResponse),
        (status = 400, description = "Implausible number, future date, or an empty row"),
    )
)]
pub async fn save_measurement(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
    Path(date): Path<NaiveDate>,
    Json(body): Json<SaveMeasurementRequest>,
) -> Result<Json<MeasurementResponse>, ApiError> {
    let measurement = state
        .profiles
        .save_measurement(
            user_id,
            date,
            gym_domain::measurement::MeasurementEntry {
                weight_kg: body.weight_kg,
                body_fat_percent: body.body_fat_percent,
                waist_cm: body.waist_cm,
                hip_cm: body.hip_cm,
                chest_cm: body.chest_cm,
                arm_cm: body.arm_cm,
                thigh_cm: body.thigh_cm,
                notes: body.notes,
            },
        )
        .await?;
    Ok(Json(measurement.into()))
}

/// Delete one day's entry. Allowed here and almost nowhere else: this is
/// self-reported body data, not an accountability record.
#[utoipa::path(
    delete,
    path = "/api/v1/me/measurements/{date}",
    tag = "profiles",
    security(("bearer" = [])),
    params(("date" = String, Path, description = "The day, YYYY-MM-DD")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "No entry that day"),
    )
)]
pub async fn delete_measurement(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
    Path(date): Path<NaiveDate>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.profiles.delete_measurement(user_id, date).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// An athlete's measurements, read by their coach or a gym manager. Same gate
/// as the athlete profile: weight trend and waist tape are shared with your
/// coach, not the gym floor.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/members/{user_id}/measurements",
    tag = "profiles",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("user_id" = Uuid, Path, description = "The athlete"),
    ),
    responses(
        (status = 200, description = "Measurements", body = Vec<MeasurementResponse>),
        (status = 404, description = "Not found, or not yours to see"),
    )
)]
pub async fn measurements_of(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, athlete)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<MeasurementResponse>>, ApiError> {
    let rows = state
        .profiles
        .measurements_of(&tenant, UserId::from(athlete))
        .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}
