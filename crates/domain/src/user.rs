//! User domain types.

use serde::{Deserialize, Serialize};

use crate::{DomainError, ids::UserId, validated_name};

/// A normalised, syntactically-valid email address.
///
/// Deliberately conservative: we validate shape only. Deliverability is proven by
/// sending mail, not by a regex.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Email(String);

impl Email {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let normalised = raw.trim().to_lowercase();

        let Some((local, domain)) = normalised.split_once('@') else {
            return Err(DomainError::InvalidEmail);
        };

        let shape_ok = !local.is_empty()
            && !domain.is_empty()
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !normalised.contains(char::is_whitespace)
            && normalised.matches('@').count() == 1;

        if !shape_ok {
            return Err(DomainError::InvalidEmail);
        }
        Ok(Self(normalised))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The domain part — safe to log for diagnostics (the local part is not).
    #[must_use]
    pub fn domain(&self) -> &str {
        self.0.split_once('@').map_or("unknown", |(_, d)| d)
    }
}

impl std::fmt::Display for Email {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub email: Email,
    pub display_name: String,
    /// When this account proved it can read its address (ADR-0029).
    ///
    /// `None` means unverified, and that is deliberately **not** a barrier to
    /// signing in: locking people out of a gym they already pay for because a
    /// confirmation email went to spam would be a worse product than one that
    /// tolerates an unconfirmed address. It gates outbound mail we would rather
    /// not send into the void, and it is what an owner looks at before
    /// wondering why someone never replies.
    pub email_verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl User {
    pub fn new(email: Email, display_name: &str) -> Result<Self, DomainError> {
        Ok(Self {
            id: UserId::new(),
            email,
            display_name: validated_name("display_name", display_name, 50)?,
            // Nobody has proved anything yet.
            email_verified_at: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("user@example.com")]
    #[case("  User@Example.COM  ")]
    #[case("a.b+tag@sub.example.co.uk")]
    fn accepts_valid_emails(#[case] raw: &str) {
        assert!(Email::parse(raw).is_ok(), "should accept {raw}");
    }

    #[rstest]
    #[case("")]
    #[case("no-at-sign")]
    #[case("@example.com")]
    #[case("user@")]
    #[case("user@nodot")]
    #[case("user@.example.com")]
    #[case("user@example.com.")]
    #[case("two@@example.com")]
    #[case("has space@example.com")]
    fn rejects_invalid_emails(#[case] raw: &str) {
        assert_eq!(
            Email::parse(raw),
            Err(DomainError::InvalidEmail),
            "should reject {raw:?}"
        );
    }

    #[test]
    fn normalises_case_and_whitespace() {
        let email = Email::parse("  Foo.Bar@Example.COM ").unwrap();
        assert_eq!(email.as_str(), "foo.bar@example.com");
    }

    #[test]
    fn exposes_domain_for_safe_logging() {
        let email = Email::parse("secret@example.com").unwrap();
        assert_eq!(email.domain(), "example.com");
    }

    #[test]
    fn rejects_overlong_display_name() {
        let email = Email::parse("a@b.com").unwrap();
        let long = "x".repeat(51);
        assert!(matches!(
            User::new(email, &long),
            Err(DomainError::TooLong { max: 50, .. })
        ));
    }
}
