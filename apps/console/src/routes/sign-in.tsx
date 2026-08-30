import { useState } from 'react';

import { DEMO_ACCOUNTS, DEMO_PASSWORD, SHOW_DEMO_ACCOUNTS } from '../dev/demo-accounts';
import { ApiError, signIn } from '../lib/api';

/**
 * The one screen in the console that is allowed to have a voice.
 *
 * A split: the product's claim on the left, the form on the right. Everywhere
 * past this point the console is a tool and says nothing about itself; here it
 * is the only thing on screen, and a centred card floating on grey would waste
 * the one moment there is to look like something.
 */
export function SignIn({ onSignedIn }: { onSignedIn: () => void }) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // One path, so a demo row cannot quietly get worse error handling than the
  // form beside it - the failure that matters most here is "the API is not
  // running", and it is worth naming wherever you press.
  async function attempt(emailValue: string, passwordValue: string) {
    setError(null);
    setBusy(true);
    try {
      await signIn(emailValue.trim(), passwordValue);
      onSignedIn();
    } catch (e) {
      setError(
        e instanceof ApiError && e.code === 'auth.too_many_attempts'
          ? // A distinct code, because the advice is the opposite: telling
            // somebody to try again when trying is the problem is unhelpful.
            'Too many attempts. Wait a few minutes, or reset your password in the app.'
          : e instanceof ApiError && e.code === 'network.unreachable'
            ? // Name the actual fault. "Incorrect email or password" here costs
              // somebody an hour resetting a password that was never wrong.
              'Cannot reach the server. Check that the API is running, then try again.'
            : e instanceof ApiError && e.code === 'auth.unauthenticated'
              ? 'Incorrect email or password.'
              : 'Could not sign in. Please try again.',
      );
    } finally {
      setBusy(false);
    }
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    await attempt(email, password);
  }

  return (
    <div className="signin">
      <section className="signin-panel">
        <span className="mark" aria-hidden="true" />
        <div>
          <h1>The floor, from a desk.</h1>
          <p>
            Ledgers, rosters and the review queue — the work the phone app is the wrong shape
            for.
          </p>
        </div>
        <span className="foot">Gym Platform · Console</span>
      </section>

      <section className="signin-form">
        <form onSubmit={submit}>
          <h2>Sign in</h2>

          {error ? <p className="banner">{error}</p> : null}

          <div className="field">
            <label htmlFor="email">Email</label>
            <input
              id="email"
              type="email"
              autoComplete="username"
              placeholder="you@example.com"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              required
            />
          </div>

          <div className="field">
            <label htmlFor="password">Password</label>
            <input
              id="password"
              type="password"
              autoComplete="current-password"
              placeholder="••••••••••••"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
            />
          </div>

          <button type="submit" disabled={busy}>
            {busy ? 'Signing in…' : 'Sign in'}
          </button>

          {/* Reset lives in the app, not here: duplicating the flow would mean
              two screens to keep saying the same careful non-committal thing
              about whether an address exists (ADR-0029). */}
          <p className="muted" style={{ fontSize: 12.5, marginBottom: 0, marginTop: 18 }}>
            Staff accounts only. Forgotten your password? Reset it from the phone app.
          </p>

          {/* The credentials, where somebody signing in is actually looking.
              They were only ever in docs/test-accounts.md, so `npm run dev`
              opened a form with nothing to type into it. Dev builds only
              (SHOW_DEMO_ACCOUNTS) — vite build strips this entirely. */}
          {SHOW_DEMO_ACCOUNTS ? (
            <div className="demo">
              <p className="demo-head">
                Demo accounts <span className="demo-tag">Dev only</span>
              </p>
              {DEMO_ACCOUNTS.map((account) => (
                <button
                  key={account.email}
                  type="button"
                  className="demo-row"
                  disabled={busy}
                  onClick={() => attempt(account.email, DEMO_PASSWORD)}
                  aria-label={`Sign in as ${account.label}. ${account.hint}.`}
                >
                  <span className="demo-body">
                    <span className="demo-label">{account.label}</span>
                    <span className="demo-hint">{account.hint}</span>
                  </span>
                  <span className="demo-go">Sign in</span>
                </button>
              ))}
              <p className="demo-foot">
                Seeded by <code>scripts/seed-demo.sh</code>; every account{"'"}s password is{' '}
                <code>{DEMO_PASSWORD}</code>. Members sign in on the phone app — see
                docs/test-accounts.md.
              </p>
            </div>
          ) : null}
        </form>
      </section>
    </div>
  );
}
