//! Entry-pass tokens: short-lived, signed, never stored.
//!
//! Deliberately a SEPARATE token shape from `JwtTokenIssuer`'s access tokens,
//! signed with the same secret but carrying a `purpose` claim no other
//! verifier checks for — an entry pass decoded by the wrong code path is
//! simply rejected, so a bug that mixed the two up fails closed, not open.

use chrono::Utc;
use gym_application::{ApplicationError, ApplicationResult, ports::EntryPassIssuer};
use gym_domain::{GymId, UserId};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

/// The one value `purpose` may hold. A constant, not a type, because it never
/// needs to vary — the whole point is that it is the same fixed string every
/// time, so a token minted for anything else can never accidentally match it.
const PURPOSE: &str = "gym_entry";

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// Subject — the member's user id.
    sub: String,
    /// Which gym this pass is good for. An access token has no equivalent —
    /// this is the one claim that makes an entry pass gym-scoped rather than
    /// account-scoped, matching every other tenant-owned thing in the system.
    gym_id: String,
    purpose: String,
    iat: i64,
    exp: i64,
}

pub struct JwtEntryPassIssuer {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
}

impl JwtEntryPassIssuer {
    #[must_use]
    pub fn new(secret: &[u8]) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        // A pass lives for seconds, not minutes — the usual 60s leeway would
        // let an already-stale pass keep working for as long as it was valid
        // in the first place. 2s covers clock skew without meaningfully
        // extending the window a screenshot stays useful.
        validation.leeway = 2;
        validation.required_spec_claims = ["exp", "sub"].into_iter().map(String::from).collect();

        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            validation,
        }
    }
}

impl EntryPassIssuer for JwtEntryPassIssuer {
    fn issue(&self, member: UserId, gym: GymId, ttl_seconds: i64) -> ApplicationResult<String> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: member.to_string(),
            gym_id: gym.to_string(),
            purpose: PURPOSE.to_owned(),
            iat: now,
            exp: now + ttl_seconds,
        };

        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .map_err(ApplicationError::internal)
    }

    fn verify(&self, token: &str) -> ApplicationResult<(UserId, GymId)> {
        let invalid = || ApplicationError::Forbidden;

        let data =
            decode::<Claims>(token, &self.decoding, &self.validation).map_err(|_| invalid())?;

        if data.claims.purpose != PURPOSE {
            return Err(invalid());
        }

        let member = data
            .claims
            .sub
            .parse::<uuid::Uuid>()
            .map_err(|_| invalid())?;
        let gym = data
            .claims
            .gym_id
            .parse::<uuid::Uuid>()
            .map_err(|_| invalid())?;

        Ok((UserId::from(member), GymId::from(gym)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issuer() -> JwtEntryPassIssuer {
        JwtEntryPassIssuer::new(b"test-secret-value-for-unit-tests")
    }

    #[test]
    fn round_trips_a_pass() {
        let issuer = issuer();
        let member = UserId::new();
        let gym = GymId::new();

        let token = issuer.issue(member, gym, 90).unwrap();
        let (verified_member, verified_gym) = issuer.verify(&token).unwrap();

        assert_eq!(verified_member, member);
        assert_eq!(verified_gym, gym);
    }

    #[test]
    fn rejects_an_expired_pass() {
        let issuer = issuer();
        let token = issuer.issue(UserId::new(), GymId::new(), -60).unwrap();
        assert!(issuer.verify(&token).is_err());
    }

    #[test]
    fn rejects_a_pass_signed_with_another_key() {
        let token = issuer().issue(UserId::new(), GymId::new(), 90).unwrap();
        let attacker = JwtEntryPassIssuer::new(b"a-completely-different-secret-key");
        assert!(attacker.verify(&token).is_err());
    }

    #[test]
    fn rejects_an_access_token_wearing_this_codes_clothes() {
        // Same secret, same algorithm, but minted without the `purpose` claim
        // this verifier requires — proves the claim is actually load-bearing,
        // not just documentation.
        #[derive(Serialize)]
        struct AccessLikeClaims {
            sub: String,
            iat: i64,
            exp: i64,
        }
        let now = Utc::now().timestamp();
        let claims = AccessLikeClaims {
            sub: UserId::new().to_string(),
            iat: now,
            exp: now + 900,
        };
        let secret = b"test-secret-value-for-unit-tests";
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret),
        )
        .unwrap();

        assert!(issuer().verify(&token).is_err());
    }

    #[test]
    fn rejects_garbage_tokens() {
        for bad in ["", "not-a-jwt", "a.b.c"] {
            assert!(issuer().verify(bad).is_err(), "should reject {bad:?}");
        }
    }
}
