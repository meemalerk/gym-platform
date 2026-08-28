//! Gym organisation — the tenant root.

use serde::{Deserialize, Serialize};

use crate::{DomainError, ids::GymId, validated_name};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Gym {
    pub id: GymId,
    pub name: String,
    /// URL-safe identifier, unique platform-wide.
    pub slug: String,
    /// A solo user's own workspace rather than a commercial gym.
    ///
    /// Same tenancy model either way — this only changes how the UI frames it,
    /// which is why solo users need no separate code path (ADR-0014).
    pub is_personal: bool,
    /// Whether anyone signed in may join as a plain member, or an invitation is
    /// the only way in (ADR-0026).
    ///
    /// **Never grants staff standing**, whatever it is set to — the open door
    /// admits members and nothing else, so flipping this switch can never be a
    /// privilege escalation. Default false: a gym that has not thought about
    /// this should not be quietly accepting strangers.
    pub open_registration: bool,
}

impl Gym {
    /// Create a commercial gym, deriving a unique slug from its name.
    ///
    /// The slug carries a short suffix from the gym's own id so two gyms called
    /// "CrossFit Central" don't collide — we'd rather have a stable unique slug
    /// than fail the signup or loop on retries.
    pub fn new(name: &str) -> Result<Self, DomainError> {
        Self::new_with_kind(name, false)
    }

    pub fn new_with_kind(name: &str, is_personal: bool) -> Result<Self, DomainError> {
        let id = GymId::new();
        let name = validated_name("name", name, 120)?;
        let slug = derive_slug(&name, id);
        Ok(Self {
            id,
            name,
            slug,
            is_personal,
            // Closed until somebody decides otherwise.
            open_registration: false,
        })
    }
}

/// Slugify to match the DB constraint `^[a-z0-9][a-z0-9-]{1,62}$`.
fn derive_slug(name: &str, id: GymId) -> String {
    let mut base = String::with_capacity(name.len());
    let mut last_dash = false;

    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            base.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !base.is_empty() {
            base.push('-');
            last_dash = true;
        }
    }
    while base.ends_with('-') {
        base.pop();
    }

    // Non-ASCII names (e.g. "北京健身") can slugify to nothing — fall back.
    if base.is_empty() {
        base.push_str("gym");
    }
    base.truncate(48);
    while base.ends_with('-') {
        base.pop();
    }

    // Take the TAIL of the uuid, not the head. UUIDv7 begins with a 48-bit
    // millisecond timestamp, so two gyms created in the same millisecond share a
    // leading prefix — the entropy lives in the trailing random bits.
    let hex = id.into_uuid().simple().to_string();
    let suffix = &hex[hex.len() - 8..];
    format!("{base}-{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slug_of(name: &str) -> String {
        Gym::new(name).unwrap().slug
    }

    /// Mirrors the CHECK constraint in the migration.
    fn matches_db_constraint(slug: &str) -> bool {
        let bytes = slug.as_bytes();
        (2..=63).contains(&slug.len())
            && bytes[0].is_ascii_lowercase() | bytes[0].is_ascii_digit()
            && slug
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }

    #[test]
    fn slugifies_a_normal_name() {
        let slug = slug_of("CrossFit Central");
        assert!(slug.starts_with("crossfit-central-"), "got {slug}");
        assert!(matches_db_constraint(&slug));
    }

    #[test]
    fn collapses_punctuation_and_trims_dashes() {
        let slug = slug_of("  Bob's   Gym & Fitness!!  ");
        assert!(slug.starts_with("bob-s-gym-fitness-"), "got {slug}");
        assert!(matches_db_constraint(&slug));
    }

    #[test]
    fn non_ascii_name_still_yields_a_valid_slug() {
        let slug = slug_of("北京健身");
        assert!(slug.starts_with("gym-"), "got {slug}");
        assert!(matches_db_constraint(&slug));
    }

    #[test]
    fn very_long_name_stays_within_db_limit() {
        let slug = slug_of(&"a".repeat(120));
        assert!(
            matches_db_constraint(&slug),
            "got {slug} (len {})",
            slug.len()
        );
    }

    /// Regression: the suffix originally used the *leading* uuid chars, which for
    /// UUIDv7 are a millisecond timestamp — so gyms created in the same
    /// millisecond collided. The suffix must come from the random tail.
    #[test]
    fn two_gyms_with_the_same_name_get_different_slugs() {
        let slugs: std::collections::HashSet<_> = (0..50).map(|_| slug_of("Iron Temple")).collect();
        assert_eq!(slugs.len(), 50, "slugs must be unique even in a tight loop");
    }

    #[test]
    fn rejects_blank_name() {
        assert_eq!(Gym::new("   "), Err(DomainError::Empty { field: "name" }));
    }
}
