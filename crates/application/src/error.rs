//! Application-level errors. Deliberately structured (not `anyhow`) so callers —
//! and the HTTP layer — can branch on the failure kind.

use gym_domain::DomainError;

pub type ApplicationResult<T> = Result<T, ApplicationError>;

#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    /// A domain invariant was violated — "is this action valid?"
    #[error(transparent)]
    Domain(#[from] DomainError),

    /// The actor may not attempt this action — "may this actor?"
    #[error("not permitted")]
    Forbidden,

    /// Authentication is missing or invalid.
    #[error("not authenticated")]
    Unauthenticated,

    #[error("{entity} not found")]
    NotFound { entity: &'static str },

    #[error("{0} already exists")]
    Conflict(String),

    /// Well-formed request, but the state has moved on — and the message says
    /// how: "Zumba is full (20 places)", "that sitting has already started".
    ///
    /// Distinct from `Conflict`, which is specifically a DUPLICATE and renders
    /// through an "{0} already exists" template. Pushing a state conflict
    /// through that produced "Zumba is full (20 places) already exists", which
    /// is worse than saying nothing — so the two kinds stay two variants. Both
    /// are 409: the caller should look again rather than rewrite the request.
    #[error("{0}")]
    StateConflict(String),

    /// A real capability, deliberately not configured for this deployment —
    /// distinct from `Internal`: nothing is broken, the operator has simply
    /// not turned this on (e.g. Stripe: ADR-0010 calls it a seam until keys
    /// are set). Safe to explain to the caller.
    #[error("{0} is not configured")]
    Unavailable(String),

    /// Too many failed sign-in attempts. Distinct from `Unauthenticated`
    /// because the caller's next move is different — waiting works, retrying
    /// does not — and because a client should be able to say so without
    /// parsing a message.
    #[error("too many attempts; try again shortly")]
    TooManyAttempts,

    /// A single-use emailed link that is unknown, expired, already spent, or
    /// meant for something else.
    ///
    /// **One variant for all four on purpose.** Distinguishing them tells a
    /// caller holding a guessed token which part of the guess was right, which
    /// is exactly the feedback loop a brute force needs.
    #[error("that link is no longer valid")]
    InvalidToken,

    /// Infrastructure failure (DB, network). Carries no user-facing detail.
    #[error("internal error")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl ApplicationError {
    pub fn internal<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Internal(Box::new(err))
    }

    /// Stable, machine-readable error code for API clients to branch on.
    /// Clients must never parse human-readable messages.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Domain(_) => "request.invalid",
            Self::Forbidden => "auth.forbidden",
            Self::Unauthenticated => "auth.unauthenticated",
            Self::NotFound { .. } => "resource.not_found",
            Self::Conflict(_) | Self::StateConflict(_) => "resource.conflict",
            Self::Unavailable(_) => "resource.unavailable",
            Self::TooManyAttempts => "auth.too_many_attempts",
            Self::InvalidToken => "auth.invalid_token",
            Self::Internal(_) => "internal.error",
        }
    }
}
