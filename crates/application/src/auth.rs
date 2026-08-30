//! Authentication use-cases: sign-up, login, refresh rotation, logout.
//!
//! Design (docs/research-2026.md §5 + the agreed auth model):
//! - short-lived access token (JWT) + long-lived **opaque, rotating** refresh token
//! - only the refresh token's *hash* is stored
//! - presenting an already-rotated token means it leaked → revoke the whole family

use std::sync::Arc;

use gym_domain::{
    SessionId, UserId,
    user::{Email, User},
};

use crate::{
    ApplicationError, ApplicationResult,
    ports::{
        AccessToken, AuthTokenPurpose, AuthTokenRepository, Clock, EmailMessage, EmailSender,
        GymRepository, LoginAttemptRepository, Membership, NewSession, PasswordHasher,
        SessionRepository, TokenIssuer, UserRepository,
    },
};

/// Minimum password length. Length beats composition rules — see NIST SP 800-63B.
const MIN_PASSWORD_LEN: usize = 12;

/// How far back the login throttle looks.
const THROTTLE_WINDOW_MINUTES: i64 = 15;

/// Failures against ONE address before it is locked for the window.
///
/// Ten is deliberately generous. A person who genuinely cannot remember which
/// of their three passwords they used here should not be locked out, and the
/// thing this defends against is not ten guesses — it is ten thousand.
const MAX_FAILURES_PER_EMAIL: i64 = 10;

/// Failures from ONE origin before it is locked for the window.
///
/// Higher, because a whole gym behind one office NAT shares an address and a
/// Monday morning of forgotten passwords must not lock the building out.
const MAX_FAILURES_PER_IP: i64 = 50;

/// How long a password-reset link lives. Short: it is a credential in an inbox.
const RESET_TTL_MINUTES: i64 = 60;

/// How long a verification link lives. Longer — nothing is at risk if it sits
/// unread, and making someone re-request it is pure friction.
const VERIFICATION_TTL_HOURS: i64 = 72;

/// Issued credential pair returned to the client.
#[derive(Debug, Clone)]
pub struct IssuedTokens {
    pub access_token: String,
    pub expires_in_seconds: i64,
    pub refresh_token: String,
}

/// Create an account. Deliberately asks nothing about roles or gyms — what you
/// do first is a separate, reversible choice (ADR-0014 onboarding).
#[derive(Debug, Clone)]
pub struct SignUpCommand {
    pub email: String,
    pub password: String,
    pub display_name: String,
    pub device_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LoginCommand {
    pub email: String,
    pub password: String,
    pub device_label: Option<String>,
}

pub struct SignUpResult {
    pub user: User,
    pub tokens: IssuedTokens,
}

#[derive(Debug, Clone)]
pub struct LoginContext {
    /// Best-effort origin, for the throttle's second counter. `None` when the
    /// deployment sits behind something that does not forward it — the
    /// per-address counter still applies, which is the one that matters.
    pub ip: Option<String>,
}

#[derive(Clone)]
pub struct AuthService {
    pub users: Arc<dyn UserRepository>,
    pub gyms: Arc<dyn GymRepository>,
    pub sessions: Arc<dyn SessionRepository>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub tokens: Arc<dyn TokenIssuer>,
    pub clock: Arc<dyn Clock>,
    pub auth_tokens: Arc<dyn AuthTokenRepository>,
    pub attempts: Arc<dyn LoginAttemptRepository>,
    pub email: Arc<dyn EmailSender>,
    /// Where a link in an email should point. The app's deep-link scheme in
    /// development, a real https origin in production.
    pub link_base_url: String,
}

impl AuthService {
    /// Create an account and sign the person in.
    ///
    /// No gym, no role: onboarding asks what they want to do *after* they have an
    /// account, and that choice stays reversible (ADR-0014).
    pub async fn sign_up(&self, cmd: SignUpCommand) -> ApplicationResult<SignUpResult> {
        let email = Email::parse(&cmd.email)?;
        validate_password(&cmd.password)?;

        let user = User::new(email.clone(), &cmd.display_name)?;
        let password_hash = self.hasher.hash(&cmd.password)?;

        // Cheap pre-check for a friendly error; the DB unique index is the real
        // guarantee (this is a TOCTOU race, and the constraint settles it).
        if self.users.find_by_email(&email).await?.is_some() {
            return Err(ApplicationError::Conflict("email".to_owned()));
        }

        self.users.insert(&user, &password_hash).await?;

        let tokens = self.issue_session(user.id, None, cmd.device_label).await?;

        Ok(SignUpResult { user, tokens })
    }

    pub async fn login(
        &self,
        cmd: LoginCommand,
        context: &LoginContext,
    ) -> ApplicationResult<IssuedTokens> {
        // Normalised BEFORE anything else, because it is the throttle's key.
        // "Bob@Example.com" and "bob@example.com" are one account, and a
        // counter that treats them as two is a counter an attacker can reset
        // by changing the case.
        let typed = cmd.email.trim().to_lowercase();
        let ip = context.ip.as_deref();

        // --- the throttle ------------------------------------------------
        //
        // Checked before the password is even looked at, so a locked-out
        // attacker gets no timing signal and does no Argon2 work — which is
        // the other half of the point: Argon2 is deliberately expensive, and
        // an unthrottled login endpoint is therefore also a cheap way to burn
        // the server's CPU.
        let since = self.clock.now() - chrono::Duration::minutes(THROTTLE_WINDOW_MINUTES);
        let counts = self.attempts.recent_failures(&typed, ip, since).await?;

        if counts.by_email >= MAX_FAILURES_PER_EMAIL || counts.by_ip >= MAX_FAILURES_PER_IP {
            // Recorded, so hammering a locked account extends the lock rather
            // than letting an attacker wait it out for free.
            self.attempts.record(&typed, ip, false).await?;
            return Err(ApplicationError::TooManyAttempts);
        }

        let Ok(email) = Email::parse(&typed) else {
            // A malformed address still counts. Otherwise the throttle is
            // trivially bypassed by appending a space.
            self.attempts.record(&typed, ip, false).await?;
            return Err(ApplicationError::Unauthenticated);
        };

        let stored = self.users.find_by_email(&email).await?;

        // Always verify against *something* so response time doesn't reveal
        // whether the account exists, and always return the same error.
        let (password_hash, user_id) = match &stored {
            Some(s) => (s.password_hash.as_str(), Some(s.user.id)),
            None => (DUMMY_HASH, None),
        };

        let password_ok = self.hasher.verify(&cmd.password, password_hash)?;

        match (password_ok, user_id) {
            (true, Some(id)) => {
                self.attempts.record(&typed, ip, true).await?;
                self.issue_session(id, None, cmd.device_label).await
            }
            _ => {
                self.attempts.record(&typed, ip, false).await?;
                Err(ApplicationError::Unauthenticated)
            }
        }
    }

    // --------------------------------------------------------- password reset

    /// Start a password reset.
    ///
    /// **Always succeeds**, whether or not the address is registered. Telling
    /// the caller "no such account" turns this endpoint into a membership
    /// oracle — post an address, read the response, learn whether that person
    /// trains here — which is the same reasoning that makes invitation
    /// redemption unprobeable.
    pub async fn request_password_reset(&self, raw_email: &str) -> ApplicationResult<()> {
        let Ok(email) = Email::parse(raw_email.trim()) else {
            return Ok(());
        };
        let Some(stored) = self.users.find_by_email(&email).await? else {
            return Ok(());
        };

        let token = self.tokens.generate_opaque()?;
        let expires_at = self.clock.now() + chrono::Duration::minutes(RESET_TTL_MINUTES);

        self.auth_tokens
            .issue(
                stored.user.id,
                AuthTokenPurpose::PasswordReset,
                email.as_str(),
                token.hash,
                expires_at,
            )
            .await?;

        let link = format!(
            "{}/reset-password?token={}",
            self.link_base_url.trim_end_matches('/'),
            token.raw,
        );

        self.email
            .send(EmailMessage {
                to: email.as_str().to_owned(),
                subject: "Reset your password".to_owned(),
                body: format!(
                    "Hello {},\n\nSomeone asked to reset the password for this account. \
                     Open this link within the hour to choose a new one:\n\n{link}\n\n\
                     If that wasn't you, ignore this — nothing has changed, and the link \
                     stops working on its own.\n",
                    stored.user.display_name,
                ),
                kind: "password_reset".to_owned(),
            })
            .await?;

        Ok(())
    }

    /// Finish a password reset.
    ///
    /// Every failure returns the same error, for the same reason the request
    /// side does: a token that is unknown, expired, already used, or bound to
    /// a different address must be indistinguishable.
    pub async fn complete_password_reset(
        &self,
        raw_token: &str,
        new_password: &str,
    ) -> ApplicationResult<()> {
        validate_password(new_password)?;

        let hash = self.tokens.hash_opaque(raw_token);
        let now = self.clock.now();

        let token = self
            .auth_tokens
            .find_by_hash(&hash)
            .await?
            .filter(|t| t.purpose == AuthTokenPurpose::PasswordReset)
            .filter(|t| t.used_at.is_none() && t.expires_at > now)
            .ok_or(ApplicationError::InvalidToken)?;

        // Compare-and-swap, exactly like refresh rotation: two requests
        // presenting the same link at once must not both set a password.
        if !self.auth_tokens.consume(token.id, now).await? {
            return Err(ApplicationError::InvalidToken);
        }

        let password_hash = self.hasher.hash(new_password)?;
        self.users
            .set_password_hash(token.user_id, &password_hash)
            .await?;

        // Any OTHER reset link in that inbox — including one an attacker
        // requested — dies with this one.
        self.auth_tokens
            .invalidate_all(token.user_id, AuthTokenPurpose::PasswordReset, now)
            .await?;

        // Every session, everywhere. Changing a password is what someone does
        // when they think it is compromised, and leaving the attacker's
        // existing refresh token alive would make the reset theatre.
        let revoked = self.sessions.revoke_all_for_user(token.user_id).await?;
        tracing_password_reset(token.user_id, revoked);

        Ok(())
    }

    // ----------------------------------------------------- email verification

    /// Send (or resend) a verification link.
    pub async fn request_email_verification(&self, user: UserId) -> ApplicationResult<()> {
        let Some(account) = self.users.find_by_id(user).await? else {
            return Ok(());
        };
        if account.email_verified_at.is_some() {
            return Ok(());
        }

        let token = self.tokens.generate_opaque()?;
        let expires_at = self.clock.now() + chrono::Duration::hours(VERIFICATION_TTL_HOURS);

        self.auth_tokens
            .issue(
                user,
                AuthTokenPurpose::EmailVerification,
                account.email.as_str(),
                token.hash,
                expires_at,
            )
            .await?;

        let link = format!(
            "{}/verify-email?token={}",
            self.link_base_url.trim_end_matches('/'),
            token.raw,
        );

        self.email
            .send(EmailMessage {
                to: account.email.as_str().to_owned(),
                subject: "Confirm your email".to_owned(),
                body: format!(
                    "Hello {},\n\nConfirm this address so your gym knows how to reach \
                     you:\n\n{link}\n",
                    account.display_name,
                ),
                kind: "email_verification".to_owned(),
            })
            .await?;

        Ok(())
    }

    /// Redeem a verification link.
    pub async fn verify_email(&self, raw_token: &str) -> ApplicationResult<()> {
        let hash = self.tokens.hash_opaque(raw_token);
        let now = self.clock.now();

        let token = self
            .auth_tokens
            .find_by_hash(&hash)
            .await?
            .filter(|t| t.purpose == AuthTokenPurpose::EmailVerification)
            .filter(|t| t.used_at.is_none() && t.expires_at > now)
            .ok_or(ApplicationError::InvalidToken)?;

        if !self.auth_tokens.consume(token.id, now).await? {
            return Err(ApplicationError::InvalidToken);
        }

        // Bound to the address it was sent to. If the account changed its
        // email between issue and redemption, the old link must not verify the
        // new address — that would confirm an address nobody proved they can
        // read.
        let Some(account) = self.users.find_by_id(token.user_id).await? else {
            return Err(ApplicationError::InvalidToken);
        };
        if account.email.as_str() != token.email {
            return Err(ApplicationError::InvalidToken);
        }

        self.users.mark_email_verified(token.user_id, now).await
    }

    /// Rotate a refresh token.
    ///
    /// Reuse of an already-rotated/revoked token is treated as theft: every session
    /// for that user is revoked, forcing a fresh login on all devices.
    pub async fn refresh(
        &self,
        raw_refresh: &str,
        device_label: Option<String>,
    ) -> ApplicationResult<IssuedTokens> {
        let hash = self.tokens.hash_opaque(raw_refresh);

        let session = self
            .sessions
            .find_by_token_hash(&hash)
            .await?
            .ok_or(ApplicationError::Unauthenticated)?;

        let now = self.clock.now();

        if session.revoked_at.is_some() {
            let revoked = self.sessions.revoke_all_for_user(session.user_id).await?;
            tracing_reuse_detected(session.user_id, revoked);
            return Err(ApplicationError::Unauthenticated);
        }

        if !session.is_usable(now) {
            return Err(ApplicationError::Unauthenticated);
        }

        // Atomically claim the rotation. The check above is advisory only — two
        // concurrent requests can both pass it. This conditional revoke is the
        // real gate: exactly one caller gets `true`.
        //
        // Losing the race means the same token was presented twice at once, which
        // we cannot distinguish from theft, so we fail closed and burn the family.
        if !self.sessions.revoke(session.id).await? {
            let revoked = self.sessions.revoke_all_for_user(session.user_id).await?;
            tracing_reuse_detected(session.user_id, revoked);
            return Err(ApplicationError::Unauthenticated);
        }

        self.issue_session(session.user_id, Some(session.id), device_label)
            .await
    }

    /// Best-effort logout: revoke the presented session if it exists.
    ///
    /// Losing the revoke race here is fine — the session ends up revoked either
    /// way, and logout is idempotent by design.
    pub async fn logout(&self, raw_refresh: &str) -> ApplicationResult<()> {
        let hash = self.tokens.hash_opaque(raw_refresh);
        if let Some(session) = self.sessions.find_by_token_hash(&hash).await? {
            let _ = self.sessions.revoke(session.id).await?;
        }
        Ok(())
    }

    pub async fn memberships(&self, user: UserId) -> ApplicationResult<Vec<Membership>> {
        self.users.memberships(user).await
    }

    async fn issue_session(
        &self,
        user: UserId,
        rotated_from: Option<SessionId>,
        device_label: Option<String>,
    ) -> ApplicationResult<IssuedTokens> {
        let AccessToken {
            token: access_token,
            expires_in_seconds,
        } = self.tokens.issue_access(user)?;

        let refresh = self.tokens.generate_opaque()?;
        let expires_at = self.clock.now() + chrono::Duration::days(self.tokens.refresh_ttl_days());

        self.sessions
            .create(&NewSession {
                id: SessionId::new(),
                user_id: user,
                token_hash: refresh.hash,
                expires_at,
                rotated_from,
                device_label: device_label.map(|l| l.chars().take(100).collect()),
            })
            .await?;

        Ok(IssuedTokens {
            access_token,
            expires_in_seconds,
            refresh_token: refresh.raw,
        })
    }
}

fn tracing_password_reset(user: UserId, revoked: u64) {
    // Security-relevant: a password change ends every session, and knowing how
    // many it ended is what tells an investigator whether somebody else was in
    // the account at the time.
    // eprintln, matching `tracing_reuse_detected` just below: this crate has
    // no logging dependency on purpose, and adding one for two lines would put
    // a transport choice in the use-case layer.
    eprintln!("SECURITY: password reset for user {user}; revoked {revoked} session(s)");
}

fn tracing_reuse_detected(user: UserId, revoked: u64) {
    // Security-relevant event: worth alerting on in production.
    eprintln!(
        "SECURITY: refresh token reuse detected for user {user}; revoked {revoked} session(s)"
    );
}

impl AuthService {
    /// Change your own password.
    ///
    /// Exists because of staff accounts (ADR-0032): somebody handed a
    /// generated starting password needs a way off it, and the only other
    /// route — the reset link — goes through an email the deployment does not
    /// send yet. Without this, a staff member's password would be one their
    /// manager read out and could still remember.
    ///
    /// Requires the current password. Not a formality: an access token is a
    /// bearer credential, and a stolen one should not be enough to lock the
    /// real owner out of their own account.
    pub async fn change_password(
        &self,
        user_id: UserId,
        current: &str,
        new_password: &str,
    ) -> ApplicationResult<()> {
        validate_password(new_password)?;

        // Two reads rather than a new port method: `find_by_id` carries no
        // hash and `find_by_email` does. This runs once in an account's life,
        // so the round trip is cheaper than the extra surface.
        let user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or(ApplicationError::Unauthenticated)?;
        let stored = self
            .users
            .find_by_email(&user.email)
            .await?
            .ok_or(ApplicationError::Unauthenticated)?;

        if !self.hasher.verify(current, &stored.password_hash)? {
            return Err(ApplicationError::Unauthenticated);
        }

        let password_hash = self.hasher.hash(new_password)?;
        self.users
            .set_password_hash(user_id, &password_hash)
            .await?;

        // Every session, everywhere, exactly as a reset does — including this
        // one. Changing a password is what somebody does when they think it is
        // known to someone else, and leaving that someone's refresh token
        // alive would make the change theatre.
        let revoked = self.sessions.revoke_all_for_user(user_id).await?;
        // Same reporting as a reset, because it is the same event from the
        // account's point of view. See `tracing_password_reset` for why this
        // crate prints rather than logs.
        tracing_password_reset(user_id, revoked);

        Ok(())
    }
}

fn validate_password(password: &str) -> ApplicationResult<()> {
    // Count chars, not bytes, so a short multi-byte password can't pass on length.
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(ApplicationError::Domain(gym_domain::DomainError::Invalid(
            format!("password must be at least {MIN_PASSWORD_LEN} characters"),
        )));
    }
    Ok(())
}

/// A real Argon2 hash of a random value, used to equalise timing on unknown emails.
/// Verifying against it costs the same as verifying a real user's hash.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHR2YWx1ZQ$\
    Zm9yY29uc3RhbnR0aW1lY29tcGFyaXNvbm9ubHk";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short_passwords() {
        assert!(validate_password("short").is_err());
        assert!(validate_password(&"a".repeat(MIN_PASSWORD_LEN - 1)).is_err());
    }

    #[test]
    fn accepts_sufficiently_long_passwords() {
        assert!(validate_password(&"a".repeat(MIN_PASSWORD_LEN)).is_ok());
        assert!(validate_password("correct horse battery staple").is_ok());
    }

    #[test]
    fn password_length_counts_chars_not_bytes() {
        // 6 multi-byte chars = 18 bytes but only 6 characters — must be rejected.
        assert!(validate_password("ééééé é").is_err());
    }
}
