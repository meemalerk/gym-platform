//! Tenant context and capabilities.
//!
//! See ADR-0014. A person holds a **set** of capacities in a gym, not one role:
//! a trainer at a gym is often also a member there, and an owner often coaches.
//! Effective permission is the union.
//!
//! Tenant context is non-optional in the repository layer (ADR-0004): every
//! tenant-owned query takes `(tenant, id)`, never a bare id.

use serde::{Deserialize, Serialize};

use crate::ids::{GymId, UserId};

/// What a person may do in one gym. Not a "profile" — see ADR-0014.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Capacity {
    Owner,
    Trainer,
    Member,
}

impl Capacity {
    pub const ALL: [Self; 3] = [Self::Owner, Self::Trainer, Self::Member];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Trainer => "trainer",
            Self::Member => "member",
        }
    }

    /// **`admin` and `head_coach` deliberately do not parse** (ADR-0036).
    ///
    /// They are not mapped to their nearest survivor here either, tempting as
    /// that is: `parse` is what turns a database row into authority, and a
    /// function that silently upgraded a stale `admin` row to `Owner` would
    /// grant the one right ADR-0031 reserves — making other owners — to a
    /// string nobody meant to be load-bearing. The migration does the mapping
    /// once, in the open, and the CHECK constraint stops any more appearing.
    ///
    /// Unknown strings returning `None` is the existing contract, and it fails
    /// closed: a capacity that does not parse grants nothing.
    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "owner" => Self::Owner,
            "trainer" => Self::Trainer,
            "member" => Self::Member,
            _ => return None,
        })
    }
}

/// The set of capacities a person holds in one gym.
///
/// **All permission questions are answered here and nowhere else.** Handlers must
/// ask `can_*`, never inspect the raw set — that is what keeps policy from
/// scattering as capacities multiply (the guardrail in ADR-0013).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Capabilities {
    held: Vec<Capacity>,
}

impl Capabilities {
    #[must_use]
    pub fn new(mut held: Vec<Capacity>) -> Self {
        held.sort_unstable();
        held.dedup();
        Self { held }
    }

    #[must_use]
    pub fn holds(&self, capacity: Capacity) -> bool {
        self.held.contains(&capacity)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    #[must_use]
    pub fn held(&self) -> &[Capacity] {
        &self.held
    }

    /// Owner is the superset — anything anybody may do, an owner may.
    #[must_use]
    pub fn is_owner(&self) -> bool {
        self.holds(Capacity::Owner)
    }

    /// Manage the gym itself: standing, settings, billing.
    ///
    /// **Owner only, since ADR-0036 removed `admin`.** Kept as its own method
    /// rather than collapsed into `is_owner` at every call site, because the two
    /// answer different questions — "may you run this place" and "are you one of
    /// the people who own it" — and only the second one gates granting `owner`.
    /// They coincide today; a future rung would separate them again, and this is
    /// the seam it would land on.
    #[must_use]
    pub fn can_manage_gym(&self) -> bool {
        self.is_owner()
    }

    /// Own the exercise catalogue and programme templates.
    ///
    /// **Owner only, since ADR-0036 removed `head_coach`** — the rung ADR-0034
    /// had moved catalogue authoring to. Same reasoning as above for keeping the
    /// method: it is the question the programme lifecycle actually asks.
    #[must_use]
    pub fn can_manage_catalogue(&self) -> bool {
        self.can_manage_gym()
    }

    /// Coach athletes (write programmes, review sessions).
    #[must_use]
    pub fn can_coach(&self) -> bool {
        self.can_manage_catalogue() || self.holds(Capacity::Trainer)
    }

    // ------------------------------------------------- ADR-0024, then ADR-0034
    //
    // "What may a trainer do?" is several questions, not one, and answering it
    // with a single gate is what made Trainer a spectator capacity.
    //
    // ADR-0024 split authoring from publishing and opened AUTHORING to
    // trainers, on the grounds that a draft binds nobody. ADR-0034 moves that
    // line again, for a different reason: the gym decided the catalogue is
    // written by the people who run the gym, and a trainer's job is to APPLY
    // it — pick the right published programme for the client in front of them
    // and put them on it. So a trainer now reads the catalogue and assigns
    // from it, and writes none of it.
    //
    // What that buys, and it is the point: **one library, curated by the gym.**
    // Under ADR-0024 five trainers could each write their own near-duplicate of
    // "Beginner Strength", and since progress is computed per exercise and per
    // version (ADR-0018, ADR-0006), five variants fragment the very data the
    // product exists to accumulate.

    /// Write programme content: create a programme, add weeks, workouts and
    /// prescriptions, and submit a draft for review.
    ///
    /// Head coach and above (ADR-0034). Trainers read the catalogue and assign
    /// from it — see `AssignmentService`, where assigning is *their* right and
    /// no longer a manager's.
    #[must_use]
    pub fn can_author_programs(&self) -> bool {
        self.can_manage_catalogue()
    }

    /// Approve, publish or archive a programme version — the moves that commit
    /// the gym to something. Head coach and above.
    ///
    /// The same set as authoring now. Kept as its own question because the two
    /// are genuinely different acts and the review gate still stands between
    /// them: a draft is not a published version even when one person can do
    /// both, and the second-person approval rule is what makes that real.
    #[must_use]
    pub fn can_publish_programs(&self) -> bool {
        self.can_manage_catalogue()
    }

    /// Propose a movement for the shared catalogue.
    ///
    /// **Still as wide as `can_coach`, and the reason changed.** ADR-0024's
    /// argument was that a trainer who cannot name a movement cannot write a
    /// programme; they no longer write programmes, so that argument is spent.
    /// What remains is better: a trainer on the floor is who notices the
    /// catalogue is missing something, and `proposed` status means naming one
    /// commits the gym to nothing until a manager curates it (ADR-0024's
    /// `can_curate_catalogue`, unchanged).
    #[must_use]
    pub fn can_propose_exercises(&self) -> bool {
        self.can_coach()
    }

    /// Promote a proposed movement into the approved catalogue, or retire one.
    ///
    /// This one stays narrow for a reason that is about **data**, not rank:
    /// progress is computed per `exercise_id` (ADR-0018), so two entries for
    /// the same movement permanently split an athlete's estimated-1RM history
    /// into two half-charts that no later edit can rejoin. Curation is the
    /// only guard against that, and it has to be somebody's job.
    #[must_use]
    pub fn can_curate_catalogue(&self) -> bool {
        self.can_manage_catalogue()
    }

    /// Change what somebody holds in this gym.
    ///
    /// This is the door that invitations used to be (ADR-0031). Owners and
    /// admins, with one extra rule layered on top by `check_standing_change`:
    /// only an owner may hand out or take away `Owner`.
    #[must_use]
    pub fn can_set_capacities(&self) -> bool {
        self.can_manage_gym()
    }
}

/// Why a change of standing was refused.
///
/// Each variant is a distinct thing to say to a person, which is the test for
/// whether it deserves to exist: "you may not do this at all" and "you may do
/// this but not to an owner" need different sentences and different fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StandingError {
    #[error("you may not change what people hold in this gym")]
    NotPermitted,
    #[error("only an owner may grant or remove owner")]
    OwnerIsOwnersToGive,
    #[error("a gym must keep at least one owner")]
    LastOwner,
    #[error("that person is not a member of this gym")]
    NotAMember,
    #[error("a standing must include at least one capacity")]
    Empty,
}

/// May `actor` move somebody from `current` standing to `next`?
///
/// A free function rather than a method because it is a rule about a *pair* of
/// people plus the state of the whole gym, and hanging it off either party's
/// `Capabilities` would put it somewhere nobody thinks to look.
///
/// `other_owners` is how many owners the gym would still have if this person
/// stopped being one. It is passed in rather than queried because the domain
/// does not read from anywhere — but the rule it feeds is the one that stops a
/// gym being locked out of itself, so it is not optional.
pub fn check_standing_change(
    actor: &Capabilities,
    current: &Capabilities,
    next: &Capabilities,
    other_owners: usize,
) -> Result<(), StandingError> {
    check_standing_grant(actor, next)?;

    if current.is_empty() {
        return Err(StandingError::NotAMember);
    }
    if current.is_owner() != next.is_owner() && !actor.is_owner() {
        return Err(StandingError::OwnerIsOwnersToGive);
    }
    if current.is_owner() && !next.is_owner() && other_owners == 0 {
        return Err(StandingError::LastOwner);
    }
    Ok(())
}

/// May `actor` hand out `next` as a **first** standing?
///
/// The half of `check_standing_change` that does not need a "before": the
/// rules about the actor and about the standing itself, with nothing to say
/// about demotion or about the last owner because there is nobody there yet.
///
/// Split out for `GymService::create_staff` — an owner making a trainer's
/// account outright. Reusing the full check there would have meant passing a
/// fake `current`, and a fake argument is how a rule quietly stops applying.
pub fn check_standing_grant(
    actor: &Capabilities,
    next: &Capabilities,
) -> Result<(), StandingError> {
    if !actor.can_set_capacities() {
        return Err(StandingError::NotPermitted);
    }
    // Emptying somebody's standing is "remove them from the gym", which is a
    // different act with different consequences (their history, their
    // coaching relationships) and is deliberately not reachable from here.
    if next.is_empty() {
        return Err(StandingError::Empty);
    }
    // The rule that keeps `admin` from being `owner` with extra steps, and it
    // has to hold on creation too — otherwise an admin makes a second account
    // that is an owner and signs in as it.
    if next.is_owner() && !actor.is_owner() {
        return Err(StandingError::OwnerIsOwnersToGive);
    }
    Ok(())
}

/// Who is acting, and inside which gym.
///
/// Only constructed after authentication AND a live capacity lookup — never from
/// token claims, so a revoked capacity takes effect immediately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantContext {
    pub gym_id: GymId,
    pub actor_id: UserId,
    pub capabilities: Capabilities,
}

impl TenantContext {
    #[must_use]
    pub const fn new(gym_id: GymId, actor_id: UserId, capabilities: Capabilities) -> Self {
        Self {
            gym_id,
            actor_id,
            capabilities,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(list: &[Capacity]) -> Capabilities {
        Capabilities::new(list.to_vec())
    }

    #[test]
    fn capacity_round_trips_through_string() {
        for c in Capacity::ALL {
            assert_eq!(Capacity::parse(c.as_str()), Some(c));
        }
        assert_eq!(Capacity::parse("superuser"), None);
    }

    #[test]
    fn a_person_can_hold_several_capacities_in_one_gym() {
        // The case the old single-role model could not express.
        let c = caps(&[Capacity::Trainer, Capacity::Member]);
        assert!(c.holds(Capacity::Trainer));
        assert!(c.holds(Capacity::Member));
        assert!(c.can_coach());
        assert!(
            !c.can_manage_catalogue(),
            "a trainer does not own the catalogue"
        );
    }

    #[test]
    fn a_trainer_applies_the_catalogue_rather_than_writing_it() {
        // ADR-0034, stated as a test. A trainer's job is to pick the right
        // published programme for the client in front of them; the catalogue
        // itself is the gym's, written by the people who run it.
        let c = caps(&[Capacity::Trainer]);
        assert!(
            !c.can_author_programs(),
            "a trainer does not write the catalogue"
        );
        assert!(
            !c.can_publish_programs(),
            "nor commit the gym to a version of it"
        );
        // Still theirs: a trainer on the floor is who notices a movement is
        // missing, and `proposed` status means naming one binds nothing.
        assert!(
            c.can_propose_exercises(),
            "but may still name a missing movement"
        );
        assert!(
            !c.can_curate_catalogue(),
            "and cannot promote their own proposal"
        );
    }

    #[test]
    fn the_catalogue_belongs_to_whoever_runs_the_gym() {
        // One rung, since ADR-0036 removed admin and head_coach. The loop stays
        // rather than collapsing to a single assert: it is the shape that says
        // "everyone who runs the gym", and a future rung joins the list here.
        #[allow(clippy::single_element_loop)]
        for capacity in [Capacity::Owner] {
            let c = caps(&[capacity]);
            assert!(c.can_author_programs(), "{capacity:?} writes programmes");
            assert!(c.can_publish_programs(), "{capacity:?} publishes them");
        }
    }

    #[test]
    fn an_owner_may_do_both_halves() {
        let c = caps(&[Capacity::Owner]);
        assert!(c.can_author_programs());
        assert!(c.can_publish_programs());
        assert!(c.can_propose_exercises());
        assert!(c.can_curate_catalogue());
    }

    #[test]
    fn the_removed_rungs_grant_nothing_if_they_somehow_appear() {
        // The migration mapped them and the CHECK constraint forbids more, but
        // `parse` is the last line: an `admin` string that survived somewhere
        // must not resolve to authority. Failing closed is the existing
        // contract for unknown capacities, and these are now unknown.
        assert_eq!(Capacity::parse("admin"), None);
        assert_eq!(Capacity::parse("head_coach"), None);
        assert_eq!(Capacity::ALL.len(), 3);
    }

    #[test]
    fn a_member_may_do_neither_half() {
        let c = caps(&[Capacity::Member]);
        assert!(!c.can_author_programs());
        assert!(!c.can_publish_programs());
        assert!(!c.can_propose_exercises());
        assert!(!c.can_curate_catalogue());
    }

    #[test]
    fn owner_implies_everything_below() {
        let c = caps(&[Capacity::Owner]);
        assert!(c.can_manage_gym());
        assert!(c.can_manage_catalogue());
        assert!(c.can_coach());
        assert!(c.can_set_capacities());
    }

    #[test]
    fn member_can_do_none_of_the_staff_things() {
        let c = caps(&[Capacity::Member]);
        assert!(!c.can_manage_gym());
        assert!(!c.can_manage_catalogue());
        assert!(!c.can_coach());
        assert!(!c.can_set_capacities());
    }

    // ------------------------------------------------------ ADR-0031: standing

    fn owner() -> Capabilities {
        caps(&[Capacity::Owner])
    }
    fn member() -> Capabilities {
        caps(&[Capacity::Member])
    }
    fn trainer_and_member() -> Capabilities {
        caps(&[Capacity::Trainer, Capacity::Member])
    }

    #[test]
    fn an_owner_may_promote_a_member_to_trainer() {
        assert_eq!(
            check_standing_change(&owner(), &member(), &trainer_and_member(), 1),
            Ok(())
        );
    }

    #[test]
    fn a_trainer_may_not_change_standing_at_all() {
        // Since ADR-0036 the owner is the only rung that can, so the important
        // assertion is what a TRAINER cannot do — which is what the console was
        // offering them a button for.
        assert_eq!(
            check_standing_change(&trainer_and_member(), &member(), &trainer_and_member(), 1),
            Err(StandingError::NotPermitted)
        );
    }

    #[test]
    fn owner_is_still_owners_to_give_even_though_nobody_else_can_reach_it() {
        /*
            `OwnerIsOwnersToGive` guards a non-owner manager promoting somebody
            to owner. ADR-0036 removed `admin`, which was the only such actor —
            so the branch is now unreachable through the capacity model, and it
            stays anyway.

            Two reasons to keep it rather than delete a rule nothing can trip.
            It is defence in depth against a future rung being added without
            anybody re-deriving this rule; and `check_standing_grant` runs
            first, so a trainer attempting it is refused as `NotPermitted`
            before ever reaching the owner rule. That ordering is what this
            test pins — the guard nobody can reach is behind a guard everybody
            hits.
        */
        assert_eq!(
            check_standing_change(&trainer_and_member(), &member(), &owner(), 1),
            Err(StandingError::NotPermitted),
            "refused for lacking authority at all, before the owner rule"
        );
        // And the owner rule itself still passes for an owner.
        assert_eq!(
            check_standing_change(&owner(), &member(), &owner(), 1),
            Ok(())
        );
    }

    #[test]
    fn the_last_owner_cannot_be_demoted() {
        // The rule that stops a gym locking itself out. `other_owners` is 0.
        assert_eq!(
            check_standing_change(&owner(), &owner(), &member(), 0),
            Err(StandingError::LastOwner)
        );
    }

    #[test]
    fn an_owner_can_step_down_while_another_owner_remains() {
        assert_eq!(
            check_standing_change(&owner(), &owner(), &member(), 1),
            Ok(())
        );
    }

    #[test]
    fn standing_cannot_be_emptied() {
        // Removing somebody from the gym is a different act — it touches their
        // history and their coaching links — and is not reachable from here.
        assert_eq!(
            check_standing_change(&owner(), &member(), &caps(&[]), 1),
            Err(StandingError::Empty)
        );
    }

    #[test]
    fn somebody_who_is_not_a_member_cannot_be_promoted_into_one() {
        // They join first (the open door), then they are promoted. Two steps,
        // because "add a person to the gym" and "change what they hold" fail
        // differently and a caller needs to know which happened.
        assert_eq!(
            check_standing_change(&owner(), &caps(&[]), &trainer_and_member(), 1),
            Err(StandingError::NotAMember)
        );
    }

    #[test]
    fn an_owner_may_create_a_trainer_outright() {
        assert_eq!(
            check_standing_grant(&owner(), &trainer_and_member()),
            Ok(())
        );
    }

    #[test]
    fn a_trainer_may_create_nobody() {
        // The console offered a trainer an "Add staff" button. The server was
        // never going to honour it, and this is the assertion that says so.
        assert_eq!(
            check_standing_grant(&trainer_and_member(), &trainer_and_member()),
            Err(StandingError::NotPermitted)
        );
        assert_eq!(
            check_standing_grant(&trainer_and_member(), &owner()),
            Err(StandingError::NotPermitted)
        );
    }

    #[test]
    fn a_trainer_may_not_create_staff() {
        assert_eq!(
            check_standing_grant(&caps(&[Capacity::Trainer]), &member()),
            Err(StandingError::NotPermitted)
        );
    }

    #[test]
    fn a_created_account_must_hold_something() {
        assert_eq!(
            check_standing_grant(&owner(), &caps(&[])),
            Err(StandingError::Empty)
        );
    }

    #[test]
    fn duplicates_collapse() {
        let c = caps(&[Capacity::Member, Capacity::Member, Capacity::Trainer]);
        assert_eq!(c.held().len(), 2);
    }

    #[test]
    fn no_capacities_means_no_access() {
        let c = Capabilities::default();
        assert!(c.is_empty());
        assert!(!c.can_coach());
        assert!(!c.can_manage_catalogue());
    }
}
