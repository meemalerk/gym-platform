import { useState } from 'react';

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

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await signIn(email.trim(), password);
      onSignedIn();
    } catch (e) {
      setError(
        e instanceof ApiError && e.code === 'auth.too_many_attempts'
          ? // A distinct code, because the advice is the opposite: telling
            // somebody to try again when trying is the problem is unhelpful.
            'Too many attempts. Wait a few minutes, or reset your password in the app.'
          : e instanceof ApiError && e.code === 'auth.unauthenticated'
            ? 'Incorrect email or password.'
            : 'Could not sign in. Please try again.',
      );
    } finally {
      setBusy(false);
    }
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
        </form>
      </section>
    </div>
  );
}
