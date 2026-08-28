//! Request extractors for authentication and tenant scoping.
//!
//! Two deliberate properties:
//!  - the access token carries **only** a subject; roles are never read from it
//!  - `TenantScope` re-checks membership **from the database on every request**,
//!    so a revoked membership takes effect immediately rather than when the
//!    token expires.

use axum::{
    extract::{FromRequestParts, Path},
    http::{header::AUTHORIZATION, request::Parts},
};
use gym_application::ApplicationError;
use gym_domain::{GymId, TenantContext, UserId};
use serde::Deserialize;

use crate::{error::ApiError, state::AppState};

/// An authenticated caller. Proves the bearer token was valid; says nothing
/// about what they may do.
#[derive(Debug, Clone, Copy)]
pub struct Authenticated {
    pub user_id: UserId,
}

impl FromRequestParts<AppState> for Authenticated {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(ApplicationError::Unauthenticated)?;

        // Scheme comparison is case-insensitive per RFC 7235.
        let token = header
            .split_once(' ')
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
            .map(|(_, token)| token.trim())
            .filter(|t| !t.is_empty())
            .ok_or(ApplicationError::Unauthenticated)?;

        let user_id = state.tokens.verify_access(token)?;
        Ok(Self { user_id })
    }
}

#[derive(Debug, Deserialize)]
struct GymPath {
    gym_id: uuid::Uuid,
}

/// An authenticated caller acting **inside a specific gym**, with their role
/// resolved from the database.
///
/// Deref-style access to the inner `TenantContext` is intentional: handlers pass
/// it straight to repositories, which require it (ADR-0004).
#[derive(Debug, Clone)]
pub struct TenantScope(pub TenantContext);

impl FromRequestParts<AppState> for TenantScope {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Authenticated { user_id } = Authenticated::from_request_parts(parts, state).await?;

        let Path(GymPath { gym_id }) = Path::<GymPath>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApplicationError::NotFound { entity: "gym" })?;

        let gym_id = GymId::from(gym_id);

        // Resolved live from the database, never from token claims, so a revoked
        // capacity takes effect on the very next request.
        let capabilities = state.users.capabilities_in(user_id, gym_id).await?;

        // Holding no capacity → 404, not 403. Revealing "this gym exists but you
        // cannot see it" leaks tenant existence to outsiders.
        if capabilities.is_empty() {
            return Err(ApplicationError::NotFound { entity: "gym" }.into());
        }

        Ok(Self(TenantContext::new(gym_id, user_id, capabilities)))
    }
}

impl std::ops::Deref for TenantScope {
    type Target = TenantContext;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
