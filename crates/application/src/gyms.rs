//! Gyms: creation, the open door, settings, and who holds what.
//!
//! Creation is separate from account creation so onboarding can ask for an
//! account first and the "what do you want to do?" question second (ADR-0014).
//!
//! Standing lives here too, because since ADR-0031 it is the only way staff
//! exist: there are no invitations, so everyone joins as a member and somebody
//! who runs the gym promotes them.

use std::sync::Arc;

use gym_domain::{
    Capabilities, Capacity, GymId, TenantContext, UserId,
    gym::Gym,
    tenancy::{StandingError, check_standing_change, check_standing_grant},
    user::{Email, User},
};

use crate::{
    ApplicationError, ApplicationResult,
    ports::{GymMember, GymRepository, PasswordHasher, TokenIssuer, UserRepository},
};

/// How long a generated starting password is, in hex characters.
///
/// Cut from a 256-bit opaque token, so the entropy is not in question; the
/// length is about the human on the other end reading it aloud once. Sixteen
/// is long enough that it is not worth attacking and short enough to say down
/// a phone without losing your place.
const TEMP_PASSWORD_LEN: usize = 16;

/// A staff account, and the one-time password to hand over with it.
#[derive(Debug, Clone)]
pub struct CreatedStaff {
    pub user: User,
    pub capacities: Vec<Capacity>,
    /// Shown to the creator **once**. Never stored in plaintext, never
    /// retrievable, and not in the audit trail — the same contract the
    /// invitation token had, for the same reason.
    pub temporary_password: String,
}

#[derive(Debug, Clone)]
pub struct CreateStaffCommand {
    pub email: String,
    pub display_name: String,
    pub capacities: Vec<Capacity>,
}

#[derive(Debug, Clone)]
pub struct CreateGymCommand {
    pub name: String,
    /// A solo trainer's or self-coached athlete's own workspace, rather than a
    /// commercial gym. Same tenancy model either way — only the framing differs.
    pub is_personal: bool,
}

#[derive(Clone)]
pub struct GymService {
    pub gyms: Arc<dyn GymRepository>,
    pub users: Arc<dyn UserRepository>,
    /// For staff accounts only: the starting password has to be hashed before
    /// it is stored, like every other one.
    pub hasher: Arc<dyn PasswordHasher>,
    /// Source of the starting password. Reuses the opaque-token generator
    /// rather than growing a second random source — 256 bits from the same
    /// CSPRNG that mints refresh tokens.
    pub tokens: Arc<dyn TokenIssuer>,
}

impl GymService {
    /// Create a gym; the creator becomes an owner.
    ///
    /// No authorization check: any authenticated account may create a gym, the
    /// same way anyone may create a workspace. Authorization begins once the gym
    /// exists and other people are involved.
    pub async fn create(&self, creator: UserId, cmd: CreateGymCommand) -> ApplicationResult<Gym> {
        let gym = Gym::new_with_kind(&cmd.name, cmd.is_personal)?;
        self.gyms.create_with_owner(&gym, creator).await?;
        Ok(gym)
    }

    /// Everyone in the gym.
    ///
    /// Restricted to head coaches and above, deliberately more narrowly than
    /// `can_coach`. A membership list is personal information about every person
    /// on it — who trains at this gym is exactly the kind of thing a member has
    /// not agreed to share with every other member — and the operational need for
    /// it (pairing a coach with an athlete) sits at head-coach level anyway. A
    /// trainer sees the people they actually coach, which is what they need.
    pub async fn roster(&self, tenant: &TenantContext) -> ApplicationResult<Vec<GymMember>> {
        if !tenant.capabilities.can_manage_catalogue() {
            return Err(ApplicationError::Forbidden);
        }
        self.users.roster(tenant).await
    }

    /// Gyms anyone may walk into (ADR-0026).
    ///
    /// Answered for an authenticated account that holds no membership at all —
    /// the only situation in the API where there is no tenant to scope by. It
    /// returns names and ids of gyms whose owners opted in to being findable,
    /// and nothing about who trains at them.
    pub async fn open_for_registration(&self) -> ApplicationResult<Vec<Gym>> {
        self.gyms.open_for_registration().await
    }

    /// Walk through the open door.
    ///
    /// Grants `member`, never anything else. The capacity is not a parameter
    /// and is not read from the request — see `join_as_member` — so no input
    /// can turn the open door into a way to make yourself an owner.
    pub async fn join(&self, gym: GymId, user: UserId) -> ApplicationResult<()> {
        self.gyms.join_as_member(gym, user).await
    }

    /// The gym as its managers see it, for the settings screen.
    pub async fn find_for_settings(&self, tenant: &TenantContext) -> ApplicationResult<Gym> {
        self.gyms
            .find(tenant.gym_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "gym" })
    }

    /// Change what somebody holds here (ADR-0031).
    ///
    /// This is the whole of staff management now that invitations are gone.
    /// Everyone arrives through the open door as a `member`; becoming a
    /// trainer, head coach, admin or owner is this call, made by somebody who
    /// already runs the gym, recorded in the audit trail.
    ///
    /// Why replace rather than grant: "promote to trainer" and "they are just
    /// a member again" are the same operation with different arguments, and a
    /// grant-only API needs a revoke twin that can disagree with it. The rules
    /// live in `check_standing_change` — including the one that stops the last
    /// owner demoting themselves and locking everyone out.
    pub async fn set_capacities(
        &self,
        tenant: &TenantContext,
        target: UserId,
        capacities: Vec<Capacity>,
    ) -> ApplicationResult<Vec<Capacity>> {
        let next = Capabilities::new(capacities);
        let current = self.users.capabilities_in(target, tenant.gym_id).await?;
        // Only consulted when it matters, but computed before the check so the
        // domain function stays pure and takes the number rather than a port.
        let other_owners = if current.is_owner() {
            self.users.owners_other_than(tenant, target).await?
        } else {
            usize::MAX
        };

        check_standing_change(&tenant.capabilities, &current, &next, other_owners)?;

        self.users
            .set_capacities(tenant, target, next.held())
            .await?;

        Ok(next.held().to_vec())
    }

    /// Create a staff account outright (ADR-0032).
    ///
    /// The other way round from everything else here: instead of waiting for
    /// somebody to sign up and walk through the open door so they can be
    /// promoted, an owner makes the account and the standing in one go, and
    /// hands over a password.
    ///
    /// **The account already existing is a refusal, not a merge.** Attaching
    /// somebody's existing account to a gym because a manager typed their
    /// address would be a membership granted without consent — the thing
    /// invitations existed to avoid. If they already have an account they join
    /// through the door themselves, and then they are promoted from the
    /// roster, which is two taps and involves them.
    ///
    /// The password is generated rather than chosen by the creator. An owner
    /// picking a colleague's first password picks a bad one, and picks the
    /// same one twice.
    pub async fn create_staff(
        &self,
        tenant: &TenantContext,
        cmd: CreateStaffCommand,
    ) -> ApplicationResult<CreatedStaff> {
        let next = Capabilities::new(cmd.capacities);
        check_standing_grant(&tenant.capabilities, &next)?;

        let email = Email::parse(&cmd.email)?;
        let user = User::new(email.clone(), &cmd.display_name)?;

        // Cheap pre-check for a friendly error; the unique index in
        // `insert_staff` is the real guarantee, and it settles the race.
        if self.users.find_by_email(&email).await?.is_some() {
            return Err(ApplicationError::Conflict("email".to_owned()));
        }

        let temporary_password: String = self
            .tokens
            .generate_opaque()?
            .raw
            .chars()
            .take(TEMP_PASSWORD_LEN)
            .collect();
        let password_hash = self.hasher.hash(&temporary_password)?;

        self.users
            .insert_staff(tenant, &user, &password_hash, next.held())
            .await?;

        Ok(CreatedStaff {
            user,
            capacities: next.held().to_vec(),
            temporary_password,
        })
    }

    /// Open or close the door. Owners and admins only.
    ///
    /// `can_manage_gym`, not `can_manage_catalogue`: this is a gym *settings*
    /// decision about who may walk in, not a coaching one, and a head coach
    /// should not be able to change the gym's membership policy.
    pub async fn set_open_registration(
        &self,
        tenant: &TenantContext,
        open: bool,
    ) -> ApplicationResult<Gym> {
        if !tenant.capabilities.can_manage_gym() {
            return Err(ApplicationError::Forbidden);
        }
        self.gyms.set_open_registration(tenant, open).await
    }
}

impl From<StandingError> for ApplicationError {
    fn from(err: StandingError) -> Self {
        match err {
            // "You may not" and "not like that" are different answers, and a
            // client shows different words for each.
            StandingError::NotPermitted | StandingError::OwnerIsOwnersToGive => Self::Forbidden,
            StandingError::NotAMember => Self::NotFound { entity: "member" },
            other => Self::Domain(gym_domain::DomainError::Invalid(other.to_string())),
        }
    }
}
