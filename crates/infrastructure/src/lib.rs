//! Adapters implementing the application's ports: Postgres repositories,
//! password hashing, token issuing, and database setup.

pub mod assignments;
pub mod audit;
pub mod auth_hardening;
pub mod billing;
pub mod billing_cycle;
pub mod calendar;
pub mod checkin_pass;
pub mod checkins;
pub mod classes;
pub mod coaching;
pub mod coaching_requests;
pub mod db;
pub mod dummy_gateway;
pub mod execution;
pub mod goals;
pub mod outbox;
pub mod password;
pub mod profiles;
pub mod programs;
pub mod repositories;
pub mod stripe;
pub mod tokens;

pub use audit::PgAuditRepository;
pub use auth_hardening::{PgAuthTokenRepository, PgLoginAttemptRepository, RecordingEmailSender};
pub use billing::PgBillingRepository;
pub use calendar::PgCalendarRepository;
pub use checkin_pass::JwtEntryPassIssuer;
pub use checkins::PgCheckInRepository;
pub use classes::PgClassRepository;
pub use coaching_requests::{PgCoachingRequestRepository, PgTrainerDirectoryRepository};
pub use db::{DbPool, connect, run_migrations};
pub use dummy_gateway::{CardOutcome, DummyGateway, evaluate_card};
pub use password::Argon2PasswordHasher;
pub use repositories::{
    PgExerciseRepository, PgGymRepository, PgSessionRepository, PgUserRepository,
};
pub use stripe::{NotConfiguredGateway, StripeGateway, verify_webhook_signature};
pub use tokens::JwtTokenIssuer;
