//! Gym entry — a member's QR pass, and staff scanning it at the door.
//!
//! Reuses `EntitlementService::for_member` wholesale: "may this account train
//! here right now" is exactly what a door needs to answer, and it is already
//! the single place that question is resolved (managers always pass, a gym
//! that bills nobody withholds nothing, `Feature::GymAccess` is the ladder's
//! bottom rung). This file does not re-derive any of that — it asks, then
//! writes down what it was told.

use std::sync::Arc;

use gym_domain::{
    CheckInId, TenantContext,
    checkin::CheckIn,
    entitlement::{Feature, Source},
};
use uuid::Uuid;

use crate::{
    ApplicationError, ApplicationResult,
    entitlements::EntitlementService,
    ports::{CheckInRepository, CheckInView, Clock, EntryPassIssuer},
};

/// Long enough to walk from pocket to scanner; short enough that a screenshot
/// is useless by the time anyone could act on it.
const PASS_TTL_SECONDS: i64 = 90;

pub struct EntryPass {
    pub token: String,
    pub expires_in_seconds: i64,
}

#[derive(Clone)]
pub struct CheckInService {
    pub checkins: Arc<dyn CheckInRepository>,
    pub entitlements: EntitlementService,
    pub passes: Arc<dyn EntryPassIssuer>,
    pub clock: Arc<dyn Clock>,
}

impl CheckInService {
    fn ensure_front_desk(&self, tenant: &TenantContext) -> ApplicationResult<()> {
        // "Front desk" is coaching level, not gym-management level: a trainer
        // works the floor and lets their own clients in every day, and making
        // them fetch a manager to open the door would be a worse product for
        // a rule the capacity model already expresses.
        if tenant.capabilities.can_coach() {
            Ok(())
        } else {
            Err(ApplicationError::Forbidden)
        }
    }

    /// Self-service: anyone signed in and in this gym gets their own pass —
    /// staff walk through the same door as everyone else.
    pub async fn my_pass(&self, tenant: &TenantContext) -> ApplicationResult<EntryPass> {
        let token = self
            .passes
            .issue(tenant.actor_id, tenant.gym_id, PASS_TTL_SECONDS)?;
        Ok(EntryPass {
            token,
            expires_in_seconds: PASS_TTL_SECONDS,
        })
    }

    /// Staff scans a pass. Always returns a `CheckIn` — a refusal is not an
    /// error from THIS call's point of view, it is the answer; `allowed`
    /// carries it, and it is recorded exactly like an admission would be.
    pub async fn scan(&self, tenant: &TenantContext, token: &str) -> ApplicationResult<CheckIn> {
        self.ensure_front_desk(tenant)?;

        // Every verification failure — bad signature, expired, wrong gym —
        // collapses to the same message: a screen at the door has no business
        // distinguishing "forged" from "stale" for whoever is holding it.
        let invalid = || {
            ApplicationError::Domain(gym_domain::DomainError::Invalid(
                "That code is not valid. Ask them to reopen their entry pass and try again."
                    .to_owned(),
            ))
        };
        let (member_id, pass_gym) = self.passes.verify(token).map_err(|_| invalid())?;
        if pass_gym != tenant.gym_id {
            return Err(invalid());
        }

        let held = self.entitlements.for_member(tenant, member_id).await?;
        let (allowed, reason) = match held.because(Feature::GymAccess) {
            Some(source) => (true, describe(source)),
            None => (false, "No active plan covers gym access.".to_owned()),
        };

        let now = self.clock.now();
        let checkin = CheckIn::new(
            CheckInId::from(Uuid::now_v7()),
            tenant.gym_id,
            member_id,
            tenant.actor_id,
            allowed,
            &reason,
            now,
        )?;

        self.checkins.insert(tenant, &checkin).await?;
        Ok(checkin)
    }

    /// The door's recent history — staff-only, same gate as scanning.
    pub async fn recent(&self, tenant: &TenantContext) -> ApplicationResult<Vec<CheckInView>> {
        self.ensure_front_desk(tenant)?;
        self.checkins.recent(tenant).await
    }
}

/// Matches `crates/api/src/routes/entitlements.rs`'s wording exactly — a
/// member reads the same sentence on the "why can I train here" screen and
/// hears it again the moment they walk in, not a second phrasing of the same
/// fact invented for the door.
fn describe(source: &Source) -> String {
    match source {
        Source::Subscription { plan_name } => format!("Your {plan_name} membership"),
        Source::NotBilled => "This gym does not bill through the app".to_owned(),
        Source::PlatformTier => "Included for every gym".to_owned(),
    }
}
