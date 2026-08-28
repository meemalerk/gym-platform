//! Server entrypoint: load config, wire adapters to ports, serve.

mod config;

use std::sync::Arc;

use anyhow::{Context, Result};
use gym_api::{AppState, build_router};
use gym_application::{
    assignments::AssignmentService, auth::AuthService, billing::BillingService,
    calendar::CalendarService, checkins::CheckInService, classes::ClassService, coaching::CoachingService,
    coaching_requests::CoachingRequestService, entitlements::EntitlementService,
    execution::ExecutionService, exercises::ExerciseService, goals::GoalService, gyms::GymService,
    ports::SystemClock, profiles::ProfileService,
    programs::ProgramService, recommendations::RecommendationService,
};
use gym_infrastructure::{
    Argon2PasswordHasher, DummyGateway, JwtEntryPassIssuer, JwtTokenIssuer, NotConfiguredGateway,
    PgAuditRepository, PgAuthTokenRepository, PgBillingRepository, PgCalendarRepository,
    PgCheckInRepository, PgClassRepository, PgLoginAttemptRepository, RecordingEmailSender,
    StripeGateway,
    assignments::PgAssignmentRepository,
    coaching::PgCoachRepository,
    coaching_requests::{PgCoachingRequestRepository, PgTrainerDirectoryRepository},
    connect,
    execution::PgExecutionRepository,
    goals::PgGoalRepository,
    profiles::PgProfileRepository,
    programs::PgProgramRepository,
    repositories::{PgExerciseRepository, PgGymRepository, PgSessionRepository, PgUserRepository},
    run_migrations,
};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use utoipa_swagger_ui::SwaggerUi;

use crate::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // Local dev convenience; absent in production, which uses real env vars.
    let _ = dotenvy::dotenv();

    init_tracing();

    let config = Config::from_env()?;
    tracing::info!(
        addr = %config.bind_address(),
        "starting gym platform api"
    );

    // Migrations run with the privileged role, then that pool is dropped.
    {
        let migration_pool = connect(&config.database_url, 2)
            .await
            .context("failed to connect for migrations")?;

        run_migrations(&migration_pool)
            .await
            .context("failed to apply database migrations")?;
        migration_pool.close().await;
        tracing::info!("migrations up to date");
    }

    // Runtime uses the unprivileged role so RLS policies actually apply.
    let pool = connect(&config.app_database_url, config.database_max_connections)
        .await
        .context("failed to connect to the database")?;

    match sqlx::query_scalar::<_, bool>("SELECT current_user = 'gym_app'")
        .fetch_one(&pool)
        .await
    {
        Ok(true) => tracing::info!("runtime role is gym_app — row-level security is enforced"),
        Ok(false) => tracing::warn!(
            "runtime role is NOT gym_app — row-level security may be bypassed (see \
             APP_DATABASE_URL)"
        ),
        Err(e) => tracing::warn!(error = %e, "could not determine the runtime database role"),
    }

    // Wire adapters to ports. Everything downstream depends on the traits.
    let users = Arc::new(PgUserRepository::new(pool.clone()));
    let tokens = Arc::new(JwtTokenIssuer::new(
        config.jwt_secret.as_bytes(),
        config.access_token_ttl_seconds,
        config.refresh_token_ttl_days,
    ));

    let gyms = Arc::new(PgGymRepository::new(pool.clone(), config.single_gym_mode));
    let exercise_repo = Arc::new(PgExerciseRepository::new(pool.clone()));
    let program_repo = Arc::new(PgProgramRepository::new(pool.clone()));
    let coach_repo = Arc::new(PgCoachRepository::new(pool.clone()));
    let coaching_request_repo = Arc::new(PgCoachingRequestRepository::new(pool.clone()));
    let trainer_directory_repo = Arc::new(PgTrainerDirectoryRepository::new(pool.clone()));
    let assignment_repo = Arc::new(PgAssignmentRepository::new(pool.clone()));
    let goal_repo = Arc::new(PgGoalRepository::new(pool.clone()));
    let profile_repo = Arc::new(PgProfileRepository::new(pool.clone()));
    let billing_repo = Arc::new(PgBillingRepository::new(pool.clone()));
    let checkin_repo = Arc::new(PgCheckInRepository::new(pool.clone()));
    let class_repo = Arc::new(PgClassRepository::new(pool.clone()));
    // Same secret as access tokens, deliberately a different token SHAPE — see
    // crates/infrastructure/src/checkin_pass.rs's module doc.
    let entry_pass_issuer = Arc::new(JwtEntryPassIssuer::new(config.jwt_secret.as_bytes()));

    // ADR-0010's seam, now with two implementations behind it (ADR-0028).
    //
    // `NotConfiguredGateway` keeps `BillingService` fully usable when neither
    // is set up — "pay by card" simply explains itself as unavailable rather
    // than anything crashing.
    //
    // `dummy_gateway` is kept separately as well as behind the port, because
    // the card page needs to VERIFY tokens and the port only mints them. It is
    // `None` under Stripe, which is what makes `/pay/...` refuse rather than
    // render a form that could never settle.
    let dummy_gateway = (config.payment_gateway == "dummy").then(|| {
        Arc::new(DummyGateway::new(
            config.jwt_secret.as_bytes(),
            config.public_base_url.clone(),
        ))
    });

    let payment_gateway: Arc<dyn gym_application::ports::PaymentGateway> = match (
        config.payment_gateway.as_str(),
        &config.stripe_secret_key,
    ) {
        ("stripe", Some(key)) => {
            tracing::info!("stripe payment gateway configured");
            Arc::new(StripeGateway::new(key.clone()))
        }
        ("stripe", None) => {
            tracing::error!(
                "PAYMENT_GATEWAY=stripe but STRIPE_SECRET_KEY is not set —                      card payment is unavailable"
            );
            Arc::new(NotConfiguredGateway)
        }
        ("dummy", _) => {
            tracing::warn!(
                base_url = %config.public_base_url,
                "using the DEMO card gateway — no money will move"
            );
            Arc::clone(dummy_gateway.as_ref().expect("just constructed")) as Arc<_>
        }
        (other, _) => {
            if other != "none" {
                tracing::error!(gateway = other, "unknown PAYMENT_GATEWAY value");
            }
            tracing::warn!("no payment gateway configured — card payment is unavailable");
            Arc::new(NotConfiguredGateway)
        }
    };

    // One instance, shared: every feature gate must ask the same resolver.
    let entitlements = EntitlementService {
        billing: billing_repo.clone(),
    };

    let hasher: Arc<dyn gym_application::ports::PasswordHasher> =
        Arc::new(Argon2PasswordHasher);

    let state = AppState {
        billing: BillingService {
            billing: billing_repo,
            users: users.clone(),
            clock: Arc::new(SystemClock),
            gateway: payment_gateway,
        },
        entitlements: entitlements.clone(),
        calendar: CalendarService {
            calendar: Arc::new(PgCalendarRepository::new(pool.clone())),
            // Needed to refuse availability for somebody who does not coach
            // here — otherwise rows accumulate for people nobody can book.
            users: users.clone(),
        },
        auth: AuthService {
            users: users.clone(),
            gyms: gyms.clone(),
            sessions: Arc::new(PgSessionRepository::new(pool.clone())),
            hasher: hasher.clone(),
            tokens: tokens.clone(),
            clock: Arc::new(SystemClock),
            auth_tokens: Arc::new(PgAuthTokenRepository::new(pool.clone())),
            attempts: Arc::new(PgLoginAttemptRepository::new(pool.clone())),
            // No SMTP adapter exists (ADR-0029). This records what would have
            // been sent, which is what makes the reset flow demonstrable and
            // testable without a provider account.
            email: Arc::new(RecordingEmailSender::new(pool.clone())),
            // Reset and verification links point at the app, not the API — a
            // person clicking one wants a screen, not JSON.
            link_base_url: config.link_base_url.clone(),
        },
        gyms: GymService {
            gyms: gyms.clone(),
            users: users.clone(),
            // Both only for staff accounts (ADR-0032): the generated starting
            // password comes from the same CSPRNG that mints refresh tokens,
            // and is stored the same way every other password is.
            hasher: hasher.clone(),
            tokens: tokens.clone(),
        },
        exercises: ExerciseService {
            exercises: exercise_repo.clone(),
        },
        recommendations: RecommendationService {
            // Reads only; composes what the other services own. Roster access
            // here exposes nothing beyond coach names and their COACHING
            // profiles — the data trainer_profiles exists to show.
            goals: goal_repo.clone(),
            programs: program_repo.clone(),
            assignments: assignment_repo.clone(),
            relationships: coach_repo.clone(),
            profiles: profile_repo.clone(),
            users: users.clone(),
        },
        goals: GoalService {
            goals: goal_repo,
            // A lift goal names an exercise; the tenant-scoped lookup checks it.
            exercises: exercise_repo.clone(),
            relationships: coach_repo.clone(),
            users: users.clone(),
            clock: Arc::new(SystemClock),
        },
        coaching: CoachingService {
            relationships: coach_repo.clone(),
            // Needed to read both parties' capacities *in this gym* before pairing.
            users: users.clone(),
            clock: Arc::new(SystemClock),
        },
        coaching_requests: CoachingRequestService {
            requests: coaching_request_repo.clone(),
            // Accepting creates the pairing, so this service needs the same
            // relationship access the pairing service has — and re-checks both
            // parties' capacities rather than trusting the request row.
            relationships: coach_repo.clone(),
            directory: trainer_directory_repo.clone(),
            users: users.clone(),
            clock: Arc::new(SystemClock),
        },
        execution: ExecutionService {
            sessions: Arc::new(PgExecutionRepository::new(pool.clone())),
            // Sessions must belong to the athlete's own active assignment, and
            // the workout must belong to the assignment's pinned version.
            assignments: assignment_repo.clone(),
            programs: program_repo.clone(),
            relationships: coach_repo.clone(),
            // Starting a session is the first surface a membership gates.
            // (checkins below needs its own copy too, so this is a clone, not a move.)
            entitlements: entitlements.clone(),
            clock: Arc::new(SystemClock),
        },
        assignments: AssignmentService {
            assignments: assignment_repo,
            // Shares the programme adapter to load the version being assigned,
            // and the coach adapter because the RELATIONSHIP is the authority: a
            // trainer may assign only to their own clients.
            programs: program_repo.clone(),
            relationships: coach_repo.clone(),
            users: users.clone(),
            clock: Arc::new(SystemClock),
        },
        profiles: ProfileService {
            profiles: profile_repo,
            users: users.clone(),
            // The coaching gate: a coach may read their client's athlete profile.
            relationships: coach_repo.clone(),
            clock: Arc::new(SystemClock),
        },
        programs: ProgramService {
            programs: program_repo,
            // Shares the catalogue adapter: prescribing an exercise has to look
            // it up to check the prescription suits how it is measured.
            exercises: exercise_repo,
            // Both are for the approval policy: whether the gym is personal,
            // and whether anybody *else* here could sign a version off. A gym
            // with no second reviewer allows self-approval, or its lifecycle
            // stops dead at "in review".
            gyms: gyms.clone(),
            users: users.clone(),
            clock: Arc::new(SystemClock),
        },
        checkins: CheckInService {
            checkins: checkin_repo,
            entitlements: entitlements.clone(),
            passes: entry_pass_issuer,
            clock: Arc::new(SystemClock),
        },
        classes: ClassService {
            classes: class_repo,
            entitlements: entitlements.clone(),
            clock: Arc::new(SystemClock),
        },
        audit: Arc::new(PgAuditRepository::new(pool.clone())),
        users,
        tokens,
        pool,
        stripe_webhook_secret: config.stripe_webhook_secret.clone(),
        dummy_gateway,
    };

    if !config.cors_allowed_origins.is_empty() {
        tracing::info!(origins = ?config.cors_allowed_origins, "CORS enabled for origins");
    }

    let (router, api) = build_router(state, &config.cors_allowed_origins);
    let app = router.merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", api));

    let listener = tokio::net::TcpListener::bind(config.bind_address())
        .await
        .with_context(|| format!("failed to bind {}", config.bind_address()))?;

    tracing::info!("listening on http://{}", config.bind_address());

    // `into_make_service_with_connect_info` rather than the bare service: the
    // login throttle wants the caller's address, and `ConnectInfo` is the only
    // way a handler can see the peer socket. Without this the extractor simply
    // never resolves and the IP counter is silently always zero — which looks
    // exactly like "nobody is attacking us".
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server error")?;

    tracing::info!("shutdown complete");
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();
}

/// Drain in-flight requests on SIGINT/SIGTERM rather than dropping connections.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("received Ctrl+C, shutting down"),
        () = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
