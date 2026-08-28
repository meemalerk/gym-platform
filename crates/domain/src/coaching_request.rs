//! A member asking a coach to work with them (ADR-0025).
//!
//! Why this exists at all, when `CoachRelationship` already models the pairing:
//! the relationship is an **access grant**. `may_coach_athlete` opens that
//! member's whole training history, measurements and goals to another person.
//! Until now the only way to create one was for a head coach to do it, which
//! is correct but leaves no route at all for the ordinary case — a member who
//! walks in and wants a coach.
//!
//! The two obvious shortcuts are both wrong:
//!
//! - **Let the member pick and pair instantly.** A member would be granting
//!   someone access to their data with one tap, and a trainer would acquire a
//!   client without being asked. One-sided consent on a two-sided grant.
//! - **Let the trainer pick.** That is precisely what
//!   `CoachRelationship::may_assign` refuses, and for good reason: anyone with
//!   `can_coach` could then grant themselves visibility of any member's data.
//!
//! So: the member asks, and the coach (or a manager, on their behalf) answers.
//! Both sides consent to the thing that is actually happening, and the roster
//! privacy rule in `GymService::roster` survives untouched — a trainer
//! *directory* lists staff who published a professional profile, which is not
//! the membership list.
//!
//! Like every other relationship in this codebase, a request is **resolved,
//! never deleted**. "Sara asked Tariq and he said no" is a fact about the gym;
//! silently dropping the row would make it look like she never asked.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    Capabilities,
    ids::{CoachingRequestId, GymId, UserId},
};

/// The longest note a member may attach when asking.
///
/// Short on purpose. This is "I want to get back into squatting after an ankle
/// injury", not an intake form — the profile and the goals already carry the
/// structured version, and a long free-text field here would become a second,
/// unvalidated place for medical detail to live.
const MAX_MESSAGE_LEN: usize = 500;

/// Where a request stands.
///
/// Carries its own evidence, matching `RelationshipStatus` and `VersionStatus`:
/// "declined, but with no decider and no date" is not a state anyone should
/// have to handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RequestStatus {
    Pending,
    Accepted {
        decided_at: DateTime<Utc>,
        /// The coach, or a manager acting for them.
        decided_by: UserId,
    },
    Declined {
        decided_at: DateTime<Utc>,
        decided_by: UserId,
    },
    /// The member changed their mind before anyone answered. Distinct from
    /// `Declined` because who walked away matters — a gym looking at "lots of
    /// unanswered requests" needs to know whether it is losing people to slow
    /// replies or they are simply sorting themselves out.
    Withdrawn {
        decided_at: DateTime<Utc>,
    },
}

impl RequestStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted { .. } => "accepted",
            Self::Declined { .. } => "declined",
            Self::Withdrawn { .. } => "withdrawn",
        }
    }

    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RequestError {
    #[error("this request has already been answered")]
    AlreadyAnswered,

    #[error("only the member who asked may withdraw a request")]
    NotYours,

    #[error("that person does not coach at this gym")]
    NotACoach,

    #[error("you already have a request outstanding with this coach")]
    AlreadyPending,

    #[error("you are already working with this coach")]
    AlreadyCoached,

    #[error("you cannot ask to coach yourself")]
    SelfRequest,

    #[error("only a head coach or above may propose a coach for somebody")]
    NotAManager,

    #[error("proposing yourself as somebody's coach is not a proposal")]
    SelfProposal,
}

/// A member's request for a specific coach to take them on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CoachingRequest {
    pub id: CoachingRequestId,
    pub gym_id: GymId,
    pub athlete_id: UserId,
    pub coach_id: UserId,
    /// Who started it. Equal to `athlete_id` when the member asked; a manager's
    /// id when the gym proposed a coach for them (ADR-0034). Load-bearing, not
    /// informational — `may_answer` turns on it.
    pub raised_by: UserId,
    pub status: RequestStatus,
    /// Why they are asking, in their own words. Optional — "will you coach me"
    /// is a complete request.
    pub message: Option<String>,
    pub requested_at: DateTime<Utc>,
}

impl CoachingRequest {
    /// Raise a request.
    ///
    /// The athlete's own capacities are not checked beyond membership: under
    /// ADR-0014 a trainer may perfectly well be somebody else's client, and
    /// requiring `member` here would forbid the staff-member-who-also-trains
    /// case the identity model exists to allow.
    pub fn raise(
        gym_id: GymId,
        athlete_id: UserId,
        coach_id: UserId,
        coach_capabilities: &Capabilities,
        raised_by: UserId,
        message: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Self, RequestError> {
        if athlete_id == coach_id {
            return Err(RequestError::SelfRequest);
        }
        if !coach_capabilities.can_coach() {
            return Err(RequestError::NotACoach);
        }

        let message = match message.map(str::trim) {
            None | Some("") => None,
            Some(text) => Some(text.chars().take(MAX_MESSAGE_LEN).collect()),
        };

        Ok(Self {
            id: CoachingRequestId::new(),
            gym_id,
            athlete_id,
            coach_id,
            raised_by,
            status: RequestStatus::Pending,
            message,
            requested_at: now,
        })
    }

    /// The gym proposes a coach for a member, and the coach answers.
    ///
    /// The other direction from `choose`, and the reason `raised_by` exists. A
    /// manager pairing a trainer with a member used to create an ACTIVE
    /// relationship immediately: the trainer acquired a client, and access to
    /// that member's whole training history, without being asked. That is the
    /// one-sided consent this module's own doc comment warns about — it was
    /// simply never applied to the manager path.
    ///
    /// So this stays **Pending**. The named coach accepts, and accepting is what
    /// creates the relationship. A manager cannot answer their own proposal
    /// (`may_answer`), because a handshake you can complete alone is not one.
    pub fn propose(
        gym_id: GymId,
        athlete_id: UserId,
        coach_id: UserId,
        coach_capabilities: &Capabilities,
        proposed_by: UserId,
        proposer_capabilities: &Capabilities,
        message: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Self, RequestError> {
        // Deciding which trainer works with which member is gym management —
        // the same gate that used to guard direct pairing.
        if !proposer_capabilities.can_manage_catalogue() {
            return Err(RequestError::NotAManager);
        }
        // A manager proposing THEMSELVES as the coach would be the self-service
        // grant `CoachRelationship::may_assign` refused, arriving by a new road.
        if proposed_by == coach_id {
            return Err(RequestError::SelfProposal);
        }
        Self::raise(
            gym_id,
            athlete_id,
            coach_id,
            coach_capabilities,
            proposed_by,
            message,
            now,
        )
    }

    /// Did the gym propose this, rather than the member ask for it?
    #[must_use]
    pub fn is_proposal(&self) -> bool {
        self.raised_by != self.athlete_id
    }

    /// The member chooses a coach, and that is the whole of it (ADR-0031).
    ///
    /// Returns a request that is **already accepted**, because there is nobody
    /// left to accept it. The two-sided handshake ADR-0025 built is gone: the
    /// thing a member was granting was access to their own training history,
    /// which is theirs to grant, and making them wait for a coach to press a
    /// button in an app they may not open until Thursday meant the pairing —
    /// and therefore the programme, and therefore the whole product — did not
    /// happen.
    ///
    /// Still recorded as a request rather than only a relationship, so the
    /// message survives. A pairing with no "back to squatting after an ankle
    /// injury" attached loses the one piece of context the coach needed, and
    /// that note is the reason members write anything here at all.
    ///
    /// `decided_by` is the athlete. That is the honest record: they decided.
    pub fn choose(
        gym_id: GymId,
        athlete_id: UserId,
        coach_id: UserId,
        coach_capabilities: &Capabilities,
        message: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Self, RequestError> {
        let mut request = Self::raise(
            gym_id,
            athlete_id,
            coach_id,
            coach_capabilities,
            // The member asked, so the member raised it.
            athlete_id,
            message,
            now,
        )?;
        request.status = RequestStatus::Accepted {
            decided_at: now,
            decided_by: athlete_id,
        };
        Ok(request)
    }

    /// May `actor` answer this request?
    ///
    /// The coach it was addressed to — always. A manager — only where the
    /// manager is not the one who raised it.
    ///
    /// That exception is the whole point of a proposal (ADR-0034). If any
    /// manager could accept a manager-raised proposal, the owner would simply
    /// propose and accept in two taps and the trainer's consent would be
    /// decoration. A member-raised request (the pre-ADR-0031 rows) still lets a
    /// manager answer on the coach's behalf, so none of those are stranded.
    #[must_use]
    pub fn may_answer(&self, actor: UserId, actor_capabilities: &Capabilities) -> bool {
        if actor == self.coach_id {
            return true;
        }
        !self.is_proposal() && actor_capabilities.can_manage_catalogue()
    }

    pub fn accept(&mut self, by: UserId, now: DateTime<Utc>) -> Result<(), RequestError> {
        self.ensure_pending()?;
        self.status = RequestStatus::Accepted {
            decided_at: now,
            decided_by: by,
        };
        Ok(())
    }

    pub fn decline(&mut self, by: UserId, now: DateTime<Utc>) -> Result<(), RequestError> {
        self.ensure_pending()?;
        self.status = RequestStatus::Declined {
            decided_at: now,
            decided_by: by,
        };
        Ok(())
    }

    /// The member changes their mind. Only they may — a coach who does not want
    /// a client declines, which is a different and more honest record.
    pub fn withdraw(&mut self, by: UserId, now: DateTime<Utc>) -> Result<(), RequestError> {
        if by != self.athlete_id {
            return Err(RequestError::NotYours);
        }
        self.ensure_pending()?;
        self.status = RequestStatus::Withdrawn { decided_at: now };
        Ok(())
    }

    fn ensure_pending(&self) -> Result<(), RequestError> {
        if self.status.is_pending() {
            Ok(())
        } else {
            Err(RequestError::AlreadyAnswered)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Capacity;

    fn coach_caps() -> Capabilities {
        Capabilities::new(vec![Capacity::Trainer])
    }

    fn member_caps() -> Capabilities {
        Capabilities::new(vec![Capacity::Member])
    }

    fn manager_caps() -> Capabilities {
        // The only rung above trainer since ADR-0036.
        Capabilities::new(vec![Capacity::Owner])
    }

    fn raise() -> (CoachingRequest, UserId, UserId) {
        let athlete = UserId::new();
        let coach = UserId::new();
        let req = CoachingRequest::raise(
            GymId::new(),
            athlete,
            coach,
            &coach_caps(),
            athlete,
            Some(" back to squatting after an ankle injury "),
            Utc::now(),
        )
        .unwrap();
        (req, athlete, coach)
    }

    /// The gym proposes; the trainer answers.
    fn proposal() -> (CoachingRequest, UserId, UserId, UserId) {
        let athlete = UserId::new();
        let coach = UserId::new();
        let owner = UserId::new();
        let req = CoachingRequest::propose(
            GymId::new(),
            athlete,
            coach,
            &coach_caps(),
            owner,
            &manager_caps(),
            Some("Tariq has space on Tuesdays"),
            Utc::now(),
        )
        .unwrap();
        (req, athlete, coach, owner)
    }

    #[test]
    fn a_proposal_starts_pending_and_knows_it_is_one() {
        let (req, athlete, _, owner) = proposal();
        assert!(req.status.is_pending(), "the trainer has not answered yet");
        assert!(req.is_proposal());
        assert_eq!(req.raised_by, owner);
        assert_ne!(req.raised_by, athlete);
    }

    #[test]
    fn a_member_asking_is_not_a_proposal() {
        let (req, athlete, _, _) = {
            let (r, a, c) = raise();
            (r, a, c, ())
        };
        assert!(!req.is_proposal());
        assert_eq!(req.raised_by, athlete);
    }

    /// The rule the whole feature turns on: a manager who proposes cannot then
    /// accept it. Otherwise "the trainer accepts" is two taps by the owner.
    #[test]
    fn the_proposer_cannot_answer_their_own_proposal() {
        let (req, _, coach, owner) = proposal();
        assert!(
            !req.may_answer(owner, &manager_caps()),
            "the owner who proposed it must not be able to rubber-stamp it"
        );
        // Nor any OTHER manager — consent belongs to the trainer, not the rank.
        assert!(
            !req.may_answer(UserId::new(), &manager_caps()),
            "a second manager accepting for the trainer is the same defect"
        );
        assert!(
            req.may_answer(coach, &coach_caps()),
            "the named trainer answers"
        );
    }

    /// A member-raised request keeps the old behaviour, so pre-ADR-0031 rows
    /// are not stranded unanswerable.
    #[test]
    fn a_manager_may_still_answer_a_member_raised_request() {
        let (req, _, _) = raise();
        assert!(req.may_answer(UserId::new(), &manager_caps()));
    }

    #[test]
    fn only_a_manager_may_propose() {
        let err = CoachingRequest::propose(
            GymId::new(),
            UserId::new(),
            UserId::new(),
            &coach_caps(),
            UserId::new(),
            &coach_caps(),
            None,
            Utc::now(),
        )
        .unwrap_err();
        assert_eq!(err, RequestError::NotAManager);
    }

    /// A manager naming THEMSELVES as the coach is the self-service access grant
    /// direct pairing was refused for — arriving by a new road.
    #[test]
    fn a_manager_cannot_propose_themselves() {
        let me = UserId::new();
        let err = CoachingRequest::propose(
            GymId::new(),
            UserId::new(),
            me,
            &coach_caps(),
            me,
            &manager_caps(),
            None,
            Utc::now(),
        )
        .unwrap_err();
        assert_eq!(err, RequestError::SelfProposal);
    }

    #[test]
    fn a_proposal_still_needs_somebody_who_actually_coaches() {
        let err = CoachingRequest::propose(
            GymId::new(),
            UserId::new(),
            UserId::new(),
            &member_caps(),
            UserId::new(),
            &manager_caps(),
            None,
            Utc::now(),
        )
        .unwrap_err();
        assert_eq!(err, RequestError::NotACoach);
    }

    #[test]
    fn a_declined_proposal_records_the_trainer_who_declined() {
        let (mut req, _, coach, _) = proposal();
        req.decline(coach, Utc::now()).unwrap();
        match req.status {
            RequestStatus::Declined { decided_by, .. } => assert_eq!(decided_by, coach),
            other => panic!("expected declined, got {other:?}"),
        }
    }

    #[test]
    fn a_request_starts_pending() {
        let (req, _, _) = raise();
        assert!(req.status.is_pending());
    }

    #[test]
    fn the_message_is_trimmed_and_blank_becomes_none() {
        let (req, _, _) = raise();
        assert_eq!(
            req.message.as_deref(),
            Some("back to squatting after an ankle injury")
        );

        let asker = UserId::new();
        let bare = CoachingRequest::raise(
            GymId::new(),
            asker,
            UserId::new(),
            &coach_caps(),
            asker,
            Some("   "),
            Utc::now(),
        )
        .unwrap();
        assert!(
            bare.message.is_none(),
            "\"will you coach me\" needs no essay"
        );
    }

    #[test]
    fn an_overlong_message_is_truncated_not_refused() {
        // Refusing would lose what they wrote. This is a note, not a field
        // anything downstream parses.
        let long = "x".repeat(MAX_MESSAGE_LEN + 200);
        let asker = UserId::new();
        let req = CoachingRequest::raise(
            GymId::new(),
            asker,
            UserId::new(),
            &coach_caps(),
            asker,
            Some(&long),
            Utc::now(),
        )
        .unwrap();
        assert_eq!(req.message.unwrap().chars().count(), MAX_MESSAGE_LEN);
    }

    #[test]
    fn you_cannot_ask_someone_who_does_not_coach() {
        let asker = UserId::new();
        let err = CoachingRequest::raise(
            GymId::new(),
            asker,
            UserId::new(),
            &member_caps(),
            asker,
            None,
            Utc::now(),
        )
        .unwrap_err();
        assert_eq!(err, RequestError::NotACoach);
    }

    #[test]
    fn you_cannot_ask_yourself() {
        let me = UserId::new();
        let err = CoachingRequest::raise(GymId::new(), me, me, &coach_caps(), me, None, Utc::now())
            .unwrap_err();
        assert_eq!(err, RequestError::SelfRequest);
    }

    #[test]
    fn the_addressed_coach_may_answer() {
        let (req, _, coach) = raise();
        assert!(req.may_answer(coach, &coach_caps()));
    }

    #[test]
    fn another_trainer_may_not_answer_it() {
        let (req, _, _) = raise();
        assert!(
            !req.may_answer(UserId::new(), &coach_caps()),
            "a request is addressed to one coach, not to the floor"
        );
    }

    #[test]
    fn a_manager_may_answer_on_a_coachs_behalf() {
        let (req, _, _) = raise();
        assert!(req.may_answer(UserId::new(), &manager_caps()));
    }

    #[test]
    fn accepting_records_who_and_when() {
        let (mut req, _, coach) = raise();
        let now = Utc::now();
        req.accept(coach, now).unwrap();
        assert_eq!(
            req.status,
            RequestStatus::Accepted {
                decided_at: now,
                decided_by: coach
            }
        );
    }

    #[test]
    fn a_request_is_answered_once() {
        let (mut req, _, coach) = raise();
        req.accept(coach, Utc::now()).unwrap();
        assert_eq!(
            req.decline(coach, Utc::now()).unwrap_err(),
            RequestError::AlreadyAnswered
        );
    }

    #[test]
    fn only_the_asker_withdraws() {
        let (mut req, athlete, coach) = raise();
        assert_eq!(
            req.withdraw(coach, Utc::now()).unwrap_err(),
            RequestError::NotYours,
            "a coach who does not want a client declines — that is the honest record"
        );
        req.withdraw(athlete, Utc::now()).unwrap();
        assert_eq!(req.status.as_str(), "withdrawn");
    }

    #[test]
    fn withdrawn_is_distinct_from_declined() {
        // A gym reading "lots of unanswered requests" needs to know whether it
        // is losing people to slow replies or they sorted themselves out.
        let (mut a, athlete_a, _) = raise();
        let (mut b, _, coach_b) = raise();
        a.withdraw(athlete_a, Utc::now()).unwrap();
        b.decline(coach_b, Utc::now()).unwrap();
        assert_ne!(a.status.as_str(), b.status.as_str());
    }
}
