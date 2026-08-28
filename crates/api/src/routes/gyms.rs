//! Gym creation — the second step of onboarding.
//!
//! Any authenticated account may create a gym (the same way anyone may create a
//! workspace); the creator becomes an owner. Authorization starts mattering once
//! other people are involved.

use axum::{Json, extract::State, http::StatusCode};
use gym_application::gyms::{CreateGymCommand, CreateStaffCommand};
use serde::{Deserialize, Serialize};

use crate::{
    error::ApiError,
    extract::{Authenticated, TenantScope},
    state::AppState,
};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateGymRequest {
    #[schema(example = "Iron Box Strength")]
    pub name: String,
    /// `true` for a solo trainer's or self-coached athlete's own workspace.
    #[serde(default)]
    pub is_personal: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct GymResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub slug: String,
    pub is_personal: bool,
    /// The capacities the creator now holds here.
    pub capacities: Vec<String>,
}

/// Create a gym or personal workspace.
#[utoipa::path(
    post,
    path = "/api/v1/gyms",
    tag = "gyms",
    security(("bearer" = [])),
    request_body = CreateGymRequest,
    responses(
        (status = 201, description = "Created; you are its owner", body = GymResponse),
        (status = 400, description = "Invalid name"),
        (status = 401, description = "Not authenticated"),
    )
)]
pub async fn create(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
    Json(body): Json<CreateGymRequest>,
) -> Result<(StatusCode, Json<GymResponse>), ApiError> {
    let gym = state
        .gyms
        .create(
            user_id,
            CreateGymCommand {
                name: body.name,
                is_personal: body.is_personal,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(GymResponse {
            id: gym.id.into_uuid(),
            name: gym.name,
            slug: gym.slug,
            is_personal: gym.is_personal,
            capacities: vec!["owner".to_owned()],
        }),
    ))
}

/// One person in a gym, for pickers and roster views.
///
/// No email address: a name and the capacities held are enough to choose someone
/// from a list, and a roster endpoint should not hand out a harvestable list of
/// contact details.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct GymMemberResponse {
    pub user_id: uuid::Uuid,
    pub display_name: String,
    pub capacities: Vec<String>,
}

/// Everyone in the gym.
///
/// Head coaches and above only — deliberately narrower than . Who
/// trains at a gym is personal information its members have not agreed to share
/// with each other, and the operational need for the full list (pairing a coach
/// with an athlete) sits at head-coach level anyway.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/members",
    tag = "gyms",
    security(("bearer" = [])),
    params(("gym_id" = uuid::Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Roster", body = Vec<GymMemberResponse>),
        (status = 403, description = "Caller may not read the roster"),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn members(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<GymMemberResponse>>, ApiError> {
    let roster = state.gyms.roster(&tenant).await?;

    Ok(Json(
        roster
            .into_iter()
            .map(|m| GymMemberResponse {
                user_id: m.user_id.into_uuid(),
                display_name: m.display_name,
                capacities: m
                    .capabilities
                    .held()
                    .iter()
                    .map(|c| c.as_str().to_owned())
                    .collect(),
            })
            .collect(),
    ))
}

// -------------------------------------------------------------- standing

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetCapacitiesRequest {
    /// The standing this person should end up with — the whole set, not a
    /// delta. Sending `["member"]` for a trainer demotes them.
    #[schema(example = json!(["member", "trainer"]))]
    pub capacities: Vec<String>,
}

/// Change what somebody holds in this gym (ADR-0031).
///
/// The replacement for invitations. Everybody joins through the open door as a
/// `member`; this is how they become anything else. Owners and admins may
/// call it, but **only an owner may grant or remove `owner`**, and the last
/// owner cannot demote themselves — otherwise a gym can be locked out of
/// itself with one request.
#[utoipa::path(
    put,
    path = "/api/v1/gyms/{gym_id}/members/{user_id}/capacities",
    tag = "gyms",
    security(("bearer" = [])),
    params(
        ("gym_id" = uuid::Uuid, Path, description = "Gym id"),
        ("user_id" = uuid::Uuid, Path, description = "The person whose standing is changing"),
    ),
    request_body = SetCapacitiesRequest,
    responses(
        (status = 200, description = "The standing they now hold", body = GymMemberResponse),
        (status = 400, description = "Unknown capacity, empty set, or the last owner"),
        (status = 403, description = "Caller may not change standing, or may not touch owner"),
        (status = 404, description = "Gym not found, or that person is not a member"),
    )
)]
pub async fn set_capacities(
    State(state): State<AppState>,
    tenant: TenantScope,
    axum::extract::Path((_gym, user_id)): axum::extract::Path<(uuid::Uuid, uuid::Uuid)>,
    Json(body): Json<SetCapacitiesRequest>,
) -> Result<Json<GymMemberResponse>, ApiError> {
    let capacities = parse_capacities(&body.capacities)?;

    let target = gym_domain::UserId::from(user_id);
    let held = state.gyms.set_capacities(&tenant, target, capacities).await?;

    let display_name = state
        .gyms
        .roster(&tenant)
        .await?
        .into_iter()
        .find(|m| m.user_id == target)
        .map(|m| m.display_name)
        .unwrap_or_default();

    Ok(Json(GymMemberResponse {
        user_id,
        display_name,
        capacities: held.iter().map(|c| c.as_str().to_owned()).collect(),
    }))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateStaffRequest {
    #[schema(example = "tariq@ironbox.example")]
    pub email: String,
    #[schema(example = "Tariq Trainer")]
    pub display_name: String,
    /// What they hold from the moment the account exists.
    #[schema(example = json!(["trainer", "member"]))]
    pub capacities: Vec<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreatedStaffResponse {
    pub user_id: uuid::Uuid,
    pub display_name: String,
    pub email: String,
    pub capacities: Vec<String>,
    /// **Shown once.** Not stored in plaintext, not retrievable, and not in the
    /// audit trail. Hand it over; they change it on first sign-in.
    pub temporary_password: String,
}

/// Create a staff account outright (ADR-0032).
///
/// The fast path for setting a gym up: instead of waiting for a trainer to
/// sign up and walk through the open door so they can be promoted, the owner
/// makes the account and the standing in one request and hands over a
/// generated password.
///
/// **An address that already has an account is a 409, not a merge.** Attaching
/// somebody's existing account to a gym because a manager typed their address
/// would be a membership granted without their consent. They join through the
/// door themselves, and are promoted from the roster.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/staff",
    tag = "gyms",
    security(("bearer" = [])),
    params(("gym_id" = uuid::Uuid, Path, description = "Gym id")),
    request_body = CreateStaffRequest,
    responses(
        (status = 201, description = "Account created, with its one-time password", body = CreatedStaffResponse),
        (status = 400, description = "Bad address, empty standing, or an unknown capacity"),
        (status = 403, description = "Caller may not create staff, or may not create an owner"),
        (status = 409, description = "That address already has an account"),
    )
)]
pub async fn create_staff(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<CreateStaffRequest>,
) -> Result<(StatusCode, Json<CreatedStaffResponse>), ApiError> {
    let capacities = parse_capacities(&body.capacities)?;

    let created = state
        .gyms
        .create_staff(
            &tenant,
            CreateStaffCommand {
                email: body.email,
                display_name: body.display_name,
                capacities,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(CreatedStaffResponse {
            user_id: created.user.id.into_uuid(),
            display_name: created.user.display_name,
            email: created.user.email.as_str().to_owned(),
            capacities: created.capacities.iter().map(|c| c.as_str().to_owned()).collect(),
            temporary_password: created.temporary_password,
        }),
    ))
}

/// Capacity strings, parsed at the edge.
///
/// Here rather than in the service so an unknown name is a 400 about the
/// request rather than a 500 about the database, and so no string the domain
/// has not vouched for reaches the service.
fn parse_capacities(raw: &[String]) -> Result<Vec<gym_domain::Capacity>, ApiError> {
    raw.iter()
        .map(|name| {
            gym_domain::Capacity::parse(name).ok_or_else(|| {
                ApiError::from(gym_application::ApplicationError::Domain(
                    gym_domain::DomainError::Invalid(format!("unknown capacity: {name}")),
                ))
            })
        })
        .collect()
}

// ------------------------------------------------------- open registration

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpenGymResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetOpenRegistrationRequest {
    pub open_registration: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct GymSettingsResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub open_registration: bool,
}

/// Gyms accepting members right now.
///
/// Authenticated, but **not** tenant-scoped: the caller holds no membership
/// yet, which is precisely the state this resolves. Safe to answer because a
/// gym appears only if its owner opted in to being findable, and the answer
/// carries nothing about who trains there.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/open",
    tag = "gyms",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Gyms accepting members", body = Vec<OpenGymResponse>),
        (status = 401, description = "Not authenticated"),
    )
)]
pub async fn open_for_registration(
    State(state): State<AppState>,
    Authenticated { user_id: _ }: Authenticated,
) -> Result<Json<Vec<OpenGymResponse>>, ApiError> {
    let gyms = state.gyms.open_for_registration().await?;

    Ok(Json(
        gyms.into_iter()
            .map(|g| OpenGymResponse {
                id: g.id.into_uuid(),
                name: g.name,
                slug: g.slug,
            })
            .collect(),
    ))
}

/// Join a gym that has opened its doors. You become a plain member.
///
/// Never grants staff standing whatever the request says — the capacity is
/// hard-coded in the repository, so this endpoint cannot be used to escalate.
/// Staff are made afterwards, from members, by somebody who already runs the
/// gym (ADR-0031) — this is the only door, and it only ever admits members.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/join",
    tag = "gyms",
    security(("bearer" = [])),
    params(("gym_id" = uuid::Uuid, Path, description = "Gym id")),
    responses(
        (status = 204, description = "Joined as a member"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "No such gym, or it is not accepting members"),
        (status = 409, description = "You already belong to this gym"),
    )
)]
pub async fn join(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
    axum::extract::Path(gym_id): axum::extract::Path<uuid::Uuid>,
) -> Result<StatusCode, ApiError> {
    state
        .gyms
        .join(gym_domain::GymId::from(gym_id), user_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Open or close the door. Owners and admins.
#[utoipa::path(
    put,
    path = "/api/v1/gyms/{gym_id}/settings/registration",
    tag = "gyms",
    security(("bearer" = [])),
    params(("gym_id" = uuid::Uuid, Path, description = "Gym id")),
    request_body = SetOpenRegistrationRequest,
    responses(
        (status = 200, description = "Updated", body = GymSettingsResponse),
        (status = 403, description = "Caller may not manage this gym"),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn set_registration(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<SetOpenRegistrationRequest>,
) -> Result<Json<GymSettingsResponse>, ApiError> {
    let gym = state
        .gyms
        .set_open_registration(&tenant, body.open_registration)
        .await?;

    Ok(Json(GymSettingsResponse {
        id: gym.id.into_uuid(),
        name: gym.name,
        open_registration: gym.open_registration,
    }))
}

/// The gym's settings, for the screen that changes them.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/settings",
    tag = "gyms",
    security(("bearer" = [])),
    params(("gym_id" = uuid::Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Settings", body = GymSettingsResponse),
        (status = 403, description = "Caller may not manage this gym"),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn settings(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<GymSettingsResponse>, ApiError> {
    // Read-gated the same way the write is: "may strangers walk in" is not
    // something every member needs to know, and keeping both at the same level
    // means one rule instead of two.
    if !tenant.capabilities.can_manage_gym() {
        return Err(gym_application::ApplicationError::Forbidden.into());
    }

    let gym = state.gyms.find_for_settings(&tenant).await?;

    Ok(Json(GymSettingsResponse {
        id: gym.id.into_uuid(),
        name: gym.name,
        open_registration: gym.open_registration,
    }))
}
