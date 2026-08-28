/**
 * Typed operations against the gym API.
 *
 * Payload types are DERIVED from the generated OpenAPI schema, so a backend
 * contract change becomes a TypeScript error rather than a runtime failure on a
 * phone. Run `npm run codegen:api` after changing the API.
 */

import { request, signOutLocally } from '@/api/client';
import type { components } from '@/api/schema';
import { readPlanChoicePending } from '@/session/onboarding';
import {
  clearRefreshToken,
  readRefreshToken,
  saveRefreshToken,
  useSession,
  type Membership,
  type SessionUser,
} from '@/session/store';

type Schemas = components['schemas'];

type TokensPayload = Schemas['TokensResponse'];
type UserPayload = Schemas['UserSummary'];
type MePayload = Schemas['MeResponse'];
type SignUpPayload = Schemas['SignUpResponse'];

export type Exercise = Schemas['ExerciseResponse'];
export type Modality = Schemas['Modality'];
/** The discriminant of `Modality` — "repetitions" | "duration" | "distance". */
export type ModalityKind = Modality['kind'];

const toUser = (u: UserPayload): SessionUser => ({
  id: u.id,
  email: u.email,
  displayName: u.display_name,
});

/**
 * The server still reports `/me` as a list (it stays multi-gym-capable even
 * under ADR-0023 — see `MeResponse`'s doc on the backend). The app is what
 * commits to single-gym: take the first entry, or null. In a real deployment
 * (`SINGLE_GYM_MODE=true`) the list is never longer than one anyway.
 */
const toMembership = (memberships: MePayload['memberships']): Membership | null => {
  const m = memberships[0];
  return m
    ? {
        gymId: m.gym_id,
        gymName: m.gym_name,
        isPersonal: m.is_personal,
        capacities: m.capacities,
      }
    : null;
};

// ----------------------------------------------------------------------- auth

/**
 * Create an account. Asks nothing about gyms or roles — that is the next,
 * reversible step (ADR-0014).
 */
export async function signUp(input: {
  email: string;
  password: string;
  displayName: string;
}): Promise<void> {
  const result = await request<SignUpPayload>('/api/v1/auth/sign-up', {
    method: 'POST',
    authenticated: false,
    body: {
      email: input.email,
      password: input.password,
      display_name: input.displayName,
      device_label: 'mobile',
    },
  });

  await saveRefreshToken(result.refresh_token);
  useSession.getState().setSignedIn({
    accessToken: result.access_token,
    user: toUser(result.user),
    membership: null, // belongs to nothing yet — onboarding is "redeem a code"
  });
}

export async function signIn(input: { email: string; password: string }): Promise<void> {
  const tokens = await request<TokensPayload>('/api/v1/auth/login', {
    method: 'POST',
    authenticated: false,
    body: { email: input.email, password: input.password, device_label: 'mobile' },
  });

  await saveRefreshToken(tokens.refresh_token);
  useSession.getState().setAccessToken(tokens.access_token);

  const me = await request<MePayload>('/api/v1/me');
  useSession.getState().setSignedIn({
    accessToken: tokens.access_token,
    user: toUser(me.user),
    membership: toMembership(me.memberships),
  });

  /*
    Same rule as `restoreSession`: resume an interrupted registration, and only
    if it is THIS account's.

    Signing in does not by itself put anybody in onboarding — that is the bug
    this whole marker exists to fix — so for an established member the read
    below finds nothing and they go straight to the app. It matters only for
    somebody who signed up, joined, and signed out again before choosing.
  */
  const pending = await readPlanChoicePending();
  useSession.getState().setPlanPendingFor(pending === me.user.id ? pending : null);
}

/** Tell the server to revoke this session, then clear local state regardless. */
export async function signOut(): Promise<void> {
  const refreshToken = await readRefreshToken();
  if (refreshToken) {
    try {
      await request<void>('/api/v1/auth/logout', {
        method: 'POST',
        authenticated: false,
        body: { refresh_token: refreshToken },
      });
    } catch {
      // Server-side revocation is best-effort; local sign-out must still happen.
    }
  }
  await signOutLocally();
}

/** Restore a session on cold start. Called once from the root layout. */
export async function restoreSession(): Promise<void> {
  const session = useSession.getState();

  const refreshToken = await readRefreshToken();
  if (!refreshToken) {
    session.setSignedOut();
    return;
  }

  try {
    const tokens = await request<TokensPayload>('/api/v1/auth/refresh', {
      method: 'POST',
      authenticated: false,
      body: { refresh_token: refreshToken, device_label: 'mobile' },
    });

    await saveRefreshToken(tokens.refresh_token);
    session.setAccessToken(tokens.access_token);

    const me = await request<MePayload>('/api/v1/me');
    session.setSignedIn({
      accessToken: tokens.access_token,
      user: toUser(me.user),
      membership: toMembership(me.memberships),
    });

    // Resume an interrupted registration, and only that. The marker is keyed by
    // account, so a device that has seen two people cannot hand one of them the
    // other's half-finished sign-up.
    const pending = await readPlanChoicePending();
    session.setPlanPendingFor(pending === me.user.id ? pending : null);
  } catch {
    await clearRefreshToken();
    session.setSignedOut();
  }
}

// -------------------------------------------------------------------- standing

export type CreatedStaff = Schemas['CreatedStaffResponse'];

/**
 * Create a staff account outright (ADR-0032).
 *
 * The other way round from promoting: instead of waiting for somebody to sign
 * up and walk through the open door, the owner makes the account and the
 * standing together and hands over the password that comes back.
 *
 * **`temporary_password` is shown once.** It is not stored in plaintext, not
 * retrievable, and not in the audit trail — so a screen that displays it must
 * not navigate away before the person has written it down.
 */
export function createStaff(
  gymId: string,
  input: { email: string; displayName: string; capacities: string[] },
): Promise<CreatedStaff> {
  return request<CreatedStaff>(`/api/v1/gyms/${gymId}/staff`, {
    method: 'POST',
    body: {
      email: input.email.trim(),
      display_name: input.displayName.trim(),
      capacities: input.capacities,
    },
  });
}

/**
 * Change your own password (ADR-0032).
 *
 * Every session dies with it, including this one, so the caller is signed out
 * on success — the local session is cleared rather than left holding a refresh
 * token the server has already revoked.
 */
export async function changePassword(current: string, next: string): Promise<void> {
  await request<void>('/api/v1/auth/change-password', {
    method: 'POST',
    body: { current_password: current, new_password: next },
  });
  await signOut();
}

/**
 * Change what somebody holds in this gym (ADR-0031).
 *
 * Send the whole standing, not a delta: `['member']` for a trainer demotes
 * them. That is deliberate — "promote" and "demote" being one call is what
 * stops the two disagreeing.
 *
 * Re-reads `/me` when the person changed is *you*, because your own standing
 * decides which tabs exist and the server is the authority on it. An owner
 * stepping down should not keep a Billing tab until they next restart.
 */
export async function setCapacities(
  gymId: string,
  userId: string,
  capacities: string[],
): Promise<GymMember> {
  const updated = await request<GymMember>(
    `/api/v1/gyms/${gymId}/members/${userId}/capacities`,
    { method: 'PUT', body: { capacities } },
  );
  if (useSession.getState().user?.id === userId) await refreshMembership();
  return updated;
}

// ------------------------------------------------------- auth hardening

/**
 * Ask for a password-reset link (ADR-0029).
 *
 * Always resolves, whether or not the address is registered — the server
 * answers identically either way, so that posting an address cannot be used to
 * discover who trains here. The screen says "if that address has an account…"
 * for the same reason, and must not be "helpfully" changed to say otherwise.
 */
export function requestPasswordReset(email: string): Promise<void> {
  return request<void>('/api/v1/auth/forgot-password', {
    method: 'POST',
    body: { email: email.trim() },
  });
}

/** Set a new password from an emailed link. Signs out every device. */
export function resetPassword(token: string, password: string): Promise<void> {
  return request<void>('/api/v1/auth/reset-password', {
    method: 'POST',
    body: { token: token.trim(), password },
  });
}

/** Send (or resend) a confirmation link to your own address. */
export function sendEmailVerification(): Promise<void> {
  return request<void>('/api/v1/auth/send-verification', { method: 'POST' });
}

export function verifyEmail(token: string): Promise<void> {
  return request<void>('/api/v1/auth/verify-email', {
    method: 'POST',
    body: { token: token.trim() },
  });
}

export type OpenGym = Schemas['OpenGymResponse'];

/**
 * Gyms you can join right now without a code (ADR-0026).
 *
 * The one authenticated read that is not tenant-scoped, because the caller has
 * no gym yet. Returns id, name and slug only — a gym appears here because its
 * owner switched the door on, and the answer says nothing about who trains
 * there.
 */
export function listOpenGyms(): Promise<OpenGym[]> {
  return request<OpenGym[]>('/api/v1/gyms/open');
}

/**
 * Walk through the open door. You become a plain member.
 *
 * Staff standing never comes this way — the server hard-codes the capacity, so
 * there is nothing to pass here and nothing this call could escalate.
 */
export async function joinOpenGym(gymId: string): Promise<void> {
  await request<void>(`/api/v1/gyms/${gymId}/join`, { method: 'POST' });
  await refreshMembership();
}

export type GymSettings = Schemas['GymSettingsResponse'];

export function getGymSettings(gymId: string): Promise<GymSettings> {
  return request<GymSettings>(`/api/v1/gyms/${gymId}/settings`);
}

export function setOpenRegistration(gymId: string, open: boolean): Promise<GymSettings> {
  return request<GymSettings>(`/api/v1/gyms/${gymId}/settings/registration`, {
    method: 'PUT',
    body: { open_registration: open },
  });
}

/** Re-read the account's standing in the gym from the server. */
export async function refreshMembership(): Promise<void> {
  const me = await request<MePayload>('/api/v1/me');
  useSession.getState().setMembership(toMembership(me.memberships));
}

// ------------------------------------------------------------------- coaching

export type CoachRelationship = Schemas['CoachRelationshipResponse'];

/**
 * Coaching relationships the caller may see.
 *
 * The server decides the scope, not us: a manager gets the whole roster, a
 * trainer gets only their own clients, a member gets their coaches. The client
 * renders whatever comes back rather than filtering, because a filter here would
 * be a second copy of a permission rule.
 */
export function listCoachRelationships(gymId: string): Promise<CoachRelationship[]> {
  return request<CoachRelationship[]>(`/api/v1/gyms/${gymId}/coach-relationships`);
}

// --------------------------------------------------- coaching requests (ADR-0025)

export type TrainerDirectoryEntry = Schemas['TrainerDirectoryResponse'];
export type CoachingRequest = Schemas['CoachingRequestResponse'];

/**
 * The gym's coaches, as someone looking for one sees them.
 *
 * Open to every member, and safe to be: it carries only what each coach
 * published about themselves professionally. It is emphatically **not**
 * `listMembers` — no emails, no other member's name. Confusing the two would
 * undo the roster privacy rule the whole design rests on.
 */
export function listTrainers(gymId: string): Promise<TrainerDirectoryEntry[]> {
  return request<TrainerDirectoryEntry[]>(`/api/v1/gyms/${gymId}/trainers`);
}

/**
 * Coaching requests the caller is party to — or all of them, for a manager.
 *
 * Scoped by the server, like `listCoachRelationships`. The client renders what
 * comes back rather than filtering, because a filter here would be a second
 * copy of a permission rule.
 */
export function listCoachingRequests(gymId: string): Promise<CoachingRequest[]> {
  return request<CoachingRequest[]>(`/api/v1/gyms/${gymId}/coaching-requests`);
}

/**
 * Choose a coach. They coach you from that moment (ADR-0031) — there is no
 * acceptance step, so the response comes back already `accepted` and the
 * relationship exists.
 */
export function chooseCoach(
  gymId: string,
  coachId: string,
  message?: string,
): Promise<CoachingRequest> {
  return request<CoachingRequest>(`/api/v1/gyms/${gymId}/coaching-requests`, {
    method: 'POST',
    body: { coach_id: coachId, message: message?.trim() ? message.trim() : null },
  });
}

/**
 * Answer a request raised before ADR-0031 removed the handshake.
 *
 * Nothing in either client offers this any more — nothing can create a pending
 * request. Kept so a deployment upgrading with rows already in flight is not
 * left with records nobody can resolve.
 */
export function answerCoachingRequest(
  gymId: string,
  requestId: string,
  decision: 'accept' | 'decline',
): Promise<CoachingRequest> {
  return request<CoachingRequest>(
    `/api/v1/gyms/${gymId}/coaching-requests/${requestId}/answer`,
    { method: 'POST', body: decision },
  );
}

export function withdrawCoachingRequest(
  gymId: string,
  requestId: string,
): Promise<CoachingRequest> {
  return request<CoachingRequest>(
    `/api/v1/gyms/${gymId}/coaching-requests/${requestId}/withdraw`,
    { method: 'POST' },
  );
}

export type GymMember = Schemas['GymMemberResponse'];

/**
 * Everyone in the gym. Head coaches and above only — the server returns 403
 * otherwise, which is why the screens that use it are capability-gated too.
 */
export function listMembers(gymId: string): Promise<GymMember[]> {
  return request<GymMember[]>(`/api/v1/gyms/${gymId}/members`);
}

/**
 * Propose a coach for a member. Head coaches and above.
 *
 * This replaced pairing them outright (ADR-0034). It comes back **pending**:
 * the named trainer accepts, and accepting is what creates the relationship —
 * because the relationship hands that trainer the member's whole training
 * history and they were never asked. The proposer cannot answer their own
 * proposal, so there is no way to complete it alone.
 */
export function proposeCoach(
  gymId: string,
  input: { coachId: string; athleteId: string; message?: string | null },
): Promise<CoachingRequest> {
  return request<CoachingRequest>(
    `/api/v1/gyms/${gymId}/coaching-requests/propose`,
    {
      method: 'POST',
      body: {
        athlete_id: input.athleteId,
        coach_id: input.coachId,
        message: input.message?.trim() ? input.message : null,
      },
    },
  );
}

/**
 * Stop a coaching pair.
 *
 * End-dated, never deleted: the sessions that coach saw were legitimately seen,
 * and erasing the relationship would make the audit trail lie about why.
 */
export function endCoaching(gymId: string, relationshipId: string): Promise<CoachRelationship> {
  return request<CoachRelationship>(
    `/api/v1/gyms/${gymId}/coach-relationships/${relationshipId}/end`,
    { method: 'POST' },
  );
}

/** One member's athlete profile — self, their coach, or a manager. */
export function getAthleteProfileOf(gymId: string, userId: string): Promise<AthleteProfile> {
  return request<AthleteProfile>(`/api/v1/gyms/${gymId}/members/${userId}/athlete-profile`);
}

/** One member's measurements, same gate as their profile. */
export function getMeasurementsOf(gymId: string, userId: string): Promise<BodyMeasurement[]> {
  return request<BodyMeasurement[]>(`/api/v1/gyms/${gymId}/members/${userId}/measurements`);
}

// ------------------------------------------------------------------ programmes

export type Program = Schemas['ProgramResponse'];
export type ProgramVersion = Schemas['ProgramVersionResponse'];
export type ProgramFocus = Schemas['ProgramFocus'];
export type VersionContent = Schemas['VersionContentResponse'];
export type Prescription = Schemas['ExercisePrescription'];
/** The lifecycle moves. The server decides which are legal from here. */
export type Transition = Schemas['TransitionRequest'];

/** Every programme in the gym, each with its newest version. */
export function listPrograms(gymId: string): Promise<Program[]> {
  return request<Program[]>(`/api/v1/gyms/${gymId}/programs`);
}

/** Start a programme. Creates version 1 as a draft. */
export function createProgram(
  gymId: string,
  input: { name: string; summary?: string | null; focus: ProgramFocus },
): Promise<Program> {
  return request<Program>(`/api/v1/gyms/${gymId}/programs`, {
    method: 'POST',
    body: {
      name: input.name,
      summary: input.summary?.trim() ? input.summary : null,
      focus: input.focus,
    },
  });
}

/** Every version of one programme, newest first. */
export function listVersions(gymId: string, programId: string): Promise<ProgramVersion[]> {
  return request<ProgramVersion[]>(`/api/v1/gyms/${gymId}/programs/${programId}/versions`);
}

/**
 * Edit a published version — by making a new draft of it (ADR-0006).
 *
 * The naming is deliberate: there is no update path, and calling this
 * `updateVersion` would invite one. Existing assignments keep pointing at the
 * version they were given.
 */
export function newDraftFrom(gymId: string, programId: string): Promise<ProgramVersion> {
  return request<ProgramVersion>(`/api/v1/gyms/${gymId}/programs/${programId}/versions`, {
    method: 'POST',
  });
}

/** A version and everything in it — weeks, workouts, prescriptions — in one read. */
export function getVersionContent(gymId: string, versionId: string): Promise<VersionContent> {
  return request<VersionContent>(`/api/v1/gyms/${gymId}/program-versions/${versionId}`);
}

export function addWeek(
  gymId: string,
  versionId: string,
  input: { weekNumber: number; label?: string | null },
): Promise<Schemas['WeekResponse']> {
  return request(`/api/v1/gyms/${gymId}/program-versions/${versionId}/weeks`, {
    method: 'POST',
    body: { week_number: input.weekNumber, label: input.label?.trim() ? input.label : null },
  });
}

export function addWorkout(
  gymId: string,
  weekId: string,
  input: { dayNumber: number; name: string; notes?: string | null },
): Promise<Schemas['WorkoutResponse']> {
  return request(`/api/v1/gyms/${gymId}/program-weeks/${weekId}/workouts`, {
    method: 'POST',
    body: {
      day_number: input.dayNumber,
      name: input.name,
      notes: input.notes?.trim() ? input.notes : null,
    },
  });
}

/**
 * Put an exercise into a workout.
 *
 * The prescription must suit how the exercise is measured — a rep scheme on a
 * distance movement is refused by the server, not merely discouraged.
 */
export function prescribeExercise(
  gymId: string,
  workoutId: string,
  input: { exerciseId: string; prescription: Prescription; notes?: string | null },
): Promise<Schemas['PrescribedExerciseResponse']> {
  return request(`/api/v1/gyms/${gymId}/workout-templates/${workoutId}/exercises`, {
    method: 'POST',
    body: {
      exercise_id: input.exerciseId,
      prescription: input.prescription,
      notes: input.notes?.trim() ? input.notes : null,
    },
  });
}

/**
 * Move a version through its lifecycle.
 *
 * The body is a bare JSON string (`"publish"`), not an object — the server
 * models this as an enum, and the wire shape follows the domain rather than
 * being wrapped for the sake of looking like a form.
 */
export function transitionVersion(
  gymId: string,
  versionId: string,
  move: Transition,
): Promise<ProgramVersion> {
  return request<ProgramVersion>(
    `/api/v1/gyms/${gymId}/program-versions/${versionId}/transition`,
    { method: 'POST', body: move },
  );
}

// ---------------------------------------------------------------- assignments

export type Assignment = Schemas['AssignmentResponse'];

/**
 * Assignments the caller may see — the server scopes: a member gets their own,
 * a trainer their clients', a manager the gym's.
 */
export function listAssignments(gymId: string): Promise<Assignment[]> {
  return request<Assignment[]>(`/api/v1/gyms/${gymId}/program-assignments`);
}

/**
 * Put an athlete on a published version.
 *
 * The version id is pinned, never "latest": a later edit to the programme
 * produces a new version and this assignment keeps pointing at the one the
 * athlete was actually given (ADR-0006).
 */
export function assignProgram(
  gymId: string,
  input: { athleteId: string; programVersionId: string; startDate: string },
): Promise<Assignment> {
  return request<Assignment>(`/api/v1/gyms/${gymId}/program-assignments`, {
    method: 'POST',
    body: {
      athlete_id: input.athleteId,
      program_version_id: input.programVersionId,
      start_date: input.startDate,
    },
  });
}

/** End an assignment. End-dated, never deleted — the history stays true. */
export function withdrawAssignment(gymId: string, assignmentId: string): Promise<Assignment> {
  return request<Assignment>(
    `/api/v1/gyms/${gymId}/program-assignments/${assignmentId}/withdraw`,
    { method: 'POST' },
  );
}

// ------------------------------------------------------------------ execution

export type WorkoutSession = Schemas['SessionResponse'];
export type SessionDetail = Schemas['SessionDetailResponse'];
export type PerformedSetPayload = Schemas['LogSetRequest']['performed'];

/** Sessions the caller may see — own + clients' (server-scoped), newest first. */
export function listSessions(
  gymId: string,
  filter?: { athleteId?: string; from?: string; to?: string; limit?: number },
): Promise<WorkoutSession[]> {
  const params = new URLSearchParams();
  // The filter NARROWS what the server already decided you may see; it never
  // widens it. Asking about someone who is not yours returns an empty list,
  // not a 403 — the scoping happens before this ever applies.
  if (filter?.athleteId) params.set('athlete_id', filter.athleteId);
  if (filter?.from) params.set('from', filter.from);
  if (filter?.to) params.set('to', filter.to);
  if (filter?.limit != null) params.set('limit', String(filter.limit));

  const query = params.toString();
  return request<WorkoutSession[]>(
    `/api/v1/gyms/${gymId}/workout-sessions${query ? `?${query}` : ''}`,
  );
}

export function getSessionDetail(gymId: string, sessionId: string): Promise<SessionDetail> {
  return request<SessionDetail>(`/api/v1/gyms/${gymId}/workout-sessions/${sessionId}`);
}

/**
 * Start a session. The id is minted HERE, on the device (ADR-0008) — the server
 * treats a replay of the same id as a no-op, which is what makes this safe to
 * retry on a flaky connection.
 *
 * Two shapes, and the server refuses anything between them (ADR-0035):
 *
 * - **Assigned** — pass `assignmentId` and `workoutTemplateId` together. The
 *   session executes a prescription and is named by its workout.
 * - **Unplanned** — pass neither, and optionally a `title`. This is what a
 *   member on an Open Gym membership starts: no coach, so no prescription, so
 *   nothing to execute but their own choices.
 */
export function startSession(
  gymId: string,
  input:
    | { id: string; assignmentId: string; workoutTemplateId: string }
    | { id: string; title?: string | null },
): Promise<WorkoutSession> {
  const planned = 'assignmentId' in input;
  return request<WorkoutSession>(`/api/v1/gyms/${gymId}/workout-sessions`, {
    method: 'POST',
    body: {
      id: input.id,
      // Sent as nulls rather than omitted so the request is the same shape
      // either way — a body whose keys change with the branch is the kind of
      // thing that breaks when somebody later adds a field.
      assignment_id: planned ? input.assignmentId : null,
      workout_template_id: planned ? input.workoutTemplateId : null,
      title: planned ? null : (input.title?.trim() || null),
      started_at: new Date().toISOString(),
    },
  });
}

export function logSet(
  gymId: string,
  sessionId: string,
  input: {
    id: string;
    exerciseId: string;
    templateExerciseId?: string | null;
    setNumber: number;
    performed: PerformedSetPayload;
    rpe?: number | null;
  },
): Promise<Schemas['PerformedSetResponse']> {
  return request<Schemas['PerformedSetResponse']>(
    `/api/v1/gyms/${gymId}/workout-sessions/${sessionId}/sets`,
    {
      method: 'POST',
      body: {
        id: input.id,
        exercise_id: input.exerciseId,
        template_exercise_id: input.templateExerciseId ?? null,
        set_number: input.setNumber,
        performed: input.performed,
        rpe: input.rpe ?? null,
      },
    },
  );
}

/**
 * Finish a session, sending the athlete's OWN clock for the end time.
 *
 * The server records when it heard, but durations are computed from
 * `started_at`/`ended_at` — both from this device. Without this, a workout
 * finished offline and synced hours later reports the sync delay as training
 * time, and every average a coach reads is wrong.
 *
 * A value the server cannot believe is dropped there, not rejected: closing a
 * workout must never fail because a phone's clock drifted.
 */
export function finishSession(
  gymId: string,
  sessionId: string,
  outcome: 'completed' | 'abandoned',
): Promise<WorkoutSession> {
  return request<WorkoutSession>(
    `/api/v1/gyms/${gymId}/workout-sessions/${sessionId}/finish`,
    { method: 'POST', body: { outcome, ended_at: new Date().toISOString() } },
  );
}

export type ExerciseHistoryEntry = Schemas['ExerciseHistoryEntryResponse'];

/**
 * Every set of one exercise, grouped by session, oldest first. Your own by
 * default; a coach passes their client's athlete id.
 */
export function getExerciseHistory(
  gymId: string,
  exerciseId: string,
  athleteId?: string,
): Promise<ExerciseHistoryEntry[]> {
  const query = athleteId ? `?athlete_id=${athleteId}` : '';
  return request<ExerciseHistoryEntry[]>(
    `/api/v1/gyms/${gymId}/exercises/${exerciseId}/history${query}`,
  );
}

// ------------------------------------------------------------------- profiles

export type MyProfiles = Schemas['MyProfilesResponse'];
export type AthleteProfile = Schemas['AthleteProfileResponse'];
export type TrainerProfile = Schemas['TrainerProfileResponse'];

/** Both profiles. `null` means never filled in — an invitation, not an error. */
export function getMyProfiles(): Promise<MyProfiles> {
  return request<MyProfiles>('/api/v1/me/profiles');
}

/** Full replace — omitted fields clear, so the form submits everything. */
export function updateAthleteProfile(
  input: Schemas['UpdateAthleteProfileRequest'],
): Promise<AthleteProfile> {
  return request<AthleteProfile>('/api/v1/me/profiles/athlete', {
    method: 'PUT',
    body: input,
  });
}

export function updateTrainerProfile(
  input: Schemas['UpdateTrainerProfileRequest'],
): Promise<TrainerProfile> {
  return request<TrainerProfile>('/api/v1/me/profiles/trainer', {
    method: 'PUT',
    body: input,
  });
}

/** Rename the account, and reflect it in the session immediately. */
export async function renameMe(displayName: string): Promise<void> {
  const result = await request<{ display_name: string }>('/api/v1/me', {
    method: 'PATCH',
    body: { display_name: displayName },
  });
  const { user, setUser } = {
    user: useSession.getState().user,
    setUser: useSession.getState().setUser,
  };
  if (user) setUser({ ...user, displayName: result.display_name });
}

// ------------------------------------------------------------- recommendations

export type Recommendations = Schemas['RecommendationsResponse'];

/**
 * Deterministic suggestions derived from YOUR active goals, each carrying its
 * reason. Empty when you have no goals — the UI renders that as an invitation.
 */
export function getRecommendations(gymId: string): Promise<Recommendations> {
  return request<Recommendations>(`/api/v1/gyms/${gymId}/recommendations`);
}

// ---------------------------------------------------------------------- goals

export type Goal = Schemas['GoalResponse'];
export type GoalMetric = Schemas['GoalMetric'];

/** Goals the caller may see — server-scoped like assignments and sessions. */
export function listGoals(gymId: string): Promise<Goal[]> {
  return request<Goal[]>(`/api/v1/gyms/${gymId}/goals`);
}

/**
 * Set a goal.
 *
 * Uniquely self-service: a member may set their own, which no other write in
 * this app allows. The baseline is captured by the SERVER at creation from the
 * athlete's real history — passing it from here would let a client invent the
 * starting point and with it the progress.
 */
export function createGoal(
  gymId: string,
  input: { athleteId: string; metric: GoalMetric; targetDate?: string | null },
): Promise<Goal> {
  return request<Goal>(`/api/v1/gyms/${gymId}/goals`, {
    method: 'POST',
    body: {
      athlete_id: input.athleteId,
      metric: input.metric,
      target_date: input.targetDate ?? null,
    },
  });
}

/** Close a goal, one way or the other. Both are outcomes; neither is a delete. */
export function closeGoal(
  gymId: string,
  goalId: string,
  outcome: 'achieved' | 'abandoned',
): Promise<Goal> {
  return request<Goal>(`/api/v1/gyms/${gymId}/goals/${goalId}/close`, {
    method: 'POST',
    body: outcome,
  });
}

// --------------------------------------------------------------- measurements

export type BodyMeasurement = Schemas['MeasurementResponse'];

export function listMeasurements(): Promise<BodyMeasurement[]> {
  return request<BodyMeasurement[]>('/api/v1/me/measurements');
}

/** Upsert by date — re-entering the same morning replaces it. */
export function saveMeasurement(
  date: string,
  input: Schemas['SaveMeasurementRequest'],
): Promise<BodyMeasurement> {
  return request<BodyMeasurement>(`/api/v1/me/measurements/${date}`, {
    method: 'PUT',
    body: input,
  });
}

export function deleteMeasurement(date: string): Promise<void> {
  return request<void>(`/api/v1/me/measurements/${date}`, { method: 'DELETE' });
}

// ---------------------------------------------------------------- audit trail

export type AuditEntry = Schemas['AuditEntryResponse'];

/**
 * The gym's audit trail. Restricted to people who manage the gym — the server
 * returns 403 otherwise, which is why the tab is capacity-gated too.
 */
export function listAudit(gymId: string): Promise<AuditEntry[]> {
  return request<AuditEntry[]>(`/api/v1/gyms/${gymId}/audit`);
}

// -------------------------------------------------------------------- billing

export type Plan = Schemas['PlanResponse'];
export type Subscription = Schemas['SubscriptionResponse'];
export type Invoice = Schemas['InvoiceResponse'];
export type PaymentRecord = Schemas['PaymentResponse'];
export type Feature = Schemas['Feature'];
export type MyEntitlements = Schemas['MyEntitlementsResponse'];

/** What the gym charges. Readable by anyone in it — prices are not a secret. */
export function listPlans(gymId: string): Promise<Plan[]> {
  return request<Plan[]>(`/api/v1/gyms/${gymId}/plans`);
}

export function createPlan(
  gymId: string,
  input: {
    name: string;
    description?: string | null;
    priceMinor: number;
    currency: string;
    interval: 'monthly' | 'once';
    /** What the plan confers. The server refuses an empty list. */
    grants: Feature[];
  },
): Promise<Plan> {
  return request<Plan>(`/api/v1/gyms/${gymId}/plans`, {
    method: 'POST',
    body: {
      name: input.name,
      description: input.description?.trim() ? input.description : null,
      price_minor: input.priceMinor,
      currency: input.currency,
      interval: input.interval,
      grants: input.grants,
    },
  });
}

export function archivePlan(gymId: string, planId: string): Promise<void> {
  return request<void>(`/api/v1/gyms/${gymId}/plans/${planId}`, { method: 'DELETE' });
}

/** The gym's for a manager; your own otherwise — the server narrows it. */
export function listSubscriptions(gymId: string): Promise<Subscription[]> {
  return request<Subscription[]>(`/api/v1/gyms/${gymId}/subscriptions`);
}

export type SubscribeResult = Schemas['SubscribeResponse'];

/**
 * End a subscription.
 *
 * A member may end their OWN, any time — which is what makes "coached to solo"
 * a thing somebody can do at 11pm on their phone rather than by catching a
 * manager. Access runs to the end of the period already paid for; the server
 * computes that date and returns it on `status`.
 *
 * Cancelling is NOT leaving the gym: membership standing is untouched, so
 * subscribing to a solo plan afterwards is the whole downgrade.
 */
export function cancelSubscription(
  gymId: string,
  subscriptionId: string,
): Promise<Subscription> {
  return request<Subscription>(
    `/api/v1/gyms/${gymId}/subscriptions/${subscriptionId}`,
    { method: 'DELETE' },
  );
}

/**
 * Put a member on a plan.
 *
 * Returns BOTH the subscription and the first invoice — subscribing creates
 * two things, and a caller that only got the invoice back could not then refer
 * to the arrangement it had just set up.
 */
/**
 * Put somebody on a plan.
 *
 * A manager may subscribe anybody; a member may subscribe **themselves** to a
 * plan the gym currently offers (ADR-0031). The server decides which of those
 * applies — this is one call either way.
 */
export function subscribeMember(
  gymId: string,
  input: { memberId: string; planId: string; startedOn: string },
): Promise<SubscribeResult> {
  return request<SubscribeResult>(`/api/v1/gyms/${gymId}/subscriptions`, {
    method: 'POST',
    body: { member_id: input.memberId, plan_id: input.planId, started_on: input.startedOn },
  });
}

export function listInvoices(gymId: string): Promise<Invoice[]> {
  return request<Invoice[]>(`/api/v1/gyms/${gymId}/invoices`);
}

export function voidInvoice(
  gymId: string,
  invoiceId: string,
  reason?: string,
): Promise<Invoice> {
  return request<Invoice>(`/api/v1/gyms/${gymId}/invoices/${invoiceId}/void`, {
    method: 'POST',
    body: { reason: reason?.trim() ? reason : null },
  });
}

export function listPayments(gymId: string, invoiceId: string): Promise<PaymentRecord[]> {
  return request<PaymentRecord[]>(`/api/v1/gyms/${gymId}/invoices/${invoiceId}/payments`);
}

export function recordPayment(
  gymId: string,
  invoiceId: string,
  input: {
    amountMinor: number;
    provider: 'cash' | 'card_terminal' | 'stripe';
    receivedOn: string;
    note?: string;
  },
): Promise<PaymentRecord> {
  return request<PaymentRecord>(`/api/v1/gyms/${gymId}/invoices/${invoiceId}/payments`, {
    method: 'POST',
    body: {
      amount_minor: input.amountMinor,
      provider: input.provider,
      received_on: input.receivedOn,
      note: input.note?.trim() ? input.note : null,
    },
  });
}

/**
 * What the signed-in person may use in this gym, and why.
 *
 * Each entry carries a printable reason, so a screen that has to refuse
 * something can name the membership that would allow it instead of saying
 * "not permitted" and leaving the member to guess.
 */
export function myEntitlements(gymId: string): Promise<MyEntitlements> {
  return request<MyEntitlements>(`/api/v1/gyms/${gymId}/entitlements/me`);
}

/**
 * Start a Stripe Checkout for an invoice's outstanding balance — self-service
 * payment (ADR-0010's seam, finally used). Nothing about the invoice changes
 * yet: opening `checkout_url` and completing payment there is what does that,
 * via a webhook the server verifies independently.
 *
 * `returnUrl` is `Linking.createURL(...)`'s job, not this function's — native
 * and web resolve to different kinds of URL, and only the app knows which one
 * it is running as.
 */
export function createCheckoutSession(
  gymId: string,
  invoiceId: string,
  returnUrl: string,
): Promise<{ checkout_url: string }> {
  return request<{ checkout_url: string }>(
    `/api/v1/gyms/${gymId}/invoices/${invoiceId}/checkout`,
    { method: 'POST', body: { return_url: returnUrl } },
  );
}

// ------------------------------------------------------------------ checkins

export type CheckIn = Schemas['CheckInResponse'];

/**
 * A short-lived pass to show at the door. Opaque — this app never decodes
 * it, only renders it and re-fetches a fresh one before it expires.
 */
export function getEntryPass(
  gymId: string,
): Promise<{ token: string; expires_in_seconds: number }> {
  return request<{ token: string; expires_in_seconds: number }>(
    `/api/v1/gyms/${gymId}/checkins/my-pass`,
    { method: 'POST' },
  );
}

export type ScanResult = Schemas['ScanResponse'];

/** Staff scans a pass. Always resolves — `allowed` carries the outcome. */
export function scanEntry(gymId: string, token: string): Promise<ScanResult> {
  return request<ScanResult>(`/api/v1/gyms/${gymId}/checkins/scan`, {
    method: 'POST',
    body: { token },
  });
}

/** The door's recent history — staff-only, newest first. */
export function listCheckins(gymId: string): Promise<CheckIn[]> {
  return request<CheckIn[]>(`/api/v1/gyms/${gymId}/checkins`);
}

// ------------------------------------------------------------------ exercises

export function listExercises(gymId: string): Promise<Exercise[]> {
  return request<Exercise[]>(`/api/v1/gyms/${gymId}/exercises`);
}

export function createExercise(
  gymId: string,
  input: { name: string; modality: ModalityKind; notes?: string },
): Promise<Exercise> {
  return request<Exercise>(`/api/v1/gyms/${gymId}/exercises`, {
    method: 'POST',
    body: {
      name: input.name,
      modality: { kind: input.modality },
      notes: input.notes?.trim() ? input.notes : null,
    },
  });
}

/**
 * Decide a proposed movement's standing (ADR-0024).
 *
 * `reinstate` exists on the server too, but no screen offers it yet — a
 * retired movement is not listed anywhere a curator could act on it. Left out
 * of this signature deliberately rather than exported unused: the day the
 * retired list gets a screen is the day to widen it.
 */
export function curateExercise(
  gymId: string,
  exerciseId: string,
  decision: 'approve' | 'retire',
): Promise<Exercise> {
  return request<Exercise>(`/api/v1/gyms/${gymId}/exercises/${exerciseId}/curate`, {
    method: 'POST',
    body: { decision },
  });
}

// ------------------------------------------------------------------- classes

export type ClassOccurrence = Schemas['ClassOccurrenceResponse'];
export type GymClass = Schemas['ClassResponse'];
export type ClassBooking = Schemas['BookingResponse'];
export type ClassRosterEntry = Schemas['RosterEntry'];

/**
 * The timetable for a date window.
 *
 * A class is a weekly slot and the server derives the dated sittings, so this
 * returns one row per class per occurrence — the same class appears once for
 * each week in the window. `booked_by_me` is resolved server-side, so the
 * Book/Cancel decision needs no second request.
 *
 * Dates are `YYYY-MM-DD` wall-clock in the gym's own zone, never instants.
 */
export function listClasses(
  gymId: string,
  from: string,
  to: string,
): Promise<ClassOccurrence[]> {
  return request<ClassOccurrence[]>(
    `/api/v1/gyms/${gymId}/classes?from=${from}&to=${to}`,
  );
}

/** Put a class on the timetable. Owners and admins. */
export function createClass(
  gymId: string,
  input: {
    name: string;
    instructorId: string;
    weekday: number;
    startsAt: string;
    durationMinutes: number;
    capacity: number;
    description?: string | null;
  },
): Promise<GymClass> {
  return request<GymClass>(`/api/v1/gyms/${gymId}/classes`, {
    method: 'POST',
    body: {
      name: input.name.trim(),
      instructor_id: input.instructorId,
      weekday: input.weekday,
      starts_at: input.startsAt,
      duration_minutes: input.durationMinutes,
      capacity: input.capacity,
      description: input.description?.trim() ? input.description : null,
    },
  });
}

/** Take a class off the timetable — archived, so old rosters stay readable. */
export function archiveClass(gymId: string, classId: string): Promise<GymClass> {
  return request<GymClass>(`/api/v1/gyms/${gymId}/classes/${classId}`, {
    method: 'DELETE',
  });
}

/**
 * Take a place in one sitting.
 *
 * `bookingId` is minted on the device (ADR-0008) so a retry after a network
 * wobble replays the same booking rather than taking a second place — the same
 * contract as logging a set.
 */
export function bookClass(
  gymId: string,
  classId: string,
  input: { bookingId: string; onDate: string },
): Promise<ClassBooking> {
  return request<ClassBooking>(
    `/api/v1/gyms/${gymId}/classes/${classId}/bookings`,
    { method: 'POST', body: { id: input.bookingId, on_date: input.onDate } },
  );
}

/** Give a place back. Allowed until the class starts. */
export function cancelClassBooking(
  gymId: string,
  bookingId: string,
): Promise<ClassBooking> {
  return request<ClassBooking>(
    `/api/v1/gyms/${gymId}/class-bookings/${bookingId}`,
    { method: 'DELETE' },
  );
}

/** Who is in one sitting. The class's own instructor, or a manager. */
export function getClassRoster(
  gymId: string,
  classId: string,
  onDate: string,
): Promise<ClassRosterEntry[]> {
  return request<ClassRosterEntry[]>(
    `/api/v1/gyms/${gymId}/classes/${classId}/roster?on_date=${onDate}`,
  );
}
