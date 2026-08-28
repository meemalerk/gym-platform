//! Token issuing: short-lived HS256 access JWTs and opaque rotating refresh tokens.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use chrono::Utc;
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{AccessToken, OpaqueToken, TokenIssuer},
};
use gym_domain::UserId;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Refresh token entropy. 32 bytes ≫ any feasible guessing attack.
const REFRESH_TOKEN_BYTES: usize = 32;

/// Access-token claims. Deliberately minimal — a JWT payload is base64, not
/// encrypted, so it carries no personal data. Roles are resolved from the
/// database per request, never trusted from the token.
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// Subject — the user id.
    sub: String,
    /// Issued at (unix seconds).
    iat: i64,
    /// Expiry (unix seconds).
    exp: i64,
}

pub struct JwtTokenIssuer {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
    access_ttl_seconds: i64,
    refresh_ttl_days: i64,
}

impl JwtTokenIssuer {
    #[must_use]
    pub fn new(secret: &[u8], access_ttl_seconds: i64, refresh_ttl_days: i64) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        // jsonwebtoken defaults to 60s of leeway, which silently extends every
        // access token's life. 5s is enough for clock skew between our own hosts.
        validation.leeway = 5;
        // No audience/issuer in this single-service deployment; revisit if we
        // ever federate tokens across services.
        validation.required_spec_claims = ["exp", "sub"].into_iter().map(String::from).collect();

        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            validation,
            access_ttl_seconds,
            refresh_ttl_days,
        }
    }
}

impl TokenIssuer for JwtTokenIssuer {
    fn issue_access(&self, user: UserId) -> ApplicationResult<AccessToken> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: user.to_string(),
            iat: now,
            exp: now + self.access_ttl_seconds,
        };

        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .map_err(|e| ApplicationError::Internal(format!("token encode failed: {e}").into()))?;

        Ok(AccessToken {
            token,
            expires_in_seconds: self.access_ttl_seconds,
        })
    }

    fn verify_access(&self, token: &str) -> ApplicationResult<UserId> {
        // Any verification failure (bad signature, expired, malformed) is the same
        // outcome to the caller: not authenticated.
        let data = decode::<Claims>(token, &self.decoding, &self.validation)
            .map_err(|_| ApplicationError::Unauthenticated)?;

        let id = data
            .claims
            .sub
            .parse::<uuid::Uuid>()
            .map_err(|_| ApplicationError::Unauthenticated)?;

        Ok(UserId::from(id))
    }

    fn generate_opaque(&self) -> ApplicationResult<OpaqueToken> {
        let mut bytes = [0u8; REFRESH_TOKEN_BYTES];
        OsRng.fill_bytes(&mut bytes);

        // Hex rather than base64: URL/JSON-safe with no padding or dependency.
        let raw = bytes.iter().fold(
            String::with_capacity(REFRESH_TOKEN_BYTES * 2),
            |mut acc, b| {
                use std::fmt::Write as _;
                let _ = write!(acc, "{b:02x}");
                acc
            },
        );

        let hash = self.hash_opaque(&raw);
        Ok(OpaqueToken { raw, hash })
    }

    fn hash_opaque(&self, raw: &str) -> Vec<u8> {
        // SHA-256 is right here (not Argon2): the token is already 256 bits of
        // entropy, so there is nothing to brute-force and we want lookups fast.
        Sha256::digest(raw.as_bytes()).to_vec()
    }

    fn refresh_ttl_days(&self) -> i64 {
        self.refresh_ttl_days
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issuer() -> JwtTokenIssuer {
        JwtTokenIssuer::new(b"test-secret-value-for-unit-tests", 900, 30)
    }

    #[test]
    fn round_trips_an_access_token() {
        let issuer = issuer();
        let user = UserId::new();

        let token = issuer.issue_access(user).unwrap();
        assert_eq!(issuer.verify_access(&token.token).unwrap(), user);
        assert_eq!(token.expires_in_seconds, 900);
    }

    #[test]
    fn rejects_a_token_signed_with_another_key() {
        let token = issuer().issue_access(UserId::new()).unwrap().token;
        let attacker = JwtTokenIssuer::new(b"a-completely-different-secret-key", 900, 30);

        assert!(attacker.verify_access(&token).is_err());
    }

    #[test]
    fn rejects_expired_tokens() {
        // Negative TTL => already expired on issue. Must exceed the 5s leeway.
        let expired = JwtTokenIssuer::new(b"test-secret-value-for-unit-tests", -600, 30);
        let token = expired.issue_access(UserId::new()).unwrap().token;

        assert!(issuer().verify_access(&token).is_err());
    }

    #[test]
    fn leeway_is_bounded() {
        // A token expired 30s ago must NOT be accepted — guards against the
        // library's permissive 60s default creeping back in.
        let expired = JwtTokenIssuer::new(b"test-secret-value-for-unit-tests", -30, 30);
        let token = expired.issue_access(UserId::new()).unwrap().token;

        assert!(issuer().verify_access(&token).is_err());
    }

    #[test]
    fn rejects_garbage_tokens() {
        for bad in ["", "not-a-jwt", "a.b.c"] {
            assert!(
                issuer().verify_access(bad).is_err(),
                "should reject {bad:?}"
            );
        }
    }

    #[test]
    fn refresh_tokens_are_unique_and_hashed_consistently() {
        let issuer = issuer();
        let a = issuer.generate_opaque().unwrap();
        let b = issuer.generate_opaque().unwrap();

        assert_ne!(a.raw, b.raw, "tokens must not repeat");
        assert_eq!(a.raw.len(), REFRESH_TOKEN_BYTES * 2, "hex-encoded length");
        assert_eq!(a.hash, issuer.hash_opaque(&a.raw), "hash must be stable");
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn stored_hash_does_not_reveal_the_token() {
        let issuer = issuer();
        let token = issuer.generate_opaque().unwrap();
        assert!(!token.hash.is_empty());
        assert_ne!(token.hash.as_slice(), token.raw.as_bytes());
    }
}
