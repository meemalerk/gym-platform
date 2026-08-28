/**
 * The seeded demo accounts, for one-tap sign-in.
 *
 * Typing `member@demo.test` and a twelve-character password on a phone,
 * repeatedly, to check what one capacity sees is the kind of friction that
 * quietly stops you from checking. The password is the one
 * `scripts/seed-demo.sh` sets: weak on purpose, only ever used against a local
 * server.
 *
 * Four accounts — owner, two trainers, member — each holding exactly one
 * capacity at the one gym this deployment serves (ADR-0023). Keep in step
 * with docs/test-accounts.md.
 *
 * The SECOND trainer is not padding. Under ADR-0034 a trainer may only
 * prescribe for their OWN clients, and the roster is readable by the class's
 * own instructor and nobody else — rules you cannot see working with a single
 * trainer, because there is no second party for them to be refused against.
 */

/**
 * Whether to offer the one-tap buttons at all.
 *
 * `__DEV__` covers day-to-day development and is compile-time, so the accounts
 * and their password are stripped from a release bundle entirely.
 *
 * The env flag is the **deliberate** second door, and exists for exactly one
 * caller: `demo/Dockerfile.web`, which builds the browser demo someone
 * non-technical is meant to click through. An export is a production build, so
 * `__DEV__` is false there and the buttons would vanish — leaving a viewer
 * staring at a login form with no credentials, which is the whole problem the
 * buttons solve.
 *
 * It stays safe because it is **opt-in and build-time**: `EXPO_PUBLIC_*` values
 * are inlined by the bundler, so a normal `eas build` — which sets nothing —
 * dead-strips this branch exactly as before. Turning it on is a visible edit to
 * a build command, never a runtime toggle an attacker could flip.
 */
export const SHOW_DEMO_ACCOUNTS =
  __DEV__ || process.env.EXPO_PUBLIC_DEMO_ACCOUNTS === 'true';

export type DemoAccount = {
  email: string;
  /** What this account is for, in two or three words. */
  label: string;
  /** The one thing worth checking as this account. */
  hint: string;
};

export const DEMO_PASSWORD = 'demopassword';

export const DEMO_ACCOUNTS: DemoAccount[] = [
  {
    email: 'owner@demo.test',
    label: 'Owner',
    hint: 'Everything, including invites',
  },
  {
    email: 'trainer@demo.test',
    label: 'Trainer',
    hint: 'Their own clients only',
  },
  {
    email: 'trainer2@demo.test',
    label: 'Trainer 2',
    hint: 'The other trainer — proves the boundary',
  },
  {
    email: 'member@demo.test',
    label: 'Member',
    hint: 'Programmes, goals, an open workout',
  },
];
