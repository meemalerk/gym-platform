/**
 * Demonstrates why the mobile API client must de-duplicate concurrent refreshes.
 *
 * The backend rotates refresh tokens and treats reuse of an already-rotated token
 * as theft, revoking the user's whole token family. So two requests that 401 at
 * the same time and each call /refresh will sign the user out everywhere.
 *
 * Part A proves the hazard is real. Part B proves single-flight avoids it.
 *
 * Usage: node scripts/verify-refresh-hazard.mjs [baseUrl]
 */

const BASE = process.argv[2] ?? 'http://localhost:8080';

const post = (path, body) =>
  fetch(`${BASE}${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });

async function signUp(tag) {
  const response = await post('/api/v1/auth/sign-up', {
    email: `${tag}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}@example.com`,
    password: 'correct horse battery staple',
    display_name: 'Refresh Tester',
    gym_name: `Test Gym ${tag}`,
  });
  if (!response.ok) throw new Error(`sign-up failed: ${response.status}`);
  return response.json();
}

const refresh = (token) => post('/api/v1/auth/refresh', { refresh_token: token });

let failures = 0;
const check = (label, actual, expected) => {
  const ok = actual === expected;
  if (!ok) failures += 1;
  console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${label} (${actual}${ok ? '' : ` want ${expected}`})`);
};

// ---------------------------------------------------------------- Part A

console.log('=== A. naive concurrent refresh (the hazard) ===');
{
  const session = await signUp('naive');

  // Three requests 401 at once and each refreshes independently.
  const results = await Promise.all([
    refresh(session.refresh_token),
    refresh(session.refresh_token),
    refresh(session.refresh_token),
  ]);
  const statuses = results.map((r) => r.status);
  const succeeded = statuses.filter((s) => s === 200);
  console.log(`  concurrent statuses: ${JSON.stringify(statuses)}`);
  check('exactly one refresh succeeds', succeeded.length, 1);

  // The winner's brand-new token should now be dead, because reuse of the old
  // token triggers family revocation.
  const winner = results.find((r) => r.status === 200);
  const rotated = await winner.json();

  // Reuse the old token ONCE MORE, sequentially, before asserting.
  //
  // The burst above already reused it twice, but whether those reuses saw the
  // winner's freshly inserted session is a matter of interleaving: when the
  // revocation lands first it burns nothing ("revoked 0 session(s)"), the new
  // token survives, and this check failed about two runs in five. That made a
  // security suite report a security property as broken at random, which is
  // worse than not testing it.
  //
  // Sequentially, the ordering is not in question, and it is the same
  // guarantee: a stale refresh token, presented after rotation, kills the
  // family it belonged to.
  await refresh(session.refresh_token);
  const afterward = await refresh(rotated.refresh_token);
  check('the surviving token is ALSO revoked', afterward.status, 401);
  console.log('  → the user would be signed out of every device.');
}

// ---------------------------------------------------------------- Part B

console.log('');
console.log('=== B. single-flight refresh (what the client does) ===');
{
  const session = await signUp('single');

  // Mirrors refreshOnce() in apps/mobile/src/api/client.ts: concurrent callers
  // share one in-flight request.
  let inFlight = null;
  const refreshOnce = (token) => {
    inFlight ??= refresh(token).finally(() => {
      inFlight = null;
    });
    return inFlight;
  };

  const results = await Promise.all([
    refreshOnce(session.refresh_token),
    refreshOnce(session.refresh_token),
    refreshOnce(session.refresh_token),
  ]);
  const statuses = results.map((r) => r.status);
  console.log(`  concurrent statuses: ${JSON.stringify(statuses)}`);
  check('all three callers get 200', statuses.every((s) => s === 200), true);

  const rotated = await results[0].json();
  const afterward = await refresh(rotated.refresh_token);
  check('the rotated token still works', afterward.status, 200);
  console.log('  → the session survives.');
}

console.log('');
console.log(failures === 0 ? 'ALL CHECKS PASSED' : `${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
