//! HTTP layer. Deliberately thin: translate HTTP ↔ use-cases and nothing more.
//! Cross-cutting concerns are Tower layers, not bespoke middleware.

pub mod error;
pub mod extract;
pub mod routes;
pub mod state;

use std::time::Duration;

use axum::{
    Router,
    http::{HeaderValue, Method, StatusCode, header},
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::CompressionLayer,
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    sensitive_headers::SetSensitiveRequestHeadersLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_axum::{router::OpenApiRouter, routes};

pub use state::AppState;

/// Maximum accepted request body. Generous for JSON, far below anything that
/// would let a client exhaust memory.
const MAX_BODY_BYTES: usize = 1024 * 1024; // 1 MiB

/// Registers the bearer scheme so Swagger UI offers an "Authorize" button.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Gym Platform API",
        version = "0.1.0",
        description = "Multi-tenant coaching and gym-management platform.",
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "health", description = "Liveness and readiness"),
        (name = "auth", description = "Sign-up, login, token rotation"),
        (name = "gyms", description = "Gym and personal-workspace creation"),
        (name = "exercises", description = "Gym-scoped exercise catalogue"),
        (name = "programs", description = "Programme authoring and the version lifecycle"),
        (name = "coaching", description = "Who coaches whom, and what that permits"),
        (name = "assignments", description = "Putting athletes on published programme versions"),
        (name = "execution", description = "Workout sessions and performed sets — training history"),
        (name = "profiles", description = "Person-owned profiles that follow the account between gyms"),
        (name = "goals", description = "Measurable targets — progress is computed, never stored"),
        (name = "recommendations", description = "Deterministic suggestions with reasons attached"),
        (name = "audit", description = "Who did what, in one gym"),
        (name = "calendar", description = "Opening hours, closures, and trainer availability"),
    )
)]
pub struct ApiDoc;

/// Build a CORS layer from an explicit origin allowlist.
///
/// Native clients are not subject to CORS, so this exists for browser clients
/// (the future instructor web app, and the web dev target). Deliberately:
///  - **exact origins only**, never `Any` — a wildcard plus credentials is the
///    classic CORS foot-gun (docs/research-2026.md §5);
///  - **no `allow_credentials`** — we authenticate with a Bearer header, not
///    cookies, so the browser never needs to send credentials cross-origin;
///  - an **empty allowlist yields no cross-origin permission at all**, which is
///    the safe default for production.
fn cors_layer(allowed_origins: &[String]) -> CorsLayer {
    let origins: Vec<HeaderValue> = allowed_origins
        .iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect();

    if origins.is_empty() {
        return CorsLayer::new();
    }

    CorsLayer::new()
        .allow_origin(origins)
        // Every method the API actually serves. PUT is not decoration: profiles
        // and body measurements are idempotent upserts on a known key, and
        // omitting it here fails only in a browser — a native client is not
        // subject to CORS at all, so the gap stays invisible until the first
        // web surface tries to save a profile.
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
        .max_age(Duration::from_secs(3600))
}

/// Build the application router plus the generated OpenAPI document.
pub fn build_router(
    state: AppState,
    allowed_origins: &[String],
) -> (Router, utoipa::openapi::OpenApi) {
    let api_v1 = OpenApiRouter::new()
        .routes(routes!(routes::auth::sign_up))
        .routes(routes!(routes::auth::login))
        .routes(routes!(routes::auth::refresh))
        .routes(routes!(routes::auth::logout))
        .routes(routes!(routes::auth::forgot_password))
        .routes(routes!(routes::auth::reset_password))
        .routes(routes!(routes::auth::change_password))
        .routes(routes!(routes::auth::send_verification))
        .routes(routes!(routes::auth::verify_email))
        .routes(routes!(routes::auth::me))
        .routes(routes!(routes::gyms::create))
        // Must be registered BEFORE any `/gyms/{gym_id}` route: axum matches
        // literal segments ahead of captures, but keeping them adjacent makes
        // the intent legible rather than relying on that.
        .routes(routes!(routes::gyms::open_for_registration))
        .routes(routes!(routes::gyms::join))
        .routes(routes!(routes::gyms::settings))
        .routes(routes!(routes::gyms::set_registration))
        // `list` and `create` share a path, so they register as one route entry.
        .routes(routes!(routes::exercises::list, routes::exercises::create))
        .routes(routes!(routes::exercises::pending))
        .routes(routes!(routes::exercises::curate))
        .routes(routes!(routes::audit::list))
        .routes(routes!(routes::programs::list, routes::programs::create))
        .routes(routes!(
            routes::programs::versions,
            routes::programs::new_draft
        ))
        .routes(routes!(routes::programs::content))
        .routes(routes!(routes::programs::add_week))
        .routes(routes!(routes::programs::add_workout))
        .routes(routes!(routes::programs::prescribe))
        .routes(routes!(routes::programs::transition))
        .routes(routes!(routes::coaching::list))
        .routes(routes!(routes::coaching::end))
        .routes(routes!(routes::coaching_requests::directory))
        .routes(routes!(
            routes::coaching_requests::list,
            routes::coaching_requests::raise
        ))
        .routes(routes!(routes::coaching_requests::propose))
        .routes(routes!(routes::coaching_requests::answer))
        .routes(routes!(routes::coaching_requests::withdraw))
        .routes(routes!(routes::gyms::members))
        .routes(routes!(routes::gyms::set_capacities))
        .routes(routes!(routes::gyms::create_staff))
        .routes(routes!(
            routes::assignments::list,
            routes::assignments::assign
        ))
        .routes(routes!(routes::assignments::withdraw))
        .routes(routes!(routes::execution::list, routes::execution::start))
        .routes(routes!(routes::execution::detail))
        .routes(routes!(routes::execution::log_set))
        .routes(routes!(routes::execution::finish))
        .routes(routes!(routes::execution::exercise_history))
        .routes(routes!(routes::profiles::my_profiles))
        .routes(routes!(routes::profiles::update_athlete))
        .routes(routes!(routes::profiles::update_trainer))
        .routes(routes!(routes::profiles::rename))
        .routes(routes!(routes::profiles::athlete_profile_of))
        .routes(routes!(routes::profiles::my_measurements))
        .routes(routes!(
            routes::profiles::save_measurement,
            routes::profiles::delete_measurement
        ))
        .routes(routes!(routes::profiles::measurements_of))
        .routes(routes!(routes::goals::list, routes::goals::create))
        .routes(routes!(routes::goals::close))
        .routes(routes!(routes::recommendations::for_me))
        .routes(routes!(
            routes::billing::list_plans,
            routes::billing::create_plan
        ))
        .routes(routes!(routes::billing::archive_plan))
        .routes(routes!(
            routes::billing::list_subscriptions,
            routes::billing::subscribe
        ))
        .routes(routes!(routes::billing::cancel_subscription))
        .routes(routes!(
            routes::billing::list_invoices,
            routes::billing::issue_invoice
        ))
        .routes(routes!(routes::billing::void_invoice))
        .routes(routes!(
            routes::billing::list_payments,
            routes::billing::record_payment
        ))
        .routes(routes!(routes::billing::create_checkout_session))
        .routes(routes!(routes::webhooks::stripe_webhook))
        .routes(routes!(routes::entitlements::mine))
        .routes(routes!(routes::calendar::calendar))
        .routes(routes!(
            routes::calendar::hours,
            routes::calendar::set_hours
        ))
        .routes(routes!(routes::calendar::set_override))
        .routes(routes!(routes::calendar::clear_override))
        .routes(routes!(
            routes::calendar::availability,
            routes::calendar::set_availability
        ))
        .routes(routes!(routes::calendar::bookable))
        .routes(routes!(routes::checkins::my_pass))
        .routes(routes!(routes::checkins::scan))
        .routes(routes!(routes::checkins::recent))
        .routes(routes!(
            routes::classes::timetable,
            routes::classes::create
        ))
        .routes(routes!(routes::classes::archive))
        .routes(routes!(routes::classes::book))
        .routes(routes!(routes::classes::cancel_booking))
        .routes(routes!(routes::classes::roster));

    // The card page is deliberately OUTSIDE the OpenAPI router: it serves HTML
    // to a browser following a redirect, not JSON to a client, and publishing
    // it would put a route in the generated SDK that nothing should ever call.
    let pay_pages = Router::new()
        .route(
            "/pay/{token}",
            axum::routing::get(routes::pay::page).post(routes::pay::submit),
        )
        .with_state(state.clone());

    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(routes::health::health))
        .routes(routes!(routes::health::readiness))
        .merge(api_v1)
        .with_state(state)
        .split_for_parts();

    let router = router
        .merge(pay_pages)
        // Outermost: never let a panic kill the connection silently.
        .layer(CatchPanicLayer::new())
        .layer(TraceLayer::new_for_http())
        // CORS must sit outside routing so it can answer preflight OPTIONS —
        // otherwise the router replies 405 and the real request never happens.
        .layer(cors_layer(allowed_origins))
        // Redact credentials from traces/logs.
        .layer(SetSensitiveRequestHeadersLayer::new([
            axum::http::header::AUTHORIZATION,
            axum::http::header::COOKIE,
        ]))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        // 504 (not the default 408): the timeout is our upstream work exceeding
        // budget, not the client being slow to send its request.
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(CompressionLayer::new());

    (router, api)
}
