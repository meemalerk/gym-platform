/**
 * Client mirror of the server's `Capabilities` (crates/domain/src/tenancy.rs).
 *
 * **This decides what UI to show. It is not security.** The server re-checks
 * every request and remains the sole authority — these helpers only stop us
 * offering a control that would come back 403.
 *
 * Kept in its own module, free of React and of the store, for two reasons:
 *  - it is pure, so it can be tested without a renderer (`scripts/verify-nav.mjs`)
 *  - drift from the server is a real hazard, and drift is easier to spot when
 *    the rules sit together in one small file instead of scattered through screens
 *
 * Each rule below names the server function it mirrors. If you change one, change
 * both, or the UI starts lying about what a person can do.
 */

/**
 * The capacities a person can hold in one gym. Mirrors `Capacity`.
 *
 * Three since ADR-0036. `admin` and `head_coach` are gone — they were rungs
 * whose only distinguishing rights were "slightly less than owner", and every
 * question they answered is answered by owner.
 */
export const CAPACITIES = ['owner', 'trainer', 'member'] as const;

export type Capacity = (typeof CAPACITIES)[number];

export const can = {
  /** Mirrors `Capabilities::can_manage_gym` — staff, settings, billing. */
  manageGym: (c: string[]) => c.includes('owner'),

  /**
   * Mirrors `Capabilities::can_manage_catalogue` — exercises, programme
   * templates.
   *
   * Identical to `manageGym` since ADR-0036, and kept separate for the same
   * reason the server keeps both: they answer different questions, and a screen
   * that asks the catalogue question should say so. They would diverge again
   * the moment a rung is added between owner and trainer.
   */
  manageCatalogue: (c: string[]) => can.manageGym(c),

  /** Mirrors `Capabilities::can_coach` — write programmes, review sessions. */
  coach: (c: string[]) => can.manageCatalogue(c) || c.includes('trainer'),

  /**
   * Mirrors `Capabilities::can_set_capacities` — change what somebody holds
   * here. Since ADR-0031 this is the whole of staff management: invitations
   * are gone, everyone joins as a member, and this is how they become
   * anything else.
   *
   * The extra rule the server layers on top (only an owner may grant or
   * remove `owner`) is `setOwner` below — it is a different question and a
   * screen needs to ask it separately.
   */
  setCapacities: (c: string[]) => can.manageGym(c),

  /** Mirrors the owner check inside `check_standing_change`. */
  setOwner: (c: string[]) => c.includes('owner'),

  /**
   * Read the audit trail. Guarded by `can_manage_gym` in
   * `crates/api/src/routes/audit.rs` — an audit log names who did what, so it is
   * deliberately narrower than "can coach".
   */
  readAudit: (c: string[]) => can.manageGym(c),

  // -------------------------------------------------------------- ADR-0024
  //
  // The catalogue belongs to whoever runs the gym (ADR-0034). A trainer READS
  // it and assigns from it; they do not write it. Under the old rule five
  // trainers could each write their own near-duplicate of "Beginner Strength",
  // and progress is computed per exercise and per version, so the variants
  // fragment the data the product exists to accumulate.
  //
  // Mirrors `Capabilities::can_author_programs` and friends in
  // crates/domain/src/tenancy.rs. Change one, change both.

  /** Write programme content and move it through review. Head coach and above. */
  authorPrograms: (c: string[]) => can.manageCatalogue(c),

  /** Approve, publish or archive a version — the moves athletes feel. */
  publishPrograms: (c: string[]) => can.manageCatalogue(c),

  /** Add a movement to the catalogue (as a proposal, unless you curate). */
  proposeExercises: (c: string[]) => can.coach(c),

  /** Promote, retire or reinstate a movement. */
  curateCatalogue: (c: string[]) => can.manageCatalogue(c),

  /**
   * Propose a coach for a member. The manager's half of the handshake — the
   * named trainer still has to accept (ADR-0034).
   */
  proposeCoach: (c: string[]) => can.manageCatalogue(c),
};

/**
 * May I put THIS athlete on a programme?
 *
 * Not a capacity question, which is why it does not live in `can` above: the
 * answer is the RELATIONSHIP. Their own coach, or themselves — a manager writes
 * the catalogue and decides who coaches whom, and does not reach past the
 * trainer to prescribe (ADR-0034, mirroring
 * `AssignmentService::ensure_may_prescribe_for`).
 *
 * Pure, so `scripts/verify-nav.mjs`-style checks can exercise it with no
 * renderer and no device.
 */
export function mayPrescribeFor(
  actorId: string | undefined,
  athleteId: string,
  relationships: { coach_id: string; athlete_id: string; is_active: boolean }[],
): boolean {
  if (!actorId) return false;
  if (actorId === athleteId) return true;
  return relationships.some(
    (r) => r.is_active && r.coach_id === actorId && r.athlete_id === athleteId,
  );
}

/** Human label for a capacity, for badges. */
export const capacityLabel = (capacity: string): string =>
  ({
    owner: 'Owner',
    trainer: 'Trainer',
    member: 'Member',
  })[capacity] ?? capacity;
