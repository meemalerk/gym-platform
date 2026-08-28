import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { NavLink, Navigate, Route, Routes } from 'react-router-dom';

import { api, isSignedIn, setSignedOutHandler, signOut, type Me } from './lib/api';
import { SignIn } from './routes/sign-in';
import { Overview } from './routes/overview';
import { Billing } from './routes/billing';
import { People } from './routes/people';
import { Catalogue } from './routes/catalogue';
import { Activity } from './routes/activity';
import { Settings } from './routes/settings';

/**
 * The console shell.
 *
 * Who this is for, and why it exists alongside the phone app: owners and head
 * coaches do the work a phone is worst at — reading a billing ledger, working
 * a roster, comparing a month of attendance. The five-tab ceiling in the app
 * (`navigation/tabs.ts`) already forced Billing to displace Library for
 * managers; that is a symptom of running a back office on a phone.
 *
 * The app is not deprecated by this. It stays the member and floor-trainer
 * surface, which is the right shape for logging a set between rounds.
 */
export function App() {
  // `isSignedIn()` reads a token from storage, so a reload lands signed in.
  const [signedIn, setSignedIn] = useState(isSignedIn);

  useEffect(() => {
    // The API client signs out when a refresh fails. Without this the shell
    // would keep rendering a dashboard whose every query 401s.
    setSignedOutHandler(() => setSignedIn(false));
  }, []);

  if (!signedIn) return <SignIn onSignedIn={() => setSignedIn(true)} />;
  return <Shell onSignOut={() => setSignedIn(false)} />;
}

function Shell({ onSignOut }: { onSignOut: () => void }) {
  const me = useQuery({ queryKey: ['me'], queryFn: api.me });

  if (me.isLoading) {
    return (
      <div className="centred">
        <p className="muted">Loading…</p>
      </div>
    );
  }

  const membership = me.data?.memberships?.[0];

  // An account with no gym has nothing to manage. Rather than an empty shell,
  // say so — this is the case where somebody signed in with a member account
  // that was never given staff standing.
  if (!membership) {
    return (
      <div className="centred">
        <div className="card">
          <h1>No gym yet</h1>
          <p className="lede">
            This account does not belong to a gym. Join one in the phone app — whoever runs
            it can then make you staff, and this console will let you in.
          </p>
          <button
            type="button"
            className="ghost"
            style={{ marginTop: 20 }}
            onClick={() => {
              signOut();
              onSignOut();
            }}
          >
            Sign out
          </button>
        </div>
      </div>
    );
  }

  const capacities = membership.capacities;
  /*
    Two rights, one rung.

    ADR-0036 removed `admin` and `head_coach`, so both of these are "is an
    owner" today. They stay as two names because they gate different things —
    `manages` is billing and settings, `curates` is the catalogue — and a
    future rung between owner and trainer would separate them again. Collapsing
    them now would mean re-deriving which tab meant which later.
  */
  const manages = capacities.includes('owner');
  const curates = manages;

  return (
    <div className="shell">
      <aside className="rail">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true" />
          <span className="brand-text">
            <span className="brand-name">{membership.gym_name}</span>
            <span className="brand-kicker">Console</span>
          </span>
        </div>

        {/*
          Grouped, because the two halves are different jobs: "today" is the
          morning triage, "the gym" is the standing record you go and look
          something up in. Ungrouped, six flat links make the reader re-read
          the list every time.
        */}
        <nav>
          <span className="nav-group">Today</span>
          <Tab to="/" label="Overview" />
          {/* A coach gets their own clients here, not the gym roster — the
              label says which, because "People" showing three names reads as a
              broken roster rather than as your client list. */}
          <Tab to="/people" label={manages ? 'People' : 'Your clients'} />
          {manages ? <Tab to="/billing" label="Billing" /> : null}

          <span className="nav-group">The gym</span>
          {curates ? <Tab to="/catalogue" label="Catalogue" /> : null}
          {manages ? <Tab to="/activity" label="Activity" /> : null}
          {manages ? <Tab to="/settings" label="Settings" /> : null}
        </nav>

        <div className="foot">
          <span className="whoami">
            {me.data?.user.display_name}
            <small>{me.data?.user.email}</small>
          </span>
          <button
            type="button"
            className="ghost small"
            style={{ marginTop: 10 }}
            onClick={() => {
              signOut();
              onSignOut();
            }}
          >
            Sign out
          </button>
        </div>
      </aside>

      <main>
        <div className="page">
          <Routes>
            <Route path="/" element={<Overview gym={membership.gym_id} />} />
            <Route path="/people" element={<People gym={membership.gym_id} me={me.data} />} />
            {/*
              Routes are registered only when the capacity allows, so a hand-typed
              URL falls through to the redirect rather than rendering a page whose
              every request 403s. Same principle as the app's `Tabs.Protected`:
              hidden and unreachable must mean the same thing.
            */}
            {curates ? (
              <Route path="/catalogue" element={<Catalogue gym={membership.gym_id} />} />
            ) : null}
            {manages ? (
              <Route path="/billing" element={<Billing gym={membership.gym_id} />} />
            ) : null}
            {manages ? (
              <Route path="/activity" element={<Activity gym={membership.gym_id} />} />
            ) : null}
            {manages ? (
              <Route path="/settings" element={<Settings gym={membership.gym_id} />} />
            ) : null}
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </div>
      </main>
    </div>
  );
}

function Tab({ to, label }: { to: string; label: string }) {
  return (
    <NavLink to={to} end={to === '/'} className={({ isActive }) => (isActive ? 'active' : '')}>
      {label}
    </NavLink>
  );
}

export type { Me };
