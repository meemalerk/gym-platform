//! Adapters for password reset, email verification, login throttling and mail
//! (ADR-0029).
//!
//! Three tables and one seam. Nothing here is tenant-scoped: an account, its
//! credentials and the mail sent to it all exist before any gym is involved,
//! and a password reset has to work for someone who belongs to no gym at all.
//! That is the same reason `users` and `sessions` sit outside RLS — the service
//! layer is the only wall, which is why every query below is keyed on a user id
//! or a token hash and never on "whatever the caller asked for".

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{
        AttemptCounts, AuthToken, AuthTokenPurpose, AuthTokenRepository, EmailMessage, EmailSender,
        LoginAttemptRepository,
    },
};
use gym_domain::UserId;
use uuid::Uuid;

use crate::DbPool;

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

// ------------------------------------------------------- single-use secrets

#[derive(Debug, Clone)]
pub struct PgAuthTokenRepository {
    pool: DbPool,
}

impl PgAuthTokenRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

fn purpose_from(raw: &str) -> ApplicationResult<AuthTokenPurpose> {
    match raw {
        "password_reset" => Ok(AuthTokenPurpose::PasswordReset),
        "email_verification" => Ok(AuthTokenPurpose::EmailVerification),
        other => Err(ApplicationError::Internal(
            format!("unknown auth token purpose '{other}'").into(),
        )),
    }
}

#[async_trait]
impl AuthTokenRepository for PgAuthTokenRepository {
    async fn issue(
        &self,
        user: UserId,
        purpose: AuthTokenPurpose,
        email: &str,
        token_hash: Vec<u8>,
        expires_at: DateTime<Utc>,
    ) -> ApplicationResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO auth_tokens (id, user_id, purpose, token_hash, email, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            Uuid::now_v7(),
            user.into_uuid(),
            purpose.as_str(),
            token_hash,
            email,
            expires_at,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)
        .map(|_| ())
    }

    async fn find_by_hash(&self, token_hash: &[u8]) -> ApplicationResult<Option<AuthToken>> {
        let row = sqlx::query!(
            r#"
            SELECT id, user_id, purpose, email, expires_at, used_at
            FROM auth_tokens
            WHERE token_hash = $1
            "#,
            token_hash,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        row.map(|r| {
            Ok(AuthToken {
                id: r.id,
                user_id: UserId::from(r.user_id),
                purpose: purpose_from(&r.purpose)?,
                email: r.email,
                expires_at: r.expires_at,
                used_at: r.used_at,
            })
        })
        .transpose()
    }

    async fn consume(&self, id: Uuid, now: DateTime<Utc>) -> ApplicationResult<bool> {
        // Conditional UPDATE, and the caller is told whether THIS call won.
        //
        // The same compare-and-swap discipline as refresh rotation, and for the
        // same reason: a read-then-write lets two concurrent redemptions both
        // pass the "is it unused?" check and both act. For a reset link that
        // means two passwords set from one email.
        let updated = sqlx::query!(
            r#"UPDATE auth_tokens SET used_at = $2 WHERE id = $1 AND used_at IS NULL"#,
            id,
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(updated.rows_affected() == 1)
    }

    async fn invalidate_all(
        &self,
        user: UserId,
        purpose: AuthTokenPurpose,
        now: DateTime<Utc>,
    ) -> ApplicationResult<()> {
        sqlx::query!(
            r#"
            UPDATE auth_tokens
               SET used_at = $3
             WHERE user_id = $1 AND purpose = $2 AND used_at IS NULL
            "#,
            user.into_uuid(),
            purpose.as_str(),
            now,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)
        .map(|_| ())
    }
}

// ------------------------------------------------------------ login throttle

#[derive(Debug, Clone)]
pub struct PgLoginAttemptRepository {
    pool: DbPool,
}

impl PgLoginAttemptRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LoginAttemptRepository for PgLoginAttemptRepository {
    async fn record(
        &self,
        email: &str,
        ip: Option<&str>,
        succeeded: bool,
    ) -> ApplicationResult<()> {
        sqlx::query!(
            r#"INSERT INTO login_attempts (email, ip, succeeded) VALUES ($1, $2, $3)"#,
            email,
            ip,
            succeeded,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)
        .map(|_| ())
    }

    async fn recent_failures(
        &self,
        email: &str,
        ip: Option<&str>,
        since: DateTime<Utc>,
    ) -> ApplicationResult<AttemptCounts> {
        // One round trip for both counters. Two queries would be two chances
        // for the window to shift between them, and this runs on every login.
        let row = sqlx::query!(
            r#"
            SELECT
              count(*) FILTER (WHERE email = $1)                        AS "by_email!",
              count(*) FILTER (WHERE $2::text IS NOT NULL AND ip = $2)  AS "by_ip!"
            FROM login_attempts
            WHERE NOT succeeded AND attempted_at >= $3
            "#,
            email,
            ip,
            since,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(AttemptCounts {
            by_email: row.by_email,
            by_ip: row.by_ip,
        })
    }
}

// ---------------------------------------------------------------- mail

/// Writes every message to the database and the log, and sends nothing.
///
/// The honest state of outbound mail: there is no SMTP adapter, because there
/// are no credentials and inventing a provider integration nobody has signed
/// up for would be worse than saying so. What this buys, which a silent no-op
/// would not:
///
///   * the demo can show the mail it *would* have sent, including the reset
///     link, without configuring anything;
///   * the verification suite can read the link it is supposed to follow, so
///     the whole reset flow is testable end to end;
///   * a support question — "did the reset email go out?" — has an answer.
///
/// The cost is that a live reset link sits in a table for as long as the row
/// does. Acceptable only because those tokens are single-use and expire in an
/// hour; a real deployment wants a retention job, and there isn't one yet.
pub struct RecordingEmailSender {
    pool: DbPool,
}

impl RecordingEmailSender {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EmailSender for RecordingEmailSender {
    async fn send(&self, message: EmailMessage) -> ApplicationResult<()> {
        // The subject and recipient, never the body: a body can contain a live
        // credential and logs get shipped somewhere.
        tracing::info!(
            to = %message.to,
            kind = %message.kind,
            subject = %message.subject,
            "email recorded (no provider configured — nothing was actually sent)"
        );

        sqlx::query!(
            r#"
            INSERT INTO sent_emails (id, to_email, subject, body, kind)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            Uuid::now_v7(),
            message.to,
            message.subject,
            message.body,
            message.kind,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)
        .map(|_| ())
    }
}
