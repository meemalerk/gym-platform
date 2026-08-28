//! Entitlements — "what may this actor access right now", decoupled from where
//! the money came from.
//!
//! The shape [subscriptions-billing.md] settles:
//!
//! ```text
//! payment event (card / IAP / cash / admin comp) → reconcile → ENTITLEMENT
//!                                                                  ↓
//!                                        feature gating reads this, never the
//!                                        payment provider's state directly
//! ```
//!
//! Two properties make this worth having before billing is switched on.
//!
//! **Entitlements are resolved, not stored.** A row saying "entitled until the
//! 30th" is wrong on the 31st unless something rewrites it nightly — the same
//! second-source-of-truth this codebase refuses for "overdue". So an
//! `EntitlementSet` is computed on read from subscriptions that already exist.
//!
//! **Absence of billing is not absence of entitlement.** A gym that has not set
//! up plans is not a gym whose members may not train; it is a gym that does not
//! bill through this platform. Getting that backwards would break training for
//! every gym that uses the app without its billing, which is most of them on
//! day one — so it is a rule with a name (`Source::NotBilled`) and a test,
//! rather than an implicit fallthrough.

use serde::{Deserialize, Serialize};

/// A capability a subject may hold. Deliberately small: a feature earns a
/// place here when something in the product genuinely turns on it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    /// Train here: start sessions and log work.
    GymAccess,
    /// Be assigned a programme and coached against it.
    CoachedProgramming,
    /// Draw on a class pack. Modelled now so plans can grant it; the
    /// balance-tracking it implies is not built.
    ClassCredits,
}

impl Feature {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GymAccess => "gym_access",
            Self::CoachedProgramming => "coached_programming",
            Self::ClassCredits => "class_credits",
        }
    }

    /// Unknown strings are dropped rather than erroring: the database CHECK is
    /// the guard against typos, and a reader should not fail on a feature a
    /// newer version of the app added.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "gym_access" => Some(Self::GymAccess),
            "coached_programming" => Some(Self::CoachedProgramming),
            "class_credits" => Some(Self::ClassCredits),
            _ => None,
        }
    }

    pub const ALL: [Feature; 3] = [
        Self::GymAccess,
        Self::CoachedProgramming,
        Self::ClassCredits,
    ];
}

/// Why a subject holds what it holds. Carried so a screen can explain access
/// instead of asserting it — the same rule the recommender follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// Granted by an active subscription to a plan that confers it.
    Subscription { plan_name: String },
    /// This gym does not bill through the platform, so nothing is withheld.
    /// Not a fallback that happens to be permissive — a stated position.
    NotBilled,
    /// The platform→gym SaaS tier. No such subscription exists yet, so every
    /// gym resolves to the full set; when tiers arrive this is the one place
    /// that changes.
    PlatformTier,
}

/// What one subject may access, with the reason for each.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntitlementSet {
    held: Vec<(Feature, Source)>,
}

impl EntitlementSet {
    #[must_use]
    pub fn new(mut held: Vec<(Feature, Source)>) -> Self {
        // Sorted into ladder order (the enum's own order, not alphabetical) and
        // de-duplicated by feature, so two plans granting the same thing
        // produce one entitlement. The sort is stable and dedup keeps the
        // first, so the surviving source is the caller's first — deterministic
        // rather than whatever order the query returned.
        held.sort_by_key(|held| held.0);
        held.dedup_by(|a, b| a.0 == b.0);
        Self { held }
    }

    /// Everything, for the stated reason. Used where the platform does not yet
    /// bill the subject at all.
    #[must_use]
    pub fn everything(source: Source) -> Self {
        Self::new(Feature::ALL.iter().map(|f| (*f, source.clone())).collect())
    }

    #[must_use]
    pub fn allows(&self, feature: Feature) -> bool {
        self.held.iter().any(|(f, _)| *f == feature)
    }

    /// Why this feature is held, if it is. The reason a screen can print.
    #[must_use]
    pub fn because(&self, feature: Feature) -> Option<&Source> {
        self.held
            .iter()
            .find(|(f, _)| *f == feature)
            .map(|(_, s)| s)
    }

    #[must_use]
    pub fn held(&self) -> &[(Feature, Source)] {
        &self.held
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feature_round_trips_through_its_wire_name() {
        for feature in Feature::ALL {
            assert_eq!(Feature::parse(feature.as_str()), Some(feature));
        }
    }

    #[test]
    fn an_unknown_feature_is_ignored_not_an_error() {
        // A newer version of the app may grant something this build has never
        // heard of; refusing to read the row would be worse than skipping it.
        assert_eq!(Feature::parse("teleportation"), None);
    }

    #[test]
    fn two_plans_granting_the_same_feature_yield_one_entitlement() {
        let set = EntitlementSet::new(vec![
            (
                Feature::GymAccess,
                Source::Subscription {
                    plan_name: "Open Gym".into(),
                },
            ),
            (
                Feature::GymAccess,
                Source::Subscription {
                    plan_name: "Coaching".into(),
                },
            ),
        ]);
        assert_eq!(set.held().len(), 1);
        assert!(set.allows(Feature::GymAccess));
    }

    #[test]
    fn holding_one_feature_does_not_imply_another() {
        let set = EntitlementSet::new(vec![(Feature::GymAccess, Source::NotBilled)]);
        assert!(set.allows(Feature::GymAccess));
        assert!(!set.allows(Feature::CoachedProgramming));
    }

    #[test]
    fn every_entitlement_can_say_why_it_is_held() {
        let set = EntitlementSet::new(vec![(
            Feature::CoachedProgramming,
            Source::Subscription {
                plan_name: "Coaching".into(),
            },
        )]);
        assert_eq!(
            set.because(Feature::CoachedProgramming),
            Some(&Source::Subscription {
                plan_name: "Coaching".into()
            })
        );
        assert_eq!(set.because(Feature::GymAccess), None);
    }

    #[test]
    fn features_come_back_in_ladder_order_however_they_went_in() {
        // Callers assemble these from whatever order the database returned, and
        // the order is user-visible ("Gym access + coached programming"). Two
        // gyms selling the same thing must describe it the same way.
        let set = EntitlementSet::new(vec![
            (Feature::ClassCredits, Source::NotBilled),
            (Feature::GymAccess, Source::NotBilled),
            (Feature::CoachedProgramming, Source::NotBilled),
        ]);
        let order: Vec<Feature> = set.held().iter().map(|(f, _)| *f).collect();
        assert_eq!(
            order,
            vec![
                Feature::GymAccess,
                Feature::CoachedProgramming,
                Feature::ClassCredits
            ]
        );
    }

    #[test]
    fn an_unbilled_gym_withholds_nothing() {
        let set = EntitlementSet::everything(Source::NotBilled);
        for feature in Feature::ALL {
            assert!(
                set.allows(feature),
                "a gym that does not bill through the platform must not lose {feature:?}"
            );
        }
    }

    #[test]
    fn an_empty_set_allows_nothing() {
        let set = EntitlementSet::default();
        assert!(set.is_empty());
        assert!(!set.allows(Feature::GymAccess));
    }
}
