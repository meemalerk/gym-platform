//! Authentication endpoints.
//!
//! Refresh tokens are returned in the JSON body because the primary client is a
//! native mobile app storing them in the OS keychain (expo-secure-store), not a
//! browser. A browser client should use the BFG/BFF pattern with an HttpOnly
//! cookie instead — see docs/research-2026.md §5.

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
};
use gym_application::auth::{LoginCommand, LoginContext, SignUpCommand};
use gym_domain::exercise::Modality;
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, extract::Authenticated, state::AppState};

// ------------------------------------------------------------------ payloads

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SignUpRequest {
    #[schema(example = "owner@ironbox.com")]
    pub email: String,
    /// Minimum 12 characters.
    #[schema(example = "correct horse battery staple")]
    pub password: String,
    #[schema(example = "Alex Owner")]
    pub display_name: String,
    pub device_label: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub device_label: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RefreshRequest {
    pub refresh_token: String,
    pub device_label: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TokensResponse {
    pub access_token: String,
    /// Access-token lifetime in seconds.
    pub expires_in: i64,
    /// Store in the OS keychain. Rotates on every refresh.
    pub refresh_token: String,
    pub token_type: &'static str,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UserSummary {
    pub id: uuid::Uuid,
    pub email: String,
    pub display_name: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SignUpResponse {
    pub user: UserSummary,
    #[serde(flatten)]
    pub tokens: TokensResponse,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MembershipSummary {
    pub gym_id: uuid::Uuid,
    pub gym_name: String,
    /// A personal workspace rather than a commercial gym.
    pub is_personal: bool,
    /// Every capacity held in this gym — a set, not a single role (ADR-0014).
    pub capacities: Vec<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MeResponse {
    pub user: UserSummary,
    /// Still plural even under a single-gym deployment (ADR-0023) — see
    /// `UserRepository::memberships`'s doc for why. In practice never more
    /// than one entry once `SINGLE_GYM_MODE` is on; the mobile client is what
    /// commits to treating this as singular.
    pub memberships: Vec<MembershipSummary>,
}

fn tokens_response(tokens: gym_application::auth::IssuedTokens) -> TokensResponse {
    TokensResponse {
        access_token: tokens.access_token,
        expires_in: tokens.expires_in_seconds,
        refresh_token: tokens.refresh_token,
        token_type: "Bearer",
    }
}

// ------------------------------------------------------------------ handlers

/// Create an account.
///
/// Deliberately asks nothing about gyms or roles — that is the next, reversible
/// step (ADR-0014).
#[utoipa::path(
    post,
    path = "/api/v1/auth/sign-up",
    tag = "auth",
    request_body = SignUpRequest,
    responses(
        (status = 201, description = "Account created", body = SignUpResponse),
        (status = 400, description = "Invalid input"),
        (status = 409, description = "Email already registered"),
    )
)]
pub async fn sign_up(
    State(state): State<AppState>,
    Json(body): Json<SignUpRequest>,
) -> Result<(axum::http::StatusCode, Json<SignUpResponse>), ApiError> {
    let result = state
        .auth
        .sign_up(SignUpCommand {
            email: body.email,
            password: body.password,
            display_name: body.display_name,
            device_label: body.device_label,
        })
        .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(SignUpResponse {
            user: UserSummary {
                id: result.user.id.into_uuid(),
                email: result.user.email.to_string(),
                display_name: result.user.display_name,
            },
            tokens: tokens_response(result.tokens),
        }),
    ))
}

/// Exchange credentials for tokens.
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Authenticated", body = TokensResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 429, description = "Too many failed attempts; wait and try again"),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<TokensResponse>, ApiError> {
    let tokens = state
        .auth
        .login(
            LoginCommand {
                email: body.email,
                password: body.password,
                device_label: body.device_label,
            },
            &LoginContext {
                ip: client_ip(&headers, peer),
            },
        )
        .await?;

    Ok(Json(tokens_response(tokens)))
}

/// Best-effort caller address, for the throttle's secondary counter.
///
/// `x-forwarded-for` is trusted here, which is only safe because this API is
/// expected to sit behind a proxy it controls (nginx, in the demo). A client
/// can forge the header, so this is deliberately the WEAKER of the two
/// counters — the per-address one cannot be forged and is the one that
/// actually protects an account.
///
/// The peer address is the fallback and, when there is no proxy, the truth.
fn client_ip(headers: &HeaderMap, peer: std::net::SocketAddr) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        // The left-most entry is the original client; everything after it is
        // the proxy chain.
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .or_else(|| Some(peer.ip().to_string()))
}

// ------------------------------------------------- password reset & verification

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ForgotPasswordRequest {
    #[schema(example = "member@example.com")]
    pub email: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ResetPasswordRequest {
    /// The value from the emailed link.
    pub token: String,
    #[schema(example = "a new long passphrase")]
    pub password: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    #[schema(example = "a new one, twelve characters or more")]
    pub new_password: String,
}

/// Change your own password (ADR-0032).
///
/// Needed because staff accounts start on a password somebody else generated
/// and read out. The reset link would do the same job, but it goes through an
/// email this deployment does not send.
///
/// The current password is required. An access token is a bearer credential,
/// and a stolen one must not be enough to lock the real owner out. Success
/// revokes every session including this one, so the client signs back in.
#[utoipa::path(
    post,
    path = "/api/v1/auth/change-password",
    tag = "auth",
    security(("bearer" = [])),
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Changed; every session was revoked"),
        (status = 400, description = "The new password is too short"),
        (status = 401, description = "Not signed in, or the current password is wrong"),
    )
)]
pub async fn change_password(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .auth
        .change_password(user_id, &body.current_password, &body.new_password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Ask for a password-reset link.
///
/// **Always 202**, whether or not the address is registered. Answering
/// differently would turn this into a membership oracle — post an address,
/// read the status, learn whether that person trains here.
#[utoipa::path(
    post,
    path = "/api/v1/auth/forgot-password",
    tag = "auth",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 202, description = "If that address has an account, a link has been sent"),
    )
)]
pub async fn forgot_password(
    State(state): State<AppState>,
    Json(body): Json<ForgotPasswordRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.auth.request_password_reset(&body.email).await?;
    Ok(axum::http::StatusCode::ACCEPTED)
}

/// Set a new password using an emailed link.
///
/// Ends every session on that account: changing a password is what someone
/// does when they think it is compromised, and leaving an attacker's refresh
/// token alive would make the reset theatre.
#[utoipa::path(
    post,
    path = "/api/v1/auth/reset-password",
    tag = "auth",
    request_body = ResetPasswordRequest,
    responses(
        (status = 204, description = "Password changed; all sessions signed out"),
        (status = 400, description = "Link is unknown, expired or already used — or the password is too short"),
    )
)]
pub async fn reset_password(
    State(state): State<AppState>,
    Json(body): Json<ResetPasswordRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .auth
        .complete_password_reset(&body.token, &body.password)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Send (or resend) a verification link to your own address.
#[utoipa::path(
    post,
    path = "/api/v1/auth/send-verification",
    tag = "auth",
    security(("bearer" = [])),
    responses(
        (status = 202, description = "Sent, or already verified"),
        (status = 401, description = "Not authenticated"),
    )
)]
pub async fn send_verification(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
) -> Result<axum::http::StatusCode, ApiError> {
    state.auth.request_email_verification(user_id).await?;
    Ok(axum::http::StatusCode::ACCEPTED)
}

/// Confirm an address from an emailed link.
///
/// Unauthenticated on purpose: the link is opened from a mail client, which
/// has no session. The token is the credential.
#[utoipa::path(
    post,
    path = "/api/v1/auth/verify-email",
    tag = "auth",
    request_body = VerifyEmailRequest,
    responses(
        (status = 204, description = "Address confirmed"),
        (status = 400, description = "Link is unknown, expired or already used"),
    )
)]
pub async fn verify_email(
    State(state): State<AppState>,
    Json(body): Json<VerifyEmailRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.auth.verify_email(&body.token).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Rotate a refresh token. The presented token is invalidated.
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "auth",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "Rotated", body = TokensResponse),
        (status = 401, description = "Invalid, expired, or already-used token"),
    )
)]
pub async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<TokensResponse>, ApiError> {
    let tokens = state
        .auth
        .refresh(&body.refresh_token, body.device_label)
        .await?;

    Ok(Json(tokens_response(tokens)))
}

/// Revoke the presented refresh session.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    request_body = RefreshRequest,
    responses((status = 204, description = "Logged out (idempotent)"))
)]
pub async fn logout(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.auth.logout(&body.refresh_token).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// The authenticated user and the gyms they belong to.
#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "auth",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Current user", body = MeResponse),
        (status = 401, description = "Not authenticated"),
    )
)]
pub async fn me(
    State(state): State<AppState>,
    Authenticated { user_id }: Authenticated,
) -> Result<Json<MeResponse>, ApiError> {
    let user = state
        .users
        .find_by_id(user_id)
        .await?
        .ok_or(gym_application::ApplicationError::Unauthenticated)?;

    let memberships = state.auth.memberships(user_id).await?;

    Ok(Json(MeResponse {
        user: UserSummary {
            id: user.id.into_uuid(),
            email: user.email.to_string(),
            display_name: user.display_name,
        },
        memberships: memberships
            .into_iter()
            .map(|m| MembershipSummary {
                gym_id: m.gym_id.into_uuid(),
                gym_name: m.gym_name,
                is_personal: m.is_personal,
                capacities: m
                    .capabilities
                    .held()
                    .iter()
                    .map(|c| c.as_str().to_owned())
                    .collect(),
            })
            .collect(),
    }))
}

/// Re-exported so the OpenAPI schema registry sees `Modality`.
pub type ModalitySchema = Modality;
