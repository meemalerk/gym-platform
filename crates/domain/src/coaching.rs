//! Coach–athlete relationships — who coaches whom.
//!
//! The platform already knows who belongs to a gym and what capacities they hold
//! ([ADR-0014](../../../docs/adr/0014-identity-capacities-and-profiles.md)), but
//! capacities are **gym-wide**: `can_coach` answers "may this person coach here at
//! all", never "may this person coach *Sara*". Almost everything personal — an
//! assigned programme, a measurement, a goal, nutrition guidance — needs the
//! second question, and this is the record that answers it.
//!
//! Two decisions worth understanding before changing anything:
//!
//! **Relationships end; they are not deleted.** A coach who stops working with an
//! athlete leaves behind programmes they wrote and notes they made. Deleting the
//! link would orphan that history or, worse, make it look like someone else's
//! work. So `status` moves to `Ended` and `ended_at` records when.
//!
//! **The relationship is gym-scoped.** The same two people may work together in
//! one gym and not in another — a trainer can be affiliated with several gyms. A
//! global relationship would leak one gym's roster into another's.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    Capabilities,
    ids::{CoachRelationshipId, GymId, UserId},
};

/// Whether a coaching relationship is current.
///
/// An enum carrying its own evidence, for the same reason as `VersionStatus`:
/// "ended, but with no end date" is not a state anyone should have to handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RelationshipStatus {
    Active,
    Ended {
        ended_at: DateTime<Utc>,
        /// Who ended it. Not always the coach — a head coach may reassign.
        ended_by: UserId,
    },
}

impl RelationshipStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Ended { .. } => "ended",
        }
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoachingError {
    #[error("only gym managers and head coaches may assign coaches")]
    NotPermitted,

    #[error("this coaching relationship has already ended")]
    AlreadyEnded,

    #[error("the coach and the athlete must hold the right capacities in this gym")]
    UnsuitableParticipants,
}

/// The link that makes someone the coach of a specific member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CoachRelationship {
    pub id: CoachRelationshipId,
    pub gym_id: GymId,
    pub coach_id: UserId,
    pub athlete_id: UserId,
    pub status: RelationshipStatus,
    pub started_at: DateTime<Utc>,
    /// Who created the pairing — often a head coach rather than either party.
    pub created_by: UserId,
}

impl CoachRelationship {
    /// Pair a coach with an athlete.
    ///
    /// Both participants' capacities are checked, because a "coach" who cannot
    /// coach and an "athlete" who is not a member are both nonsense that would
    /// otherwise sit in the database looking valid.
    pub fn new(
        gym_id: GymId,
        coach_id: UserId,
        coach_capabilities: &Capabilities,
        athlete_id: UserId,
        athlete_capabilities: &Capabilities,
        created_by: UserId,
        now: DateTime<Utc>,
    ) -> Result<Self, CoachingError> {
        if !coach_capabilities.can_coach() {
            return Err(CoachingError::UnsuitableParticipants);
        }
        // The athlete must actually belong to the gym. Holding *any* capacity is
        // enough — a trainer can also be somebody else's client, which ADR-0014
        // explicitly allows and this must not accidentally forbid.
        if athlete_capabilities.is_empty() {
            return Err(CoachingError::UnsuitableParticipants);
        }

        Ok(Self {
            id: CoachRelationshipId::new(),
            gym_id,
            coach_id,
            athlete_id,
            status: RelationshipStatus::Active,
            started_at: now,
            created_by,
        })
    }

    /// May `actor` create or end coaching relationships in this gym?
    ///
    /// Head coaches and above. A trainer cannot assign themselves clients —
    /// otherwise anyone with `can_coach` could grant themselves visibility of any
    /// member's data by creating the link, which turns a coaching relationship
    /// into a self-service permission grant.
    #[must_use]
    pub fn may_assign(actor: &Capabilities) -> bool {
        actor.can_manage_catalogue()
    }

    /// End the relationship. Idempotence is deliberately *not* offered: ending an
    /// already-ended relationship usually means the caller is confused about
    /// which one they are looking at.
    pub fn end(&mut self, ended_by: UserId, now: DateTime<Utc>) -> Result<(), CoachingError> {
        if !self.status.is_active() {
            return Err(CoachingError::AlreadyEnded);
        }
        self.status = RelationshipStatus::Ended {
            ended_at: now,
            ended_by,
        };
        Ok(())
    }

    /// Does this relationship let `actor` see `athlete`'s personal data **right now**?
    #[must_use]
    pub fn grants_access_to(&self, actor: UserId, athlete: UserId) -> bool {
        self.status.is_active() && self.coach_id == actor && self.athlete_id == athlete
    }
}

/// May `actor` read this athlete's personal data — programmes, measurements, goals?
///
/// The single question every per-athlete endpoint should ask. Three ways to say
/// yes, and they are genuinely different:
///
/// - **it is your own data** — a member always sees themselves
/// - **you manage the gym** — owners, admins and head coaches have oversight of
///   the whole roster; that is what those capacities mean
/// - **you are their coach** — an active relationship, and nothing weaker
///
/// A plain trainer with no relationship gets `false`, which is the entire point:
/// `can_coach` is gym-wide, and being able to coach *someone* must never imply
/// being able to read *everyone*.
#[must_use]
pub fn may_view_athlete(
    actor: UserId,
    actor_capabilities: &Capabilities,
    athlete: UserId,
    relationships: &[CoachRelationship],
) -> bool {
    if actor == athlete {
        return true;
    }
    if actor_capabilities.can_manage_catalogue() {
        return true;
    }
    relationships
        .iter()
        .any(|r| r.grants_access_to(actor, athlete))
}

/// May `actor` make coaching decisions *for* this athlete — assign a programme,
/// adjust their plan, withdraw them?
///
/// Deliberately different from `may_view_athlete` in one way: **being the
/// athlete yourself grants nothing.** Seeing your own data is a right; deciding
/// your own programming is your coach's job — a member self-assigning would
/// bypass the entire coaching workflow. (A solo user in a personal gym still
/// passes, but through the manager path: they are the owner there.)
#[must_use]
pub fn may_coach_athlete(
    actor: UserId,
    actor_capabilities: &Capabilities,
    athlete: UserId,
    relationships: &[CoachRelationship],
) -> bool {
    if actor_capabilities.can_manage_catalogue() {
        return true;
    }
    relationships
        .iter()
        .any(|r| r.grants_access_to(actor, athlete))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Capacity;

    fn caps(capacities: &[Capacity]) -> Capabilities {
        Capabilities::new(capacities.to_vec())
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn pair() -> (CoachRelationship, UserId, UserId) {
        let coach = UserId::new();
        let athlete = UserId::new();
        let rel = CoachRelationship::new(
            GymId::new(),
            coach,
            &caps(&[Capacity::Trainer]),
            athlete,
            &caps(&[Capacity::Member]),
            UserId::new(),
            now(),
        )
        .unwrap();
        (rel, coach, athlete)
    }

    #[test]
    fn pairs_a_trainer_with_a_member() {
        let (rel, ..) = pair();
        assert!(rel.status.is_active());
    }

    #[test]
    fn rejects_a_coach_who_cannot_coach() {
        let err = CoachRelationship::new(
            GymId::new(),
            UserId::new(),
            &caps(&[Capacity::Member]),
            UserId::new(),
            &caps(&[Capacity::Member]),
            UserId::new(),
            now(),
        );
        assert_eq!(err.unwrap_err(), CoachingError::UnsuitableParticipants);
    }

    #[test]
    fn rejects_an_athlete_who_is_not_in_the_gym() {
        let err = CoachRelationship::new(
            GymId::new(),
            UserId::new(),
            &caps(&[Capacity::Trainer]),
            UserId::new(),
            &caps(&[]),
            UserId::new(),
            now(),
        );
        assert_eq!(err.unwrap_err(), CoachingError::UnsuitableParticipants);
    }

    #[test]
    fn a_trainer_may_also_be_someone_elses_athlete() {
        // ADR-0014 allows holding several capacities in one gym. A trainer being
        // coached by the owner is a normal arrangement, and requiring the
        // athlete to be *only* a member would forbid it.
        let rel = CoachRelationship::new(
            GymId::new(),
            UserId::new(),
            &caps(&[Capacity::Owner]),
            UserId::new(),
            &caps(&[Capacity::Trainer, Capacity::Member]),
            UserId::new(),
            now(),
        );
        assert!(rel.is_ok());
    }

    #[test]
    fn only_managers_may_assign() {
        // ADR-0036 left one rung above trainer, so this is the whole of it.
        assert!(CoachRelationship::may_assign(&caps(&[Capacity::Owner])));

        // The important one: a trainer assigning themselves a client would be a
        // self-service grant of access to that member's data.
        assert!(!CoachRelationship::may_assign(&caps(&[Capacity::Trainer])));
        assert!(!CoachRelationship::may_assign(&caps(&[Capacity::Member])));
    }

    #[test]
    fn ending_records_who_and_when() {
        let (mut rel, ..) = pair();
        let ender = UserId::new();
        rel.end(ender, now()).unwrap();

        match rel.status {
            RelationshipStatus::Ended { ended_by, .. } => assert_eq!(ended_by, ender),
            RelationshipStatus::Active => panic!("should have ended"),
        }
    }

    #[test]
    fn cannot_end_twice() {
        let (mut rel, ..) = pair();
        rel.end(UserId::new(), now()).unwrap();
        assert_eq!(
            rel.end(UserId::new(), now()).unwrap_err(),
            CoachingError::AlreadyEnded
        );
    }

    #[test]
    fn an_ended_relationship_grants_nothing() {
        let (mut rel, coach, athlete) = pair();
        assert!(rel.grants_access_to(coach, athlete));

        rel.end(UserId::new(), now()).unwrap();
        assert!(
            !rel.grants_access_to(coach, athlete),
            "access must stop when the relationship does"
        );
    }

    #[test]
    fn access_is_not_symmetric() {
        // The coach sees the athlete. The athlete does not thereby see the coach.
        let (rel, coach, athlete) = pair();
        assert!(rel.grants_access_to(coach, athlete));
        assert!(!rel.grants_access_to(athlete, coach));
    }

    // ------------------------------------------------------- may_view_athlete

    #[test]
    fn a_member_always_sees_their_own_data() {
        let me = UserId::new();
        assert!(may_view_athlete(me, &caps(&[Capacity::Member]), me, &[]));
    }

    #[test]
    fn an_owner_sees_the_whole_roster() {
        assert!(may_view_athlete(
            UserId::new(),
            &caps(&[Capacity::Owner]),
            UserId::new(),
            &[]
        ));
    }

    #[test]
    fn a_trainer_sees_only_their_own_clients() {
        let (rel, coach, athlete) = pair();
        let stranger = UserId::new();
        let trainer = caps(&[Capacity::Trainer]);

        let held = std::slice::from_ref(&rel);
        assert!(may_view_athlete(coach, &trainer, athlete, held));

        // The whole reason this function exists: being able to coach SOMEONE
        // must never imply being able to read EVERYONE.
        assert!(!may_view_athlete(coach, &trainer, stranger, held));
    }

    #[test]
    fn a_trainer_with_no_relationship_sees_nobody() {
        assert!(!may_view_athlete(
            UserId::new(),
            &caps(&[Capacity::Trainer]),
            UserId::new(),
            &[]
        ));
    }

    #[test]
    fn access_ends_with_the_relationship() {
        let (mut rel, coach, athlete) = pair();
        rel.end(UserId::new(), now()).unwrap();

        assert!(!may_view_athlete(
            coach,
            &caps(&[Capacity::Trainer]),
            athlete,
            &[rel]
        ));
    }

    #[test]
    fn one_ended_relationship_does_not_mask_an_active_one() {
        // A coach who was reassigned away and then back must still have access.
        let gym = GymId::new();
        let coach = UserId::new();
        let athlete = UserId::new();

        let mut old = CoachRelationship::new(
            gym,
            coach,
            &caps(&[Capacity::Trainer]),
            athlete,
            &caps(&[Capacity::Member]),
            UserId::new(),
            now(),
        )
        .unwrap();
        old.end(UserId::new(), now()).unwrap();

        let current = CoachRelationship::new(
            gym,
            coach,
            &caps(&[Capacity::Trainer]),
            athlete,
            &caps(&[Capacity::Member]),
            UserId::new(),
            now(),
        )
        .unwrap();

        assert!(may_view_athlete(
            coach,
            &caps(&[Capacity::Trainer]),
            athlete,
            &[old, current]
        ));
    }

    // ------------------------------------------------------ may_coach_athlete

    #[test]
    fn a_trainer_may_coach_their_own_client_and_nobody_else() {
        let (rel, coach, athlete) = pair();
        let trainer = caps(&[Capacity::Trainer]);
        let held = std::slice::from_ref(&rel);

        assert!(may_coach_athlete(coach, &trainer, athlete, held));
        assert!(!may_coach_athlete(coach, &trainer, UserId::new(), held));
    }

    #[test]
    fn being_the_athlete_grants_no_coaching_authority() {
        // The deliberate difference from may_view_athlete: you may SEE your own
        // data, but you may not assign yourself programmes — that would bypass
        // the coaching workflow entirely.
        let me = UserId::new();
        assert!(may_view_athlete(me, &caps(&[Capacity::Member]), me, &[]));
        assert!(!may_coach_athlete(me, &caps(&[Capacity::Member]), me, &[]));
    }

    #[test]
    fn a_manager_may_coach_anyone_including_themselves() {
        // The solo-owner case: in a personal gym the owner assigns themselves
        // programmes through the manager path, not the self path.
        let owner = UserId::new();
        assert!(may_coach_athlete(
            owner,
            &caps(&[Capacity::Owner]),
            owner,
            &[]
        ));
    }

    #[test]
    fn coaching_authority_ends_with_the_relationship() {
        let (mut rel, coach, athlete) = pair();
        rel.end(UserId::new(), now()).unwrap();
        assert!(!may_coach_athlete(
            coach,
            &caps(&[Capacity::Trainer]),
            athlete,
            &[rel]
        ));
    }

    #[test]
    fn status_round_trips_through_json() {
        let (mut rel, ..) = pair();
        rel.end(UserId::new(), now()).unwrap();

        let json = serde_json::to_string(&rel.status).unwrap();
        assert!(json.contains("\"state\":\"ended\""), "got {json}");
        assert_eq!(
            serde_json::from_str::<RelationshipStatus>(&json).unwrap(),
            rel.status
        );
    }
}
