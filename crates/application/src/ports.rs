//! Ports: traits the application needs, implemented by `gym-infrastructure`.
//!
//! **Every tenant-owned method takes `TenantContext`** — never a bare id.
//! That signature is the cheapest cross-tenant-leak prevention we have (ADR-0004).

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use gym_domain::{
    AssignmentId, Capabilities, ClassBookingId, CoachRelationshipId, CoachingRequestId, ExerciseId,
    GoalId, GymClassId, GymId, InvoiceId, MembershipPlanId, ProgramId, ProgramVersionId,
    ProgramWeekId, SessionId, SubscriptionId, TenantContext, UserId, WorkoutSessionId,
    WorkoutTemplateId,
    assignment::ProgramAssignment,
    billing::{Invoice, MemberSubscription, MembershipPlan, Payment},
    calendar::{CalendarOverride, WeeklyHours},
    checkin::CheckIn,
    coaching::CoachRelationship,
    coaching_request::CoachingRequest,
    execution::{PerformedSet, WorkoutSession},
    exercise::Exercise,
    goal::Goal,
    gym::Gym,
    gym_class::{ClassBooking, GymClass},
    measurement::BodyMeasurement,
    profile::{AthleteProfile, TrainerProfile},
    program::{Program, ProgramVersion},
    user::{Email, User},
    workout::{ProgramWeek, TemplateExercise, WorkoutTemplate},
};

use crate::ApplicationResult;

/// Tenant-scoped access to the exercise catalogue.
#[async_trait]
pub trait ExerciseRepository: Send + Sync {
    async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<Exercise>>;

    async fn find(
        &self,
        tenant: &TenantContext,
        id: ExerciseId,
    ) -> ApplicationResult<Option<Exercise>>;

    async fn insert(&self, tenant: &TenantContext, exercise: &Exercise) -> ApplicationResult<()>;

    /// Persist a curation decision (ADR-0024). Status only — renaming a
    /// movement athletes already have history against is a separate, riskier
    /// operation and must not ride along on a curation call.
    async fn save_status(
        &self,
        tenant: &TenantContext,
        exercise: &Exercise,
        action: &'static str,
    ) -> ApplicationResult<()>;
}

/// A user's standing in one gym: the gym plus every capacity they hold there.
///
/// A set, not a single role — see ADR-0014. Carries the gym's name so a client
/// can render a switcher without a second round-trip per gym.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    pub gym_id: GymId,
    pub gym_name: String,
    pub is_personal: bool,
    pub capabilities: Capabilities,
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_email(&self, email: &Email) -> ApplicationResult<Option<StoredUser>>;

    async fn find_by_id(&self, id: UserId) -> ApplicationResult<Option<User>>;

    async fn insert(&self, user: &User, password_hash: &str) -> ApplicationResult<()>;

    /// Replace a password. Used only by the reset flow — there is deliberately
    /// no "change my password while signed in" path through here yet, and when
    /// there is it will want the CURRENT password as well.
    async fn set_password_hash(&self, user: UserId, password_hash: &str) -> ApplicationResult<()>;

    /// Record that an address was confirmed. Idempotent: confirming twice
    /// keeps the first date, because that is when it actually happened.
    async fn mark_email_verified(
        &self,
        user: UserId,
        at: chrono::DateTime<chrono::Utc>,
    ) -> ApplicationResult<()>;

    /// Every gym this user belongs to, with the capacities held in each.
    ///
    /// Still plural at this layer even under a single-gym deployment
    /// (ADR-0023): `SINGLE_GYM_MODE` only caps gym *creation*, and the
    /// verification suites deliberately hold several memberships to prove
    /// tenant isolation. The API and mobile client are what commit to "one,"
    /// each narrowing this at their own boundary.
    async fn memberships(&self, user: UserId) -> ApplicationResult<Vec<Membership>>;

    /// The capacities this user holds in one gym. Empty means "not a member",
    /// which callers must treat as no access.
    async fn capabilities_in(&self, user: UserId, gym: GymId) -> ApplicationResult<Capabilities>;

    /// Everyone who holds any capacity in this gym.
    ///
    /// A membership list is personal information about every person in it, so
    /// callers must gate it — see `GymService::roster`.
    async fn roster(&self, tenant: &TenantContext) -> ApplicationResult<Vec<GymMember>>;

    /// Change the account's display name. Validation happens in the service;
    /// this only persists.
    async fn rename(&self, user: UserId, display_name: &str) -> ApplicationResult<()>;

    /// Replace what one person holds in this gym, in one transaction
    /// (ADR-0031).
    ///
    /// Replace, not add: the caller sends the standing they want the person to
    /// end up with, so "make them a trainer as well" and "they are only a
    /// member now" are the same call with different arguments. A grant-and-
    /// revoke pair would leave a window where somebody held both or neither.
    ///
    /// Revocation is a `revoked_at` stamp rather than a delete, so the audit
    /// trail can still answer "who could do what, when".
    async fn set_capacities(
        &self,
        tenant: &TenantContext,
        target: UserId,
        capacities: &[gym_domain::Capacity],
    ) -> ApplicationResult<()>;

    /// Create an account **and** give it standing in this gym, in one
    /// transaction (ADR-0032).
    ///
    /// Two writes that must not come apart: an account with no standing is a
    /// person who cannot sign in anywhere useful and who nobody can find on a
    /// roster to fix, and a standing with no account is impossible. The
    /// transaction is the only reason this is a repository method rather than
    /// two calls in the service.
    async fn insert_staff(
        &self,
        tenant: &TenantContext,
        user: &User,
        password_hash: &str,
        capacities: &[gym_domain::Capacity],
    ) -> ApplicationResult<()>;

    /// How many people hold `owner` here, not counting `excluding`.
    ///
    /// Exists for exactly one rule — a gym must keep an owner — and is a
    /// count rather than a list because that rule is the only caller and a
    /// list of owners is not something anything else should be handed.
    async fn owners_other_than(
        &self,
        tenant: &TenantContext,
        excluding: UserId,
    ) -> ApplicationResult<usize>;
}

/// Person-owned profiles (ADR-0014). Not tenant-scoped — no `TenantContext` —
/// because a profile follows the account between gyms. Access control is the
/// SERVICE's job; these tables are deliberately outside RLS (migration 0003
/// documents why), so nothing below the service is checking.
#[async_trait]
pub trait ProfileRepository: Send + Sync {
    async fn athlete_profile(&self, user: UserId) -> ApplicationResult<Option<AthleteProfile>>;

    async fn trainer_profile(&self, user: UserId) -> ApplicationResult<Option<TrainerProfile>>;

    async fn upsert_athlete(&self, profile: &AthleteProfile) -> ApplicationResult<()>;

    async fn upsert_trainer(&self, profile: &TrainerProfile) -> ApplicationResult<()>;

    /// Measurements, newest first.
    async fn measurements(&self, user: UserId) -> ApplicationResult<Vec<BodyMeasurement>>;

    /// One row per (person, day); a re-entry for the same day replaces.
    async fn upsert_measurement(&self, measurement: &BodyMeasurement) -> ApplicationResult<()>;

    /// Returns whether a row existed. Deletion is allowed here and almost
    /// nowhere else: self-reported body data, not an accountability record.
    async fn delete_measurement(
        &self,
        user: UserId,
        measured_on: chrono::NaiveDate,
    ) -> ApplicationResult<bool>;
}

/// A goal with its athlete's name resolved for lists.
#[derive(Debug, Clone)]
pub struct GoalView {
    pub goal: Goal,
    pub athlete_name: String,
}

/// A plan with how many people are currently on it.
#[derive(Debug, Clone)]
pub struct PlanView {
    pub plan: MembershipPlan,
    pub active_subscribers: i64,
}

/// A subscription with the names a screen needs beside it.
#[derive(Debug, Clone)]
pub struct SubscriptionView {
    pub subscription: MemberSubscription,
    pub member_name: String,
    pub plan_name: String,
}

/// An invoice with its member's name and what has been paid against it.
#[derive(Debug, Clone)]
pub struct InvoiceView {
    pub invoice: Invoice,
    pub member_name: String,
    /// Sum of payments, refunds included — so an invoice that was paid and
    /// then refunded does not read as settled.
    pub paid_minor: i64,
}

/// Tenant-scoped access to billing.
#[async_trait]
pub trait BillingRepository: Send + Sync {
    async fn list_plans(&self, tenant: &TenantContext) -> ApplicationResult<Vec<PlanView>>;

    async fn find_plan(
        &self,
        tenant: &TenantContext,
        id: MembershipPlanId,
    ) -> ApplicationResult<Option<MembershipPlan>>;

    async fn insert_plan(
        &self,
        tenant: &TenantContext,
        plan: &MembershipPlan,
    ) -> ApplicationResult<()>;

    /// Archive a plan. Compare-and-swap on still-offered.
    async fn archive_plan(
        &self,
        tenant: &TenantContext,
        id: MembershipPlanId,
    ) -> ApplicationResult<bool>;

    async fn list_subscriptions(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<SubscriptionView>>;

    async fn find_subscription(
        &self,
        tenant: &TenantContext,
        id: SubscriptionId,
    ) -> ApplicationResult<Option<MemberSubscription>>;

    /// Persist a cancellation. Never deletes — a subscription that existed is
    /// part of the gym's financial history, and the invoices raised against it
    /// still reference it.
    async fn save_cancelled_subscription(
        &self,
        tenant: &TenantContext,
        subscription: &MemberSubscription,
    ) -> ApplicationResult<()>;

    async fn insert_subscription(
        &self,
        tenant: &TenantContext,
        subscription: &MemberSubscription,
    ) -> ApplicationResult<()>;

    /// Take the next invoice number for this gym and year, atomically.
    ///
    /// The read and the increment are one statement, so two managers issuing
    /// at the same moment cannot be handed the same number. A number taken by
    /// a transaction that then fails is simply spent — a gap an accountant can
    /// explain, rather than a duplicate nobody can.
    async fn allocate_invoice_number(
        &self,
        tenant: &TenantContext,
        year: i32,
    ) -> ApplicationResult<i32>;

    async fn list_invoices(&self, tenant: &TenantContext) -> ApplicationResult<Vec<InvoiceView>>;

    async fn find_invoice(
        &self,
        tenant: &TenantContext,
        id: InvoiceId,
    ) -> ApplicationResult<Option<Invoice>>;

    async fn insert_invoice(
        &self,
        tenant: &TenantContext,
        invoice: &Invoice,
    ) -> ApplicationResult<()>;

    /// Persist a lifecycle move (paid/void). Compare-and-swap on still-due, so
    /// two managers settling the same invoice cannot both win.
    async fn save_invoice_state(
        &self,
        tenant: &TenantContext,
        invoice: &Invoice,
        action: &'static str,
    ) -> ApplicationResult<()>;

    /// Record money received, and settle the invoice in the SAME transaction
    /// when it covers the balance — a payment that lands while its invoice
    /// stays "due" is exactly the inconsistency an audit log exists to catch.
    async fn insert_payment(
        &self,
        tenant: &TenantContext,
        payment: &Payment,
        settles: bool,
    ) -> ApplicationResult<()>;

    async fn payments_for(
        &self,
        tenant: &TenantContext,
        invoice: InvoiceId,
    ) -> ApplicationResult<Vec<Payment>>;
}

/// A real payment processor, behind a seam (ADR-0010). `Payment` records what
/// a gym says it received, regardless of how — a manager typing in cash they
/// were handed and a member's card charge confirmed here both end up as the
/// same row. This is the one place that talks to the outside world to make
/// the self-service case (a member paying their own bill) actually happen,
/// rather than a manager having to record it after the fact.
/// A started checkout: where to send the payer, and how we will recognise it.
///
/// The reference used to be discarded — `create_checkout_session` returned only
/// a URL — which meant the only record that an attempt existed was whatever the
/// processor later told us. Returning it lets the caller log the attempt before
/// the redirect, and gives every gateway one idempotency key with one shape.
#[derive(Debug, Clone)]
pub struct CheckoutSession {
    /// Absolute, because a browser follows it.
    pub url: String,
    /// The gateway's own id for this attempt. Unique per attempt, which is what
    /// makes a redelivered confirmation a no-op rather than a second charge.
    pub provider_ref: String,
}

#[async_trait]
pub trait PaymentGateway: Send + Sync {
    /// Start a hosted checkout for one invoice's outstanding balance.
    ///
    /// Nothing about the invoice changes here. It settles only when the
    /// processor confirms payment — a signed webhook for Stripe, a submitted
    /// card page for the self-hosted gateway — and both paths converge on
    /// `BillingService::apply_gateway_payment`.
    async fn create_checkout_session(
        &self,
        req: CheckoutSessionRequest,
    ) -> ApplicationResult<CheckoutSession>;
}

/// What a checkout needs to know. `amount_minor` is the OUTSTANDING balance,
/// not the invoice total — a part-paid invoice must not be charged twice.
#[derive(Debug, Clone)]
pub struct CheckoutSessionRequest {
    pub gym_id: GymId,
    pub invoice_id: InvoiceId,
    pub member_id: UserId,
    pub amount_minor: i64,
    pub currency: String,
    pub description: String,
    pub success_url: String,
    pub cancel_url: String,
}

/// The gym's coaches, as a member browsing for one sees them.
#[async_trait]
pub trait TrainerDirectoryRepository: Send + Sync {
    async fn trainers(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<TrainerDirectoryEntry>>;
}

/// Tenant-scoped access to the operating calendar (ADR-0015).
///
/// Reads rows; decides nothing. The resolution rule lives in
/// `gym_domain::calendar::resolve_day` and is not re-implemented in SQL — a
/// second copy would be the one nobody unit-tests.
#[async_trait]
pub trait CalendarRepository: Send + Sync {
    async fn opening_hours(&self, tenant: &TenantContext) -> ApplicationResult<Vec<WeeklyHours>>;

    async fn overrides_between(
        &self,
        tenant: &TenantContext,
        from: chrono::NaiveDate,
        to: chrono::NaiveDate,
    ) -> ApplicationResult<Vec<CalendarOverride>>;

    /// Replace the weekly pattern wholesale, in one transaction.
    ///
    /// Wholesale rather than diffed: the pattern is a handful of rows edited
    /// as a unit ("these are our hours"), so a diff would be more code for the
    /// same result — and doing it in one transaction is what stops a reader
    /// seeing a gym with no hours at all.
    async fn replace_opening_hours(
        &self,
        tenant: &TenantContext,
        hours: &[WeeklyHours],
    ) -> ApplicationResult<()>;

    async fn upsert_override(
        &self,
        tenant: &TenantContext,
        entry: &CalendarOverride,
    ) -> ApplicationResult<()>;

    /// Remove an override so the pattern applies again. Returns whether there
    /// was one.
    async fn remove_override(
        &self,
        tenant: &TenantContext,
        on_date: chrono::NaiveDate,
    ) -> ApplicationResult<bool>;

    async fn trainer_availability(
        &self,
        tenant: &TenantContext,
        trainer: UserId,
    ) -> ApplicationResult<Vec<WeeklyHours>>;

    async fn replace_trainer_availability(
        &self,
        tenant: &TenantContext,
        trainer: UserId,
        hours: &[WeeklyHours],
    ) -> ApplicationResult<()>;

    /// The gym's IANA zone. Wall-clock times mean nothing without it.
    async fn timezone(&self, tenant: &TenantContext) -> ApplicationResult<String>;
}

/// Tenant-scoped access to goals.
#[async_trait]
pub trait GoalRepository: Send + Sync {
    /// Every goal in the gym, active first, newest first. Scoping is the
    /// service's job, same as every other list here.
    async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<GoalView>>;

    async fn find(&self, tenant: &TenantContext, id: GoalId) -> ApplicationResult<Option<Goal>>;

    async fn insert(&self, tenant: &TenantContext, goal: &Goal) -> ApplicationResult<()>;

    /// Persist a close (achieved/abandoned). Compare-and-swap on still-active.
    async fn save_closed(
        &self,
        tenant: &TenantContext,
        goal: &Goal,
        action: &'static str,
    ) -> ApplicationResult<()>;
}

/// A check-in, with the member's name for a screen to print — the same
/// "domain object + display name" shape as `InvoiceView`/`SubscriptionView`.
#[derive(Debug, Clone)]
pub struct CheckInView {
    pub checkin: CheckIn,
    pub member_name: String,
}

/// Append-only, like `AuditRepository` and performed sets: a scan is recorded,
/// never revised. Both an admission and a denial are inserted — see
/// `gym_domain::checkin`'s module doc for why a denial is worth keeping too.
#[async_trait]
pub trait CheckInRepository: Send + Sync {
    async fn insert(&self, tenant: &TenantContext, checkin: &CheckIn) -> ApplicationResult<()>;

    /// The most recent scans at this gym's door, newest first. Staff-only —
    /// enforced by `CheckInService`, same as everywhere else the service
    /// layer is the one place authorization is decided.
    async fn recent(&self, tenant: &TenantContext) -> ApplicationResult<Vec<CheckInView>>;
}

/// A member's entry pass: a short-lived, signed token proving "this account,
/// this gym, issued moments ago" — never stored (there is nothing to store;
/// its whole security property is that it stops being useful within seconds).
/// Kept as its own port rather than folded into `TokenIssuer` because an
/// entry pass must never be usable to call anything else the way an access
/// token can — two different secrets-shaped things stay two different types.
pub trait EntryPassIssuer: Send + Sync {
    fn issue(&self, member: UserId, gym: GymId, ttl_seconds: i64) -> ApplicationResult<String>;

    /// `Ok((member, gym))` only for a signature that verifies AND has not
    /// expired. Every failure — bad signature, expired, malformed — collapses
    /// to the same generic refusal at the call site, the same principle as
    /// `TokenIssuer::verify_access`.
    fn verify(&self, token: &str) -> ApplicationResult<(UserId, GymId)>;
}

/// One person's presence in a gym, for pickers and roster views.
///
/// Deliberately carries **no email address**. A name and the capacities held are
/// enough to choose someone from a list, and a roster endpoint is not a reason to
/// hand every manager a harvestable list of contact details. The roster already
/// expose the addresses a manager actually needs, and only the ones they sent.
#[derive(Debug, Clone)]
pub struct GymMember {
    pub user_id: UserId,
    pub display_name: String,
    pub capabilities: Capabilities,
}

/// A user plus their credential hash. Kept separate from the domain `User` so the
/// hash cannot accidentally be serialized into an API response.
#[derive(Debug, Clone)]
pub struct StoredUser {
    pub user: User,
    pub password_hash: String,
}

#[async_trait]
pub trait GymRepository: Send + Sync {
    /// Create a gym and grant its creator `owner` **atomically**.
    ///
    /// One transaction: a gym with no owner is an unusable state we refuse to
    /// create. The user already exists — accounts and gyms are created
    /// separately so onboarding can ask for an account first (ADR-0014).
    async fn create_with_owner(&self, gym: &Gym, owner: UserId) -> ApplicationResult<()>;

    async fn find(&self, id: GymId) -> ApplicationResult<Option<Gym>>;

    /// Gyms currently accepting members without an invitation (ADR-0026).
    ///
    /// The one read in the system that runs with **no tenant context**, because
    /// the caller holds no membership yet — that is the whole situation it
    /// exists to resolve. It is safe precisely because of what it returns: an
    /// id and a name for gyms that have publicly opted in to being findable.
    /// Nothing about who trains there.
    async fn open_for_registration(&self) -> ApplicationResult<Vec<Gym>>;

    /// Add the caller to a gym as a plain `member`.
    ///
    /// Re-checks `open_registration` inside the transaction rather than
    /// trusting a prior read: an owner closing the door and a stranger walking
    /// through it can race, and the door must win.
    async fn join_as_member(&self, gym: GymId, user: UserId) -> ApplicationResult<()>;

    /// Open or close the door. Owner-controlled; audited.
    async fn set_open_registration(
        &self,
        tenant: &TenantContext,
        open: bool,
    ) -> ApplicationResult<Gym>;
}

// --------------------------------------------------------------------- audit

/// An action to record. Metadata is context worth keeping — **never** secrets.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    /// Dotted, past-tense, stable: `exercise.created`, `capacity.granted`.
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<uuid::Uuid>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub id: uuid::Uuid,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<uuid::Uuid>,
    /// Resolved for display; `None` if the account was since removed.
    pub actor_name: Option<String>,
    pub metadata: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn recent(
        &self,
        tenant: &TenantContext,
        limit: i64,
    ) -> ApplicationResult<Vec<AuditRecord>>;

    /// Standalone write. Prefer recording inside the mutation's own transaction —
    /// see `gym_infrastructure::audit::record_in_tx`.
    async fn record(&self, tenant: &TenantContext, entry: AuditEntry) -> ApplicationResult<()>;
}

// ------------------------------------------------------------------ sessions

/// A refresh-token session as stored. The raw token is never persisted — only
/// its hash — so a database leak does not yield usable tokens.
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: SessionId,
    pub user_id: UserId,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl SessionRecord {
    #[must_use]
    pub fn is_usable(&self, now: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && self.expires_at > now
    }
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub id: SessionId,
    pub user_id: UserId,
    pub token_hash: Vec<u8>,
    pub expires_at: DateTime<Utc>,
    pub rotated_from: Option<SessionId>,
    pub device_label: Option<String>,
}

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn create(&self, session: &NewSession) -> ApplicationResult<()>;

    async fn find_by_token_hash(&self, hash: &[u8]) -> ApplicationResult<Option<SessionRecord>>;

    /// Atomically revoke a session.
    ///
    /// Returns `true` only if **this call** performed the revocation. A `false`
    /// means someone else revoked it concurrently — which is the compare-and-swap
    /// that makes refresh rotation safe against races. Implementations MUST make
    /// this a single conditional statement (`WHERE revoked_at IS NULL`), not a
    /// read-then-write.
    async fn revoke(&self, id: SessionId) -> ApplicationResult<bool>;

    /// Revoke every active session for a user — the response to token reuse.
    async fn revoke_all_for_user(&self, user: UserId) -> ApplicationResult<u64>;
}

// -------------------------------------------------------------------- tokens

#[derive(Debug, Clone)]
pub struct AccessToken {
    pub token: String,
    pub expires_in_seconds: i64,
}

/// A freshly generated opaque token: the raw value goes to the client exactly
/// once, only the hash is stored. Used for refresh sessions and for the
/// both want "unguessable secret, useless if the database leaks".
#[derive(Debug, Clone)]
pub struct OpaqueToken {
    pub raw: String,
    pub hash: Vec<u8>,
}

pub trait TokenIssuer: Send + Sync {
    /// Mint a short-lived access token. Carries no sensitive data — JWT payloads
    /// are encoded, not encrypted.
    fn issue_access(&self, user: UserId) -> ApplicationResult<AccessToken>;

    /// Verify signature and expiry, returning the subject.
    fn verify_access(&self, token: &str) -> ApplicationResult<UserId>;

    /// 256 bits of entropy, hex-encoded.
    fn generate_opaque(&self) -> ApplicationResult<OpaqueToken>;

    /// Hash a client-presented token for lookup by hash.
    fn hash_opaque(&self, raw: &str) -> Vec<u8>;

    fn refresh_ttl_days(&self) -> i64;
}

// ------------------------------------------------------------ auth hardening

/// What a single-use emailed secret is for (ADR-0029).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthTokenPurpose {
    PasswordReset,
    EmailVerification,
}

impl AuthTokenPurpose {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PasswordReset => "password_reset",
            Self::EmailVerification => "email_verification",
        }
    }
}

/// A token row as stored — hash only, never the raw value.
#[derive(Debug, Clone)]
pub struct AuthToken {
    pub id: uuid::Uuid,
    pub user_id: UserId,
    pub purpose: AuthTokenPurpose,
    /// The address it was sent to. A link forwarded to somebody else is
    /// useless, and a link that predates an email change stops working.
    pub email: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub used_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[async_trait]
pub trait AuthTokenRepository: Send + Sync {
    async fn issue(
        &self,
        user: UserId,
        purpose: AuthTokenPurpose,
        email: &str,
        token_hash: Vec<u8>,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> ApplicationResult<()>;

    /// Find by hash. The only way in — there is deliberately no "list this
    /// user's live tokens", because no legitimate caller needs one.
    async fn find_by_hash(&self, token_hash: &[u8]) -> ApplicationResult<Option<AuthToken>>;

    /// Burn a token. Returns whether THIS call did it — the same
    /// compare-and-swap discipline as refresh rotation, so two concurrent
    /// redemptions cannot both succeed.
    async fn consume(
        &self,
        id: uuid::Uuid,
        now: chrono::DateTime<chrono::Utc>,
    ) -> ApplicationResult<bool>;

    /// Invalidate everything outstanding for one purpose.
    ///
    /// Called when a reset completes: any other reset link in the person's
    /// inbox — including one an attacker requested — must stop working the
    /// moment the password changes.
    async fn invalidate_all(
        &self,
        user: UserId,
        purpose: AuthTokenPurpose,
        now: chrono::DateTime<chrono::Utc>,
    ) -> ApplicationResult<()>;
}

/// One message to send.
#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub to: String,
    pub subject: String,
    pub body: String,
    /// `password_reset`, `email_verification`, … so a reader can filter
    /// without pattern-matching on the subject line.
    pub kind: String,
}

/// Outbound mail.
///
/// A port with no production adapter yet, and that is stated rather than
/// hidden: the platform has never sent an email, so the adapters are a
/// recorder (development, demo, and the verification suite) and nothing else.
/// What matters is that the call sites are real — the day an SMTP adapter
/// lands, nothing above this line changes.
#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send(&self, message: EmailMessage) -> ApplicationResult<()>;
}

/// How many recent failures an identity has accumulated.
#[derive(Debug, Clone, Copy, Default)]
pub struct AttemptCounts {
    pub by_email: i64,
    pub by_ip: i64,
}

/// The login throttle's ledger.
#[async_trait]
pub trait LoginAttemptRepository: Send + Sync {
    async fn record(&self, email: &str, ip: Option<&str>, succeeded: bool)
    -> ApplicationResult<()>;

    /// Failures within the window, counted separately for the address and the
    /// origin — one attacker spraying many accounts from one host and one
    /// account being hammered from a botnet are different shapes, and only
    /// counting one of them misses the other.
    async fn recent_failures(
        &self,
        email: &str,
        ip: Option<&str>,
        since: chrono::DateTime<chrono::Utc>,
    ) -> ApplicationResult<AttemptCounts>;
}

// ---------------------------------------------------------------- primitives

pub trait PasswordHasher: Send + Sync {
    fn hash(&self, plaintext: &str) -> ApplicationResult<String>;

    fn verify(&self, plaintext: &str, hash: &str) -> ApplicationResult<bool>;
}

/// A version together with everything inside it, ordered for display.
///
/// Returned as one structure because a programme is read as a whole — a coach
/// opening week 3 wants the surrounding plan, and fetching weeks, then workouts,
/// then exercises separately is three round trips and a partially-rendered screen.
#[derive(Debug, Clone)]
pub struct VersionContent {
    pub version: ProgramVersion,
    pub weeks: Vec<ProgramWeek>,
    pub workouts: Vec<WorkoutTemplate>,
    pub exercises: Vec<TemplateExercise>,
}

/// Tenant-scoped access to programmes and their versions.
///
/// The lifecycle itself lives in the domain; this only loads and stores. Any
/// method that writes version *content* must be given a version the caller has
/// already checked is editable — and the database refuses regardless
/// (migration 0007), so a missed check is a failed write, not corrupted history.
#[async_trait]
pub trait ProgramRepository: Send + Sync {
    async fn list(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<(Program, ProgramVersion)>>;

    async fn find(
        &self,
        tenant: &TenantContext,
        id: ProgramId,
    ) -> ApplicationResult<Option<Program>>;

    /// Every version of a programme, newest first.
    async fn versions(
        &self,
        tenant: &TenantContext,
        program: ProgramId,
    ) -> ApplicationResult<Vec<ProgramVersion>>;

    async fn find_version(
        &self,
        tenant: &TenantContext,
        id: ProgramVersionId,
    ) -> ApplicationResult<Option<ProgramVersion>>;

    /// A version and all of its content, ordered.
    async fn load_content(
        &self,
        tenant: &TenantContext,
        id: ProgramVersionId,
    ) -> ApplicationResult<Option<VersionContent>>;

    /// Insert a programme and its first draft version in one transaction.
    ///
    /// Atomic on purpose: a programme with no version is not a thing the rest of
    /// the system knows how to handle, and a half-created one would have to be
    /// cleaned up by hand.
    async fn insert_program(
        &self,
        tenant: &TenantContext,
        program: &Program,
        first_version: &ProgramVersion,
    ) -> ApplicationResult<()>;

    async fn insert_version(
        &self,
        tenant: &TenantContext,
        version: &ProgramVersion,
    ) -> ApplicationResult<()>;

    /// Persist a lifecycle move. `action` names it for the audit trail.
    async fn save_status(
        &self,
        tenant: &TenantContext,
        version: &ProgramVersion,
        action: &'static str,
    ) -> ApplicationResult<()>;

    async fn insert_week(
        &self,
        tenant: &TenantContext,
        week: &ProgramWeek,
    ) -> ApplicationResult<()>;

    async fn find_week(
        &self,
        tenant: &TenantContext,
        id: ProgramWeekId,
    ) -> ApplicationResult<Option<ProgramWeek>>;

    async fn insert_workout(
        &self,
        tenant: &TenantContext,
        workout: &WorkoutTemplate,
    ) -> ApplicationResult<()>;

    async fn find_workout(
        &self,
        tenant: &TenantContext,
        id: WorkoutTemplateId,
    ) -> ApplicationResult<Option<WorkoutTemplate>>;

    async fn insert_template_exercise(
        &self,
        tenant: &TenantContext,
        exercise: &TemplateExercise,
    ) -> ApplicationResult<()>;

    /// Next free position in a workout, so callers need not guess.
    async fn next_position(
        &self,
        tenant: &TenantContext,
        workout: WorkoutTemplateId,
    ) -> ApplicationResult<i32>;

    /// How many exercises a version prescribes, across every week and workout —
    /// the review gate needs it.
    ///
    /// Exercises, not weeks: an empty week (or a workout with nothing in it) is
    /// not a trainable plan, and counting containers let such versions reach
    /// published, where they can no longer be fixed.
    async fn prescribed_exercise_count(
        &self,
        tenant: &TenantContext,
        version: ProgramVersionId,
    ) -> ApplicationResult<usize>;
}

/// A relationship with both parties' names resolved, for display.
///
/// The names are joined in the query rather than fetched per row: a client list
/// is the archetypal N+1, and "12 clients" would otherwise be 13 round trips.
#[derive(Debug, Clone)]
pub struct CoachRelationshipView {
    pub relationship: CoachRelationship,
    pub coach_name: String,
    pub athlete_name: String,
}

/// A coaching request with both parties named, so a list screen needs no
/// second round of lookups.
#[derive(Debug, Clone)]
pub struct CoachingRequestView {
    pub request: CoachingRequest,
    pub athlete_name: String,
    pub coach_name: String,
}

/// One coach as a member browsing for one sees them.
///
/// Emphatically NOT a roster row: this carries only what a coach chose to
/// publish about themselves professionally, plus a client count. No email, and
/// no other member's name. `GymService::roster` stays head-coach-only
/// (ADR-0025).
#[derive(Debug, Clone)]
pub struct TrainerDirectoryEntry {
    pub user_id: UserId,
    pub display_name: String,
    pub headline: Option<String>,
    pub bio: Option<String>,
    pub specialties: Vec<String>,
    pub certifications: Vec<String>,
    /// How many people they currently coach here. A capacity signal, not a
    /// popularity score — a member deciding who to ask deserves to know if
    /// someone already has twenty clients.
    pub active_clients: i64,
}

/// Tenant-scoped access to coaching requests (ADR-0025).
#[async_trait]
pub trait CoachingRequestRepository: Send + Sync {
    /// Record a choice that was granted the moment it was made, together with
    /// the relationship it creates — one transaction (ADR-0031).
    ///
    /// Separate from `insert` rather than a flag on it: the two write
    /// different rows and only one of them grants anybody access to anybody
    /// else's data, which is not a difference to hide behind a boolean.
    async fn insert_chosen(
        &self,
        tenant: &TenantContext,
        request: &gym_domain::coaching_request::CoachingRequest,
        pairing: &gym_domain::coaching::CoachRelationship,
    ) -> ApplicationResult<()>;

    /// Every request in the gym, newest first. Scoping is the service's job,
    /// for the same reason `CoachRepository::list` is unfiltered: who may see
    /// what is a domain decision, and pushing it into SQL puts one rule in two
    /// places.
    async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<CoachingRequestView>>;

    async fn find(
        &self,
        tenant: &TenantContext,
        id: CoachingRequestId,
    ) -> ApplicationResult<Option<CoachingRequest>>;

    async fn insert(
        &self,
        tenant: &TenantContext,
        request: &CoachingRequest,
    ) -> ApplicationResult<()>;

    /// Record a decision.
    ///
    /// When `pairing` is present the relationship is created in the SAME
    /// transaction as the acceptance — otherwise there is a window in which a
    /// member has been accepted and their coach cannot see them, which is
    /// exactly the kind of half-state the audit log is written transactionally
    /// to avoid.
    async fn save_decision(
        &self,
        tenant: &TenantContext,
        request: &CoachingRequest,
        pairing: Option<&CoachRelationship>,
        action: &'static str,
    ) -> ApplicationResult<()>;
}

/// Tenant-scoped access to coach–athlete relationships.
#[async_trait]
pub trait CoachRepository: Send + Sync {
    /// Every relationship in the gym, active first, newest first.
    ///
    /// Deliberately unfiltered: *who may see what* is a domain decision, and
    /// pushing it into SQL would put the same rule in two places. Callers filter
    /// through `gym_domain::coaching`.
    async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<CoachRelationshipView>>;

    /// Active relationships involving this person, as coach or as athlete.
    /// The input to `may_view_athlete`.
    async fn active_for_user(
        &self,
        tenant: &TenantContext,
        user: UserId,
    ) -> ApplicationResult<Vec<CoachRelationship>>;

    async fn find(
        &self,
        tenant: &TenantContext,
        id: CoachRelationshipId,
    ) -> ApplicationResult<Option<CoachRelationship>>;

    async fn insert(
        &self,
        tenant: &TenantContext,
        relationship: &CoachRelationship,
    ) -> ApplicationResult<()>;

    /// Persist an ending. Never deletes — see migration 0009.
    async fn save_ended(
        &self,
        tenant: &TenantContext,
        relationship: &CoachRelationship,
    ) -> ApplicationResult<()>;
}

/// An assignment with everything a list screen needs, resolved in one query.
#[derive(Debug, Clone)]
pub struct AssignmentView {
    pub assignment: ProgramAssignment,
    pub athlete_name: String,
    pub program_name: String,
    pub version_number: i32,
}

/// Tenant-scoped access to programme assignments.
#[async_trait]
pub trait AssignmentRepository: Send + Sync {
    /// Every assignment in the gym, active first, newest first. Unfiltered for
    /// the same reason as `CoachRepository::list`: who may see what is a domain
    /// rule, and it lives in one place, not also in SQL.
    async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<AssignmentView>>;

    async fn find(
        &self,
        tenant: &TenantContext,
        id: AssignmentId,
    ) -> ApplicationResult<Option<ProgramAssignment>>;

    async fn insert(
        &self,
        tenant: &TenantContext,
        assignment: &ProgramAssignment,
    ) -> ApplicationResult<()>;

    /// Persist an ending (withdrawn/completed). Compare-and-swap on the row
    /// still being active; never deletes.
    async fn save_ended(
        &self,
        tenant: &TenantContext,
        assignment: &ProgramAssignment,
        action: &'static str,
    ) -> ApplicationResult<()>;
}

/// A session with the display context a history list needs.
#[derive(Debug, Clone)]
pub struct SessionView {
    pub session: WorkoutSession,
    pub athlete_name: String,
    /// The workout template's name — `None` for an unplanned session, which has
    /// no template. The session's own `title` is what names one of those.
    pub workout_name: Option<String>,
    /// `None` for an unplanned session, for the same reason.
    pub program_name: Option<String>,
    pub set_count: i64,
}

/// One session's worth of one exercise's history: when, what state, which sets.
#[derive(Debug, Clone)]
pub struct ExerciseHistoryEntry {
    pub session_id: WorkoutSessionId,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub session_status: String,
    pub sets: Vec<PerformedSet>,
}

/// Narrowing for the session list.
///
/// The list used to return everything the caller could see, capped at 200. That
/// is fine for a gym in its first month and wrong by its second year — and it
/// made the coach's obvious question ("show me Sara's last six weeks") a
/// client-side filter over a truncated set, which quietly answers the wrong
/// question once the cap bites.
///
/// All fields optional; the default is the previous behaviour.
#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    /// One athlete's history. Still subject to the caller's own visibility —
    /// this narrows a permitted set, it never widens one.
    pub athlete_id: Option<UserId>,
    /// Inclusive lower bound on `started_at`, by date in UTC.
    pub from: Option<chrono::NaiveDate>,
    /// Inclusive upper bound on `started_at`, by date in UTC.
    pub to: Option<chrono::NaiveDate>,
    /// Hard cap. Clamped by the repository — a client asking for a million rows
    /// gets the maximum, not a timeout.
    pub limit: Option<i64>,
}

/// Tenant-scoped access to workout sessions and performed sets.
///
/// Inserts are **idempotent on id** (ADR-0008): a replayed insert of a known id
/// reports `false` ("nothing new") instead of failing, which is what makes
/// offline sync retries safe.
#[async_trait]
pub trait ExecutionRepository: Send + Sync {
    async fn list(
        &self,
        tenant: &TenantContext,
        filter: &SessionFilter,
    ) -> ApplicationResult<Vec<SessionView>>;

    async fn find_session(
        &self,
        tenant: &TenantContext,
        id: WorkoutSessionId,
    ) -> ApplicationResult<Option<WorkoutSession>>;

    /// One session WITH its display metadata — the same projection `list`
    /// returns. The logging screen needs the workout's name, and resolving it
    /// per-caller from the list is both wasteful and easy to get wrong.
    async fn find_session_view(
        &self,
        tenant: &TenantContext,
        id: WorkoutSessionId,
    ) -> ApplicationResult<Option<SessionView>>;

    /// The sets of one session, ordered by exercise position then set number.
    async fn sets_of(
        &self,
        tenant: &TenantContext,
        session: WorkoutSessionId,
    ) -> ApplicationResult<Vec<PerformedSet>>;

    /// Returns `true` if this call inserted the row, `false` if the id was
    /// already known (an idempotent replay).
    async fn insert_session(
        &self,
        tenant: &TenantContext,
        session: &WorkoutSession,
    ) -> ApplicationResult<bool>;

    async fn insert_set(
        &self,
        tenant: &TenantContext,
        set: &PerformedSet,
    ) -> ApplicationResult<bool>;

    /// Persist a finish (completed/abandoned). Compare-and-swap on still-open.
    async fn save_finished(
        &self,
        tenant: &TenantContext,
        session: &WorkoutSession,
        action: &'static str,
    ) -> ApplicationResult<()>;

    /// Every set this athlete has logged of one exercise, grouped by session,
    /// oldest first — the raw truth a progress view derives its numbers from.
    /// Derived metrics (estimated 1RM, trends) are deliberately NOT computed
    /// here: progress is computed at the edge, never stored where it can drift.
    async fn exercise_history(
        &self,
        tenant: &TenantContext,
        exercise: ExerciseId,
        athlete: UserId,
    ) -> ApplicationResult<Vec<ExerciseHistoryEntry>>;
}

/// Injectable clock so time-dependent logic (token expiry) is testable.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// One timetable row as a screen needs it: the slot, who teaches it, and how
/// many places are already gone for the date being shown.
///
/// The count is joined in rather than fetched per row — a timetable is the
/// archetypal N+1, and "12 classes" would otherwise be 13 round trips (the
/// same reasoning as `CoachRelationshipView`'s joined names).
#[derive(Debug, Clone)]
pub struct ClassOnDate {
    pub class: GymClass,
    pub instructor_name: String,
    /// The date this row is being shown for — derived from the weekly slot, so
    /// the same class appears once per week in a multi-week window.
    pub on_date: NaiveDate,
    pub booked: u32,
    /// Whether the CALLER holds a live place. Resolved in the query because the
    /// alternative is every client re-deriving it from a second list.
    pub booked_by_me: bool,
    /// The caller's own booking, when they hold one — so the Cancel control has
    /// the id it needs. Without it a client knows it is booked and cannot say
    /// which booking to release, which is a screen that can show "Booked" and
    /// nothing to do about it.
    pub my_booking_id: Option<ClassBookingId>,
}

/// Tenant-scoped access to the class timetable and its bookings.
#[async_trait]
pub trait ClassRepository: Send + Sync {
    /// Every live class in the gym, timetable order (weekday, then start).
    async fn list_classes(&self, tenant: &TenantContext) -> ApplicationResult<Vec<GymClass>>;

    async fn find_class(
        &self,
        tenant: &TenantContext,
        id: GymClassId,
    ) -> ApplicationResult<Option<GymClass>>;

    async fn insert_class(
        &self,
        tenant: &TenantContext,
        class: &GymClass,
    ) -> ApplicationResult<()>;

    /// Persist an archive. Compare-and-swap on still-live, so two managers
    /// archiving at once cannot both believe they were first.
    async fn save_archived_class(
        &self,
        tenant: &TenantContext,
        class: &GymClass,
    ) -> ApplicationResult<()>;

    /// The timetable for a date window, one row per class per occurrence,
    /// with occupancy and whether `for_member` already holds a place.
    async fn timetable(
        &self,
        tenant: &TenantContext,
        from: NaiveDate,
        to: NaiveDate,
        for_member: UserId,
    ) -> ApplicationResult<Vec<ClassOnDate>>;

    /// Live places already taken for one occurrence. The capacity rule needs
    /// it; the unique index is what settles a genuine race.
    async fn live_booking_count(
        &self,
        tenant: &TenantContext,
        class: GymClassId,
        on_date: NaiveDate,
    ) -> ApplicationResult<u32>;

    async fn find_booking(
        &self,
        tenant: &TenantContext,
        id: ClassBookingId,
    ) -> ApplicationResult<Option<ClassBooking>>;

    /// Insert a booking. A replayed id is a no-op rather than a second place
    /// (ADR-0008), so a retry on a bad connection cannot double-book.
    async fn insert_booking(
        &self,
        tenant: &TenantContext,
        booking: &ClassBooking,
    ) -> ApplicationResult<()>;

    async fn save_cancelled_booking(
        &self,
        tenant: &TenantContext,
        booking: &ClassBooking,
    ) -> ApplicationResult<()>;

    /// Who is booked into one occurrence — the instructor's roster.
    async fn roster(
        &self,
        tenant: &TenantContext,
        class: GymClassId,
        on_date: NaiveDate,
    ) -> ApplicationResult<Vec<(UserId, String)>>;

    /// The gym's IANA zone, for resolving a wall-clock start into a moment.
    async fn timezone(&self, tenant: &TenantContext) -> ApplicationResult<String>;
}
