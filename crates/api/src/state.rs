//! Shared application state handed to handlers.

use std::sync::Arc;

use gym_application::{
    assignments::AssignmentService,
    auth::AuthService,
    billing::BillingService,
    calendar::CalendarService,
    checkins::CheckInService,
    classes::ClassService,
    coaching::CoachingService,
    coaching_requests::CoachingRequestService,
    entitlements::EntitlementService,
    execution::ExecutionService,
    exercises::ExerciseService,
    goals::GoalService,
    gyms::GymService,
    ports::{AuditRepository, TokenIssuer, UserRepository},
    profiles::ProfileService,
    programs::ProgramService,
    recommendations::RecommendationService,
};
use gym_infrastructure::{DbPool, DummyGateway};

/// Cloneable handle to the application's dependencies.
///
/// Handlers depend on *ports* and services, not concrete adapters, so tests can
/// substitute fakes without a database.
#[derive(Clone)]
pub struct AppState {
    pub pool: DbPool,
    pub auth: AuthService,
    pub assignments: AssignmentService,
    pub billing: BillingService,
    pub calendar: CalendarService,
    pub entitlements: EntitlementService,
    pub execution: ExecutionService,
    pub exercises: ExerciseService,
    pub goals: GoalService,
    pub gyms: GymService,
    pub coaching: CoachingService,
    pub coaching_requests: CoachingRequestService,
    pub profiles: ProfileService,
    pub recommendations: RecommendationService,
    pub programs: ProgramService,
    pub checkins: CheckInService,
    pub classes: ClassService,
    pub audit: Arc<dyn AuditRepository>,
    pub users: Arc<dyn UserRepository>,
    pub tokens: Arc<dyn TokenIssuer>,
    /// `None` when Stripe is not configured for this deployment — the webhook
    /// route then refuses every event rather than trusting an unverifiable one.
    pub stripe_webhook_secret: Option<String>,
    /// The self-hosted card page's verifier, when that gateway is the one in
    /// use. `None` under Stripe or with payments off, and the `/pay` routes
    /// then refuse rather than rendering a form that could never settle.
    pub dummy_gateway: Option<Arc<DummyGateway>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}
