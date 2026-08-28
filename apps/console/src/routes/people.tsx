import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import { ApiError, api, type CreatedStaff, type Me } from '../lib/api';
import { Card, Chip, Empty, PageHead, SectionHead, Segmented, Stat, TableCard } from '../ui';

/**
 * The capability ladder, senior first, with what each rung unlocks.
 *
 * Written out rather than left as bare capacity names, because a role name is
 * not self-explanatory to an owner opening this for the first time.
 *
 * Three rungs since ADR-0036, down from five. `admin` and `head_coach` were
 * only ever "slightly less than owner", and this picker — which could grant
 * them — was very nearly the only place in the product they existed.
 */
const RUNGS: { key: string; label: string; unlocks: string }[] = [
  {
    key: 'owner',
    label: 'Owner',
    unlocks: 'Run the gym: billing, settings, the catalogue, who is staff',
  },
  {
    key: 'trainer',
    label: 'Trainer',
    unlocks: 'Coach clients and assign them published programmes',
  },
  { key: 'member', label: 'Member', unlocks: 'Train here, log workouts, be coached' },
];

/** Days since an ISO instant, or null if it never happened. */
function daysSince(iso: string | undefined, now = Date.now()): number | null {
  if (!iso) return null;
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return null;
  return Math.floor((now - then) / 86_400_000);
}

type Lens = 'all' | 'lapsed' | 'uncoached' | 'staff';

/**
 * The roster, with the one thing a list of names cannot tell you: who has
 * stopped coming.
 *
 * The app's People tab shows a trainer their own clients. This shows a manager
 * everybody, sorted so the people who need chasing are at the top — which is
 * the actual job, and which needs a table rather than a scroll.
 *
 * The lenses are the three questions a manager actually opens this page with,
 * named as questions rather than as filters: who has drifted, who has nobody
 * looking after them, who works here.
 */
export function People({ gym, me }: { gym: string; me?: Me }) {
  const queryClient = useQueryClient();
  const [search, setSearch] = useState('');
  const [lens, setLens] = useState<Lens>('all');
  /** Whose standing is being edited, and to what. Null when nobody's is. */
  const [editing, setEditing] = useState<{ id: string; name: string; caps: string[] } | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  /** The new-staff form, open or not. Null when it is not. */
  const [adding, setAdding] = useState<{
    email: string;
    display_name: string;
    capacities: string[];
  } | null>(null);
  /** The account just created, and the password shown exactly once. */
  const [created, setCreated] = useState<CreatedStaff | null>(null);

  const myCapacities = me?.memberships?.[0]?.capacities ?? [];
  const iAmOwner = myCapacities.includes('owner');
  /*
    Does this account run the gym?

    This page had NO such check. It rendered "Add staff" and a "Set standing"
    button on every row for anybody who reached it, and called the
    manager-only roster endpoint — which 403s for a trainer, so a coach opened
    People to an error and two buttons the server would never honour.

    Mirrors `Capabilities::can_manage_gym`. Since ADR-0036 that is owner alone,
    which is why this reads as a single check rather than a list.
  */
  const manages = iAmOwner;

  const save = useMutation({
    mutationFn: () => api.setCapacities(gym, editing!.id, editing!.caps),
    onSuccess: () => {
      setError(null);
      setEditing(null);
      void queryClient.invalidateQueries({ queryKey: ['members', gym] });
    },
    onError: (e: Error) =>
      setError(
        e instanceof ApiError && e.code === 'auth.forbidden'
          ? iAmOwner
            ? 'That change was refused.'
            : 'Only an owner can grant or remove owner.'
          : e instanceof ApiError
            ? e.message
            : 'Could not save that. Please try again.',
      ),
  });

  const createStaff = useMutation({
    mutationFn: () => api.createStaff(gym, adding!),
    onSuccess: (result) => {
      setError(null);
      setAdding(null);
      setCreated(result);
      void queryClient.invalidateQueries({ queryKey: ['members', gym] });
    },
    onError: (e: Error) =>
      setError(
        e instanceof ApiError && e.code === 'resource.conflict'
          ? 'That address already has an account. Ask them to join this gym, then set their standing from the roster.'
          : e instanceof ApiError && e.code === 'auth.forbidden'
            ? iAmOwner
              ? 'That was refused.'
              : 'Only an owner can create an owner.'
            : e instanceof ApiError
              ? e.message
              : 'Could not create that account. Please try again.',
      ),
  });

  // Manager-only endpoint, so manager-only query. Asking anyway would put a
  // guaranteed 403 in the console's network log on every trainer's first visit.
  const members = useQuery({
    queryKey: ['members', gym],
    queryFn: () => api.members(gym),
    enabled: manages,
  });
  const relationships = useQuery({
    queryKey: ['relationships', gym],
    queryFn: () => api.relationships(gym),
  });
  const sessions = useQuery({
    queryKey: ['sessions', gym, 'roster'],
    queryFn: () => api.sessions(gym, { limit: 500 }),
  });

  const everyone = useMemo(() => {
    const now = Date.now();

    // Most recent session per athlete, in one pass.
    const lastSeen = new Map<string, string>();
    for (const s of sessions.data ?? []) {
      const current = lastSeen.get(s.athlete_id);
      if (!current || s.started_at > current) lastSeen.set(s.athlete_id, s.started_at);
    }

    const coachOf = new Map<string, string>();
    for (const r of relationships.data ?? []) {
      if (r.is_active) coachOf.set(r.athlete_id, r.coach_name ?? 'Coach');
    }

    const training = new Set(
      (sessions.data ?? []).filter((s) => s.is_open).map((s) => s.athlete_id),
    );

    return (members.data ?? [])
      .map((m) => ({
        ...m,
        coach: coachOf.get(m.user_id) ?? null,
        idle: daysSince(lastSeen.get(m.user_id), now),
        live: training.has(m.user_id),
        isStaff: m.capacities.some((c) => c !== 'member'),
      }))
      // Never-trained first, then longest-idle. The default ordering IS the
      // triage: alphabetical would be tidy and useless.
      .sort((a, b) => {
        if (a.idle === null && b.idle === null) return a.display_name.localeCompare(b.display_name);
        if (a.idle === null) return -1;
        if (b.idle === null) return 1;
        return b.idle - a.idle;
      });
  }, [members.data, relationships.data, sessions.data]);

  const counts = useMemo(
    () => ({
      all: everyone.length,
      // Two weeks is the line the app's own attention rule uses, so the two
      // surfaces agree about who counts as drifting.
      lapsed: everyone.filter((m) => m.idle === null || m.idle >= 14).length,
      uncoached: everyone.filter((m) => m.coach === null && !m.isStaff).length,
      staff: everyone.filter((m) => m.isStaff).length,
    }),
    [everyone],
  );

  const rows = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return everyone
      .filter((m) => !needle || m.display_name.toLowerCase().includes(needle))
      .filter((m) =>
        lens === 'lapsed'
          ? m.idle === null || m.idle >= 14
          : lens === 'uncoached'
            ? m.coach === null && !m.isStaff
            : lens === 'staff'
              ? m.isStaff
              : true,
      );
  }, [everyone, search, lens]);

  const training = everyone.filter((m) => m.live);

  /*
    A coach gets their own clients instead of the gym's roster.

    Not the same page with the buttons hidden: the DATA is different. The
    roster comes from a manager-only endpoint, so for a trainer there is
    nothing to hide the buttons on top of. What they can read — their own
    coaching relationships, and their clients' sessions — is exactly a client
    list, which is what the phone app has always shown them.
  */
  if (!manages) {
    return <MyClients relationships={relationships.data} sessions={sessions.data} />;
  }

  return (
    <>
      <PageHead
        title="People"
        lede="Everybody at this gym, with whoever coaches them and when they last trained. This is also where staff are made — everyone joins as a member, and you promote them from here."
      />

      {error ? <p className="banner">{error}</p> : null}

      {/*
        Shown once, and only here. The server keeps no readable copy, so this
        card is the single moment the password exists anywhere a person can
        see it — which is why dismissing it is a deliberate press rather than
        something that happens when the table refreshes.
      */}
      {created ? (
        <Card>
          <SectionHead title={`${created.display_name} is set up`} />
          <p className="muted" style={{ margin: '0 0 16px', maxWidth: '62ch' }}>
            They sign in with these, then change the password from their own account. This is
            the only time it is shown.
          </p>
          <table className="handover">
            <tbody>
              <tr>
                <th>Email</th>
                <td>{created.email}</td>
              </tr>
              <tr>
                <th>Password</th>
                <td>
                  <code className="secret">{created.temporary_password}</code>
                </td>
              </tr>
              <tr>
                <th>Holds</th>
                <td>
                  {created.capacities.map((c) => (
                    <span key={c} style={{ marginRight: 4 }}>
                      <Chip tone="ink">{c.replace('_', ' ')}</Chip>
                    </span>
                  ))}
                </td>
              </tr>
            </tbody>
          </table>
          <button type="button" style={{ marginTop: 18 }} onClick={() => setCreated(null)}>
            I have written this down
          </button>
        </Card>
      ) : null}

      {adding ? (
        <Card>
          <div className="row" style={{ justifyContent: 'space-between' }}>
            <h2>New staff account</h2>
            <button type="button" className="quiet" onClick={() => setAdding(null)}>
              Cancel
            </button>
          </div>
          <p className="muted" style={{ margin: '4px 0 16px', maxWidth: '62ch' }}>
            Creates the account and their standing together, and gives you a password to hand
            over. For somebody who already has an account, have them join and set their
            standing from the roster instead — that way they pick their own password.
          </p>
          <div className="row" style={{ alignItems: 'flex-start', gap: 16 }}>
            <div className="field" style={{ flex: 1, marginBottom: 0 }}>
              <label htmlFor="staff-name">Name</label>
              <input
                id="staff-name"
                value={adding.display_name}
                placeholder="Tariq Trainer"
                onChange={(e) => setAdding({ ...adding, display_name: e.target.value })}
              />
            </div>
            <div className="field" style={{ flex: 1, marginBottom: 0 }}>
              <label htmlFor="staff-email">Email</label>
              <input
                id="staff-email"
                type="email"
                value={adding.email}
                placeholder="tariq@example.com"
                onChange={(e) => setAdding({ ...adding, email: e.target.value })}
              />
            </div>
          </div>
          <div className="rungs" style={{ marginTop: 18 }}>
            {RUNGS.map((rung) => {
              const on = adding.capacities.includes(rung.key);
              const locked = rung.key === 'owner' && !iAmOwner;
              return (
                <label key={rung.key} className={locked ? 'rung locked' : 'rung'}>
                  <input
                    type="checkbox"
                    checked={on}
                    disabled={locked}
                    onChange={() =>
                      setAdding({
                        ...adding,
                        capacities: on
                          ? adding.capacities.filter((c) => c !== rung.key)
                          : [...adding.capacities, rung.key],
                      })
                    }
                  />
                  <span>
                    <strong>{rung.label}</strong>
                    <small>{rung.unlocks}</small>
                  </span>
                </label>
              );
            })}
          </div>
          <button
            type="button"
            style={{ marginTop: 18 }}
            disabled={
              createStaff.isPending ||
              adding.capacities.length === 0 ||
              adding.display_name.trim() === '' ||
              !adding.email.includes('@')
            }
            onClick={() => createStaff.mutate()}
          >
            {createStaff.isPending ? 'Creating…' : 'Create the account'}
          </button>
        </Card>
      ) : null}

      <div className="stats">
        <Stat label="On the roster" value={String(counts.all)} hint="members and staff" />
        <Stat
          label="Training now"
          value={String(training.length)}
          hint={training.length > 0 ? training.map((m) => m.display_name).join(', ') : 'nobody in'}
        />
        <Stat
          label="Drifting"
          value={String(counts.lapsed)}
          hint="not trained in a fortnight"
          alert={counts.lapsed > 0}
        />
        <Stat
          label="Without a coach"
          value={String(counts.uncoached)}
          hint="members nobody is looking after"
          alert={counts.uncoached > 0}
        />
      </div>

      <SectionHead title="Roster" count={rows.length} note="longest away first">
        <button
          type="button"
          onClick={() => {
            setError(null);
            setCreated(null);
            // Trainer and member together is the common case by a distance: a
            // coach at a gym almost always trains there too.
            setAdding({ email: '', display_name: '', capacities: ['trainer', 'member'] });
          }}
        >
          Add staff
        </button>
        <Segmented
          label="Filter the roster"
          value={lens}
          onChange={setLens}
          options={[
            { key: 'all', label: 'Everyone', count: counts.all },
            { key: 'lapsed', label: 'Drifting', count: counts.lapsed },
            { key: 'uncoached', label: 'No coach', count: counts.uncoached },
            { key: 'staff', label: 'Staff', count: counts.staff },
          ]}
        />
      </SectionHead>

      <div className="row" style={{ marginBottom: 12 }}>
        <input
          className="search"
          type="search"
          placeholder="Search by name…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          aria-label="Search the roster"
        />
      </div>

      {editing ? (
        <Card>
          <div className="row" style={{ justifyContent: 'space-between' }}>
            <h2>{editing.name}</h2>
            <button type="button" className="quiet" onClick={() => setEditing(null)}>
              Cancel
            </button>
          </div>
          <p className="muted" style={{ margin: '4px 0 16px', maxWidth: '62ch' }}>
            Tick what they should hold. This replaces their standing rather than adding to it,
            so unticking Trainer demotes them.
          </p>
          <div className="rungs">
            {RUNGS.map((rung) => {
              const on = editing.caps.includes(rung.key);
              // Owner is the one rung an admin can see and not move.
              const locked = rung.key === 'owner' && !iAmOwner;
              return (
                <label key={rung.key} className={locked ? 'rung locked' : 'rung'}>
                  <input
                    type="checkbox"
                    checked={on}
                    disabled={locked}
                    onChange={() =>
                      setEditing({
                        ...editing,
                        caps: on
                          ? editing.caps.filter((c) => c !== rung.key)
                          : [...editing.caps, rung.key],
                      })
                    }
                  />
                  <span>
                    <strong>{rung.label}</strong>
                    <small>{rung.unlocks}</small>
                  </span>
                </label>
              );
            })}
          </div>
          <div className="row" style={{ marginTop: 18 }}>
            <button
              type="button"
              disabled={editing.caps.length === 0 || save.isPending}
              onClick={() => save.mutate()}
            >
              {save.isPending ? 'Saving…' : 'Save standing'}
            </button>
            {editing.caps.length === 0 ? (
              <span className="muted">Somebody has to hold something.</span>
            ) : null}
          </div>
        </Card>
      ) : null}

      {rows.length === 0 ? (
        <Empty title={search || lens !== 'all' ? 'Nobody matches' : 'Nobody has joined yet'}>
          {search || lens !== 'all'
            ? 'Clear the search, or switch back to Everyone.'
            : 'Invite your first member from the phone app, or open registration in Settings.'}
        </Empty>
      ) : (
        <TableCard>
          <thead>
            <tr>
              <th>Name</th>
              <th>Standing</th>
              <th>Coach</th>
              <th>Last trained</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {rows.map((m) => (
              <tr key={m.user_id}>
                <td className="strong">{m.display_name}</td>
                <td>
                  {m.capacities.map((c) => (
                    <span key={c} style={{ marginRight: 4 }}>
                      <Chip tone={c === 'member' ? 'quiet' : 'ink'}>{c.replace('_', ' ')}</Chip>
                    </span>
                  ))}
                </td>
                <td className={m.coach ? undefined : 'muted'}>{m.coach ?? 'nobody'}</td>
                <td>{m.live ? <Chip tone="live">Training now</Chip> : <Idle days={m.idle} />}</td>
                <td>
                  <div className="rowactions">
                    <button
                      type="button"
                      className="ghost small"
                      onClick={() => {
                        setError(null);
                        setEditing({
                          id: m.user_id,
                          name: m.display_name,
                          caps: [...m.capacities],
                        });
                      }}
                    >
                      Set standing
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </TableCard>
      )}
    </>
  );
}

/**
 * A coach's own clients.
 *
 * Built from the two things a trainer may actually read — their coaching
 * relationships and their clients' sessions — rather than from the gym roster,
 * which the server refuses them and rightly so.
 *
 * No management controls, because a trainer has no management rights: the
 * server refuses `setCapacities` and `createStaff` for them, and a button that
 * exists only to produce a 403 is worse than no button. It teaches the person
 * that the software is broken rather than that the action is not theirs.
 */
function MyClients({
  relationships,
  sessions,
}: {
  relationships?: { athlete_id: string; athlete_name?: string | null; is_active: boolean }[];
  sessions?: { athlete_id: string; started_at: string; is_open: boolean }[];
}) {
  const rows = useMemo(() => {
    const now = Date.now();

    const lastSeen = new Map<string, string>();
    for (const s of sessions ?? []) {
      const current = lastSeen.get(s.athlete_id);
      if (!current || s.started_at > current) lastSeen.set(s.athlete_id, s.started_at);
    }
    const training = new Set((sessions ?? []).filter((s) => s.is_open).map((s) => s.athlete_id));

    return (relationships ?? [])
      .filter((r) => r.is_active)
      .map((r) => ({
        id: r.athlete_id,
        name: r.athlete_name ?? 'Athlete',
        idle: daysSince(lastSeen.get(r.athlete_id), now),
        live: training.has(r.athlete_id),
      }))
      // Same triage order as the manager roster: never-trained first, then
      // longest away. The two views disagreeing about who needs attention
      // would be worse than either being wrong.
      .sort((a, b) => {
        if (a.idle === null && b.idle === null) return a.name.localeCompare(b.name);
        if (a.idle === null) return -1;
        if (b.idle === null) return 1;
        return b.idle - a.idle;
      });
  }, [relationships, sessions]);

  return (
    <>
      <PageHead
        title="Your clients"
        lede="The people you coach, longest away first. Pairing and standing are set by whoever runs the gym."
      />

      {rows.length === 0 ? (
        <Empty title="No clients yet">
          <p className="muted">
            When somebody chooses you as their coach, or the gym pairs you with them and you
            accept, they appear here.
          </p>
        </Empty>
      ) : (
        <TableCard>
          <thead>
            <tr>
              <th>Name</th>
              <th>Last trained</th>
              <th>Now</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.id}>
                <td>{r.name}</td>
                <td>
                  <Idle days={r.idle} />
                </td>
                <td>{r.live ? <Chip tone="live">Training now</Chip> : null}</td>
              </tr>
            ))}
          </tbody>
        </TableCard>
      )}
    </>
  );
}

function Idle({ days }: { days: number | null }) {
  // Only a member who trains is expected to; somebody who never has is a
  // different problem from somebody who stopped, and the words say which.
  if (days === null) return <Chip tone="late">Never</Chip>;
  if (days === 0) return <Chip tone="paid">Today</Chip>;
  if (days >= 14) return <Chip tone="late">{days} days</Chip>;
  if (days >= 7) return <Chip tone="due">{days} days</Chip>;
  return <span className="muted">{days === 1 ? 'Yesterday' : `${days} days ago`}</span>;
}
