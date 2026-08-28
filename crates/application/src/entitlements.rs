//! Resolving entitlements — the single place that answers "may this actor use
//! this feature right now".
//!
//! Every feature gate in the codebase asks this service. That is the point of
//! building it before the platform bills anyone: when real SaaS tiers and
//! dunning arrive, they change *this file*, not every feature that depends on
//! them ([feature-plan-2026-07.md](../../../docs/feature-plan-2026-07.md) §6).
//!
//! The resolution rules, in order:
//!
//! 1. **A gym with no plans does not bill through the platform** — everyone in
//!    it holds everything, sourced `NotBilled`. This is the rule that keeps the
//!    app usable for the gyms that only want programming and logging, and
//!    getting it backwards would break training everywhere on day one.
//! 2. **Otherwise, a member holds what their active subscriptions grant.** A
//!    plan says what it confers; two plans granting the same feature is one
//!    entitlement.
//! 3. **Managers are never locked out of their own gym.** An owner whose card
//!    failed still has to be able to log in and fix it, so gym management is
//!    not gated on the gym's own bill.

use std::sync::Arc;

use gym_domain::{
    TenantContext, UserId,
    entitlement::{EntitlementSet, Feature, Source},
};

use crate::{
    ApplicationResult,
    ports::{BillingRepository, PlanView},
};

#[derive(Clone)]
pub struct EntitlementService {
    pub billing: Arc<dyn BillingRepository>,
}

impl EntitlementService {
    /// What this gym itself may use — the platform→gym SaaS side.
    ///
    /// No gym subscribes to the platform yet (ADR-0010 defers that until there
    /// are paying gyms), so every gym resolves to the full set. When tiers
    /// exist, this function is the one that changes.
    pub async fn for_gym(&self, _tenant: &TenantContext) -> ApplicationResult<EntitlementSet> {
        Ok(EntitlementSet::everything(Source::PlatformTier))
    }

    /// What one member of this gym may use.
    pub async fn for_member(
        &self,
        tenant: &TenantContext,
        member: UserId,
    ) -> ApplicationResult<EntitlementSet> {
        let plans = self.billing.list_plans(tenant).await?;

        // Rule 1: a gym that sells nothing is not withholding anything.
        if !bills_anyone(&plans) {
            return Ok(EntitlementSet::everything(Source::NotBilled));
        }

        let subscriptions = self.billing.list_subscriptions(tenant).await?;

        let held = subscriptions
            .iter()
            .filter(|view| view.subscription.member_id == member)
            .filter(|view| view.subscription.status.is_active())
            .flat_map(|view| {
                let plan_name = view.plan_name.clone();
                plans
                    .iter()
                    .find(|p| p.plan.id == view.subscription.plan_id)
                    .map(|p| p.plan.grants.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |feature| {
                        (
                            feature,
                            Source::Subscription {
                                plan_name: plan_name.clone(),
                            },
                        )
                    })
            })
            .collect();

        Ok(EntitlementSet::new(held))
    }

    /// The gate itself. Returns `Ok(())` when allowed, so call sites read as
    /// one line and cannot accidentally ignore the answer.
    ///
    /// Managers pass unconditionally (rule 3): an owner whose own gym is behind
    /// on its bill must still be able to get in and sort it out.
    pub async fn require(
        &self,
        tenant: &TenantContext,
        member: UserId,
        feature: Feature,
    ) -> ApplicationResult<()> {
        if tenant.capabilities.can_manage_gym() {
            return Ok(());
        }

        let held = self.for_member(tenant, member).await?;
        if held.allows(feature) {
            Ok(())
        } else {
            Err(crate::ApplicationError::Forbidden)
        }
    }
}

/// Does this gym bill anyone at all? Archived plans do not count — a gym that
/// stopped selling has stopped billing, and its members should not lose access
/// because the owner tidied up.
fn bills_anyone(plans: &[PlanView]) -> bool {
    plans.iter().any(|p| p.plan.is_offered())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gym_domain::billing::{BillingInterval, MembershipPlan};
    use gym_domain::{GymId, MembershipPlanId};

    fn plan(name: &str, grants: Vec<Feature>, archived: bool) -> PlanView {
        let mut p = MembershipPlan::new(
            MembershipPlanId::from(uuid::Uuid::nil()),
            GymId::from(uuid::Uuid::nil()),
            name,
            None,
            1000,
            "GBP",
            BillingInterval::Monthly,
            grants,
        )
        .unwrap();
        if archived {
            p.archived_at = Some(chrono::Utc::now());
        }
        PlanView {
            plan: p,
            active_subscribers: 0,
        }
    }

    #[test]
    fn a_gym_with_no_plans_bills_nobody() {
        assert!(!bills_anyone(&[]));
    }

    #[test]
    fn a_gym_whose_only_plan_is_archived_bills_nobody() {
        // The owner tidying up their price list must not revoke access.
        assert!(!bills_anyone(&[plan(
            "Old",
            vec![Feature::GymAccess],
            true
        )]));
    }

    #[test]
    fn one_offered_plan_means_the_gym_bills() {
        assert!(bills_anyone(&[
            plan("Old", vec![Feature::GymAccess], true),
            plan("Current", vec![Feature::GymAccess], false),
        ]));
    }
}
