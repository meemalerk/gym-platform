//! Coaching requests: a member asks, a coach answers (ADR-0025).
//!
//! The one thing to keep straight in here is *who* each check protects.
//!
//! - Raising a request is self-service, because it grants nothing. It is an
//!   ask. The only guards are that the target actually coaches here and that
//!   nobody spams the same coach.
//! - **Accepting** is where the access grant happens, and it creates the
//!   relationship in the same transaction — so there is never a moment where a
//!   member has been accepted and their coach cannot see them.
//! - Reading is scoped by involvement: your own requests, requests addressed to
//!   you, or everything if you manage the gym.

use std::sync::Arc;

use gym_domain::{
    CoachingRequestId, TenantContext, UserId,
    coaching::CoachRelationship,
    coaching_request::{CoachingRequest, RequestError},
};

use crate::{
    ApplicationError, ApplicationResult,
    ports::{
        Clock, CoachRepository, CoachingRequestRepository, CoachingRequestView,
        TrainerDirectoryEntry, TrainerDirectoryRepository, UserRepository,
    },
};

#[derive(Debug, Clone)]
pub struct RaiseRequestCommand {
    pub coach_id: UserId,
    pub message: Option<String>,
}

/// A manager naming a coach for a member. Both ids are explicit — unlike
/// `RaiseRequestCommand`, where the athlete is always the caller.
#[derive(Debug, Clone)]
pub struct ProposeCoachCommand {
    pub athlete_id: UserId,
    pub coach_id: UserId,
    /// A line for the trainer: "Malaak trains Tuesdays and Thursdays".
    pub message: Option<String>,
}

/// How a request was answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestDecision {
    Accept,
    Decline,
}

#[derive(Clone)]
pub struct CoachingRequestService {
    pub requests: Arc<dyn CoachingRequestRepository>,
    pub relationships: Arc<dyn CoachRepository>,
    pub directory: Arc<dyn TrainerDirectoryRepository>,
    pub users: Arc<dyn UserRepository>,
    pub clock: Arc<dyn Clock>,
}

impl CoachingRequestService {
    /// The gym's coaches, for a member choosing one.
    ///
    /// Open to anyone in the gym. This is not the roster — it carries only what
    /// each coach published about themselves professionally, so it leaks no
    /// member's name and no email. Making it manager-only would defeat the
    /// entire feature.
    pub async fn directory(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<TrainerDirectoryEntry>> {
        self.directory.trainers(tenant).await
    }

    /// The requests this caller may see.
    ///
    /// Managers see the gym's; everyone else sees the ones they are a party to.
    /// Note this is deliberately *not* "trainers see everything addressed to
    /// any trainer" — a request is addressed to one person.
    pub async fn list(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<CoachingRequestView>> {
        let all = self.requests.list(tenant).await?;

        if tenant.capabilities.can_manage_catalogue() {
            return Ok(all);
        }

        Ok(all
            .into_iter()
            .filter(|v| {
                v.request.athlete_id == tenant.actor_id || v.request.coach_id == tenant.actor_id
            })
            .collect())
    }

    /// Choose a coach. They coach you from that moment (ADR-0031).
    ///
    /// This used to raise a pending request that the coach then had to accept.
    /// The handshake is gone, and the reason it went is worth keeping written
    /// down: the thing being granted is access to the *member's own* training
    /// history, which is the member's to grant. Requiring a second party to
    /// agree before anything happened meant the pairing waited on somebody
    /// opening an app — and everything downstream of the pairing (a programme,
    /// a workout to do today) waited with it.
    ///
    /// The coach is not trapped. `CoachService::end` is still theirs to call,
    /// and ending a relationship they did not want is one tap against a queue
    /// they had to clear before anything could start.
    pub async fn choose(
        &self,
        tenant: &TenantContext,
        cmd: RaiseRequestCommand,
    ) -> ApplicationResult<CoachingRequest> {
        // The target must coach *here*. Read their capacities from the DB
        // rather than trusting the caller's claim — the same rule TenantScope
        // follows.
        //
        // "Not in this gym" and "in this gym but does not coach" both return
        // NOT FOUND, deliberately. Distinguishing them would turn this endpoint
        // into a membership oracle: post a user id, read the status code, learn
        // whether that person trains here. The invitation flow already refuses
        // to be probed that way and this matches it. The directory is the
        // sanctioned way to discover who coaches here, and it lists exactly the
        // people for whom this call succeeds.
        let coach_capabilities = self
            .users
            .capabilities_in(cmd.coach_id, tenant.gym_id)
            .await?;
        if !coach_capabilities.can_coach() {
            return Err(ApplicationError::NotFound { entity: "coach" });
        }

        // Already working together? Say so plainly rather than creating a
        // request that could only ever be a no-op.
        let existing = self
            .relationships
            .active_for_user(tenant, tenant.actor_id)
            .await?;
        if existing
            .iter()
            .any(|r| r.coach_id == cmd.coach_id && r.athlete_id == tenant.actor_id)
        {
            return Err(RequestError::AlreadyCoached.into());
        }

        let now = self.clock.now();

        // `choose` re-checks `can_coach` itself. That is not redundant — it is
        // the domain refusing to build a nonsensical request regardless of who
        // calls it, the same belt-and-braces as the programme triggers. What it
        // cannot do is choose the HTTP shape of the refusal, which is why the
        // anti-probing 404 lives above.
        let request = CoachingRequest::choose(
            tenant.gym_id,
            tenant.actor_id,
            cmd.coach_id,
            &coach_capabilities,
            cmd.message.as_deref(),
            now,
        )?;

        // Built here rather than in the repository because it is a domain
        // decision: `CoachRelationship::new` re-checks both participants'
        // capacities, and skipping that in the name of "we already know they
        // are fine" is how a stale capacity becomes a live access grant.
        let athlete_capabilities = self
            .users
            .capabilities_in(tenant.actor_id, tenant.gym_id)
            .await?;

        let pairing = CoachRelationship::new(
            tenant.gym_id,
            cmd.coach_id,
            &coach_capabilities,
            tenant.actor_id,
            &athlete_capabilities,
            // Created by the athlete: they are the one who decided, and the
            // audit trail should not credit the coach with a choice they were
            // not asked to make.
            tenant.actor_id,
            now,
        )?;

        self.requests
            .insert_chosen(tenant, &request, &pairing)
            .await?;
        Ok(request)
    }

    /// The gym proposes a coach for a member; the coach answers (ADR-0034).
    ///
    /// This replaces a manager creating the pairing outright. Same decision —
    /// which trainer works with which member is gym management — but it now
    /// lands as a **pending** request, because the thing being created grants a
    /// trainer access to that member's whole training history and they were
    /// never asked. `may_answer` refuses the proposer, so the owner cannot
    /// propose and accept in two taps.
    pub async fn propose(
        &self,
        tenant: &TenantContext,
        cmd: ProposeCoachCommand,
    ) -> ApplicationResult<CoachingRequest> {
        // Deciding which trainer works with which member is gym management —
        // the same gate that guarded direct pairing. Checked here, before any
        // lookup, so an unauthorised caller learns nothing about who exists;
        // `CoachingRequest::propose` re-checks it as belt-and-braces.
        if !tenant.capabilities.can_manage_catalogue() {
            return Err(ApplicationError::Forbidden);
        }

        // Both parties must be here, and the coach must actually coach. NOT
        // FOUND rather than a specific refusal, matching `choose`: telling a
        // caller which of the two ids is the problem turns this into a
        // membership oracle.
        let coach_capabilities = self
            .users
            .capabilities_in(cmd.coach_id, tenant.gym_id)
            .await?;
        if !coach_capabilities.can_coach() {
            return Err(ApplicationError::NotFound { entity: "coach" });
        }
        let athlete_capabilities = self
            .users
            .capabilities_in(cmd.athlete_id, tenant.gym_id)
            .await?;
        if athlete_capabilities.is_empty() {
            return Err(ApplicationError::NotFound { entity: "member" });
        }

        // Already paired? Say so rather than raising a request whose acceptance
        // could only fail on the relationship's own uniqueness rule.
        let existing = self
            .relationships
            .active_for_user(tenant, cmd.coach_id)
            .await?;
        if existing
            .iter()
            .any(|r| r.coach_id == cmd.coach_id && r.athlete_id == cmd.athlete_id)
        {
            return Err(RequestError::AlreadyCoached.into());
        }

        let request = CoachingRequest::propose(
            tenant.gym_id,
            cmd.athlete_id,
            cmd.coach_id,
            &coach_capabilities,
            tenant.actor_id,
            &tenant.capabilities,
            cmd.message.as_deref(),
            self.clock.now(),
        )?;

        self.requests.insert(tenant, &request).await?;
        Ok(request)
    }

    /// Answer a request addressed to you (or, as a manager, to one of your
    /// coaches).
    ///
    /// Accepting creates the coaching relationship. The pairing is built here,
    /// in the service, rather than in the repository, because it is a domain
    /// decision — `CoachRelationship::new` re-checks both participants'
    /// capacities, and skipping that in the name of "we already know they are
    /// fine" is how a stale capacity becomes a live access grant.
    pub async fn answer(
        &self,
        tenant: &TenantContext,
        id: CoachingRequestId,
        decision: RequestDecision,
    ) -> ApplicationResult<CoachingRequest> {
        let mut request = self
            .requests
            .find(tenant, id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "request" })?;

        if !request.may_answer(tenant.actor_id, &tenant.capabilities) {
            return Err(ApplicationError::Forbidden);
        }

        let now = self.clock.now();

        match decision {
            RequestDecision::Decline => {
                request.decline(tenant.actor_id, now)?;
                self.requests
                    .save_decision(tenant, &request, None, "coaching_request.declined")
                    .await?;
            }
            RequestDecision::Accept => {
                let coach_capabilities = self
                    .users
                    .capabilities_in(request.coach_id, tenant.gym_id)
                    .await?;
                let athlete_capabilities = self
                    .users
                    .capabilities_in(request.athlete_id, tenant.gym_id)
                    .await?;

                let pairing = CoachRelationship::new(
                    tenant.gym_id,
                    request.coach_id,
                    &coach_capabilities,
                    request.athlete_id,
                    &athlete_capabilities,
                    // The relationship is created BY whoever accepted — usually
                    // the coach, sometimes a manager clearing a queue. Recording
                    // the athlete here would misattribute a grant they asked for
                    // but did not make.
                    tenant.actor_id,
                    now,
                )?;

                request.accept(tenant.actor_id, now)?;
                self.requests
                    .save_decision(
                        tenant,
                        &request,
                        Some(&pairing),
                        "coaching_request.accepted",
                    )
                    .await?;
            }
        }

        Ok(request)
    }

    /// Change your mind before anyone answers.
    pub async fn withdraw(
        &self,
        tenant: &TenantContext,
        id: CoachingRequestId,
    ) -> ApplicationResult<CoachingRequest> {
        let mut request = self
            .requests
            .find(tenant, id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "request" })?;

        // `withdraw` itself refuses anyone but the asker, so this is not a
        // second check — it is the difference between "not yours" (403) and
        // "does not exist" (404), and a request you are not party to should
        // read as the latter.
        if request.athlete_id != tenant.actor_id {
            return Err(ApplicationError::NotFound { entity: "request" });
        }

        request.withdraw(tenant.actor_id, self.clock.now())?;
        self.requests
            .save_decision(tenant, &request, None, "coaching_request.withdrawn")
            .await?;

        Ok(request)
    }
}

impl From<RequestError> for ApplicationError {
    fn from(err: RequestError) -> Self {
        match err {
            // Well-formed, but the world moved: refresh, don't rewrite.
            RequestError::AlreadyAnswered
            | RequestError::AlreadyPending
            | RequestError::AlreadyCoached => Self::Conflict(err.to_string()),
            // Authorization refusals, not malformed requests. A caller who may
            // not do this needs 403 — a 400 says "fix your payload", which is
            // wrong and unactionable.
            RequestError::NotYours | RequestError::NotAManager => Self::Forbidden,
            other => Self::Domain(gym_domain::DomainError::Invalid(other.to_string())),
        }
    }
}
