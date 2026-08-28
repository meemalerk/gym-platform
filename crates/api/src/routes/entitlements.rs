//! Entitlement endpoints — "what may I use here, and why".
//!
//! Exposed rather than left implicit because a member who cannot start a
//! session deserves to be told which membership would let them, and a screen
//! that says "you need the Coaching plan" is only possible if the reason
//! travels with the answer. Same rule as the recommender: nothing asserts
//! access without being able to explain it.

use axum::{Json, extract::State};
use gym_domain::entitlement::{EntitlementSet, Feature, Source};
use serde::Serialize;
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EntitlementResponse {
    pub feature: Feature,
    pub source: Source,
    /// One line a screen can print verbatim.
    pub because: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MyEntitlementsResponse {
    pub gym_id: Uuid,
    pub held: Vec<EntitlementResponse>,
}

fn describe(source: &Source) -> String {
    match source {
        Source::Subscription { plan_name } => format!("Your {plan_name} membership"),
        Source::NotBilled => "This gym does not bill through the app".to_owned(),
        Source::PlatformTier => "Included for every gym".to_owned(),
    }
}

fn to_response(gym_id: Uuid, set: &EntitlementSet) -> MyEntitlementsResponse {
    MyEntitlementsResponse {
        gym_id,
        held: set
            .held()
            .iter()
            .map(|(feature, source)| EntitlementResponse {
                feature: *feature,
                source: source.clone(),
                because: describe(source),
            })
            .collect(),
    }
}

/// What the caller may use in this gym, with the reason for each.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/entitlements/me",
    tag = "billing",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses((status = 200, description = "Entitlements", body = MyEntitlementsResponse))
)]
pub async fn mine(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<MyEntitlementsResponse>, ApiError> {
    let set = state
        .entitlements
        .for_member(&tenant, tenant.actor_id)
        .await?;

    Ok(Json(to_response(tenant.gym_id.into_uuid(), &set)))
}
