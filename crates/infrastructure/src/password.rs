//! Argon2id password hashing.
//!
//! Pinned to argon2 0.5 (stable). 0.6 was still a release candidate (0.6.0-rc.8)
//! as of 2026-07 — we do not ship auth on a pre-release.

use argon2::{
    Argon2,
    password_hash::{
        PasswordHash, PasswordHasher as _, PasswordVerifier, SaltString, rand_core::OsRng,
    },
};
use gym_application::{ApplicationError, ApplicationResult, ports::PasswordHasher};

#[derive(Debug, Clone, Default)]
pub struct Argon2PasswordHasher;

impl PasswordHasher for Argon2PasswordHasher {
    fn hash(&self, plaintext: &str) -> ApplicationResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(plaintext.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| ApplicationError::Internal(format!("hash failure: {e}").into()))
    }

    fn verify(&self, plaintext: &str, hash: &str) -> ApplicationResult<bool> {
        // A malformed stored hash is an internal problem, not a wrong password.
        let parsed = PasswordHash::new(hash)
            .map_err(|e| ApplicationError::Internal(format!("malformed hash: {e}").into()))?;

        Ok(Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies_correct_password() {
        let hasher = Argon2PasswordHasher;
        let hash = hasher.hash("correct horse battery staple").unwrap();

        assert!(
            hasher
                .verify("correct horse battery staple", &hash)
                .unwrap()
        );
    }

    #[test]
    fn rejects_wrong_password() {
        let hasher = Argon2PasswordHasher;
        let hash = hasher.hash("correct horse battery staple").unwrap();

        assert!(
            !hasher
                .verify("Correct horse battery staple", &hash)
                .unwrap()
        );
        assert!(!hasher.verify("", &hash).unwrap());
    }

    #[test]
    fn same_password_yields_different_hashes() {
        let hasher = Argon2PasswordHasher;
        let a = hasher.hash("same").unwrap();
        let b = hasher.hash("same").unwrap();

        assert_ne!(a, b, "salt must make hashes unique");
        assert!(hasher.verify("same", &a).unwrap());
        assert!(hasher.verify("same", &b).unwrap());
    }

    #[test]
    fn malformed_hash_is_an_internal_error_not_a_false_negative() {
        let hasher = Argon2PasswordHasher;
        assert!(hasher.verify("x", "not-a-phc-string").is_err());
    }
}
