import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';

import { api } from '../lib/api';
import { Card, Chip, Empty, PageHead, SectionHead, Stat, TableCard } from '../ui';

/**
 * The morning glance: what needs attention today.
 *
 * Everything here is a NUMBER PLUS A ROUTE — a count with nowhere to go is
 * decoration. The three that earn a place are the three that represent work
 * waiting on a person: money owed, proposals unreviewed, coaching requests
 * unanswered.
 *
 * The layout says the same thing twice on purpose. The numbers along the top
 * are the state of the gym; the queue below is the list of things somebody has
 * to do about it. Owners open this once in the morning and want the second
 * one — so the queue is a real table, not a footnote under the tiles.
 */
export function Overview({ gym }: { gym: string }) {
  const invoices = useQuery({ queryKey: ['invoices', gym], queryFn: () => api.invoices(gym) });
  const members = useQuery({ queryKey: ['members', gym], queryFn: () => api.members(gym) });
  const pending = useQuery({
    queryKey: ['pending-exercises', gym],
    queryFn: () => api.pendingExercises(gym),
    // Head-coach and above; a plain admin without that standing would 403.
    retry: false,
  });
  const requests = useQuery({
    queryKey: ['coaching-requests', gym],
    queryFn: () => api.coachingRequests(gym),
  });
  const sessions = useQuery({
    queryKey: ['sessions', gym, 'recent'],
    queryFn: () => api.sessions(gym, { limit: 200 }),
  });

  const all = invoices.data ?? [];
  const outstanding = all.filter((i) => i.status.state === 'due');
  const overdue = outstanding.filter((i) => i.is_overdue);
  const owed = outstanding.reduce((sum, i) => sum + i.amount_minor - i.paid_minor, 0);

  const currency = all[0]?.currency ?? 'GBP';
  const money = (minor: number) =>
    new Intl.NumberFormat(undefined, {
      style: 'currency',
      currency,
      maximumFractionDigits: minor % 100 === 0 ? 0 : 2,
    }).format(minor / 100);

  // "Trained this week" over sessions, by day, so a quiet week is visible as a
  // number rather than something you have to notice the absence of.
  const weekAgo = new Date(Date.now() - 7 * 86_400_000).toISOString();
  const recent = sessions.data ?? [];
  const thisWeek = recent.filter((s) => s.started_at >= weekAgo);
  const activeMembers = new Set(thisWeek.map((s) => s.athlete_id)).size;
  const training = recent.filter((s) => s.is_open);

  const proposals = pending.data ?? [];
  const waiting = (requests.data ?? []).filter((r) => r.is_pending);
  const roster = members.data ?? [];

  const queue = overdue.length + waiting.length;

  return (
    <>
      <PageHead
        title="Overview"
        lede="What is waiting on somebody today, and how the gym is doing this week."
      />

      <div className="stats">
        <Stat
          label="Owed"
          value={money(owed)}
          hint={`${outstanding.length} unpaid ${outstanding.length === 1 ? 'invoice' : 'invoices'}`}
        />
        <Stat
          label="Overdue"
          value={String(overdue.length)}
          hint={overdue.length > 0 ? 'past the due date' : 'nothing late'}
          alert={overdue.length > 0}
        />
        <Stat
          label="Proposals"
          value={pending.isError ? '—' : String(proposals.length)}
          hint={pending.isError ? 'head coaches only' : 'movements to review'}
          alert={proposals.length > 0}
        />
        <Stat
          label="Requests"
          value={String(waiting.length)}
          hint="members wanting a coach"
          alert={waiting.length > 0}
        />
        <Stat label="Members" value={String(roster.length)} hint="on the roster" />
        <Stat
          label="Trained"
          value={String(activeMembers)}
          hint="different people, last 7 days"
        />
      </div>

      {/*
        Who is on the floor right now. Not a stat — it changes minute to minute
        and it is the one thing on this page a manager might act on by walking
        somewhere, so it reads as a line of names rather than a count.
      */}
      {training.length > 0 ? (
        <>
          <SectionHead title="On the floor now" count={training.length} />
          <Card>
            <div className="row">
              {training.map((session) => (
                <span key={session.id} className="row" style={{ gap: 8 }}>
                  <Chip tone="live">Live</Chip>
                  <span className="strong">{session.athlete_name ?? 'Member'}</span>
                  <span className="muted">{session.workout_name ?? 'Workout'}</span>
                </span>
              ))}
            </div>
          </Card>
        </>
      ) : null}

      <SectionHead
        title="Needs attention"
        count={queue > 0 ? queue : undefined}
        note={queue > 0 ? 'oldest first' : undefined}
      />

      {queue === 0 ? (
        <Empty title="Nothing outstanding">
          No invoice is late and no coaching request is unanswered. Unusual and good.
        </Empty>
      ) : (
        <TableCard>
          <thead>
            <tr>
              <th>What</th>
              <th>Who</th>
              <th className="num">Amount</th>
              <th className="num">Waiting</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {[...overdue]
              .sort((a, b) => b.days_overdue - a.days_overdue)
              .map((invoice) => (
                <tr key={invoice.id}>
                  <td>
                    <Chip tone="late">Overdue</Chip>{' '}
                    <span className="strong">{invoice.description}</span>
                  </td>
                  <td>{invoice.member_name}</td>
                  <td className="num strong">
                    {money(invoice.amount_minor - invoice.paid_minor)}
                  </td>
                  <td className="num muted">{invoice.days_overdue}d</td>
                  <td className="num">
                    <Link to="/billing">Take payment</Link>
                  </td>
                </tr>
              ))}
            {waiting.map((r) => (
              <tr key={r.id}>
                <td>
                  <Chip tone="ink">Request</Chip>{' '}
                  <span className="strong">wants {r.coach_name} to coach them</span>
                </td>
                <td>{r.athlete_name}</td>
                <td className="num muted">—</td>
                <td className="num muted">
                  {Math.max(
                    0,
                    Math.floor((Date.now() - new Date(r.requested_at).getTime()) / 86_400_000),
                  )}
                  d
                </td>
                <td className="num">
                  <Link to="/people">See request</Link>
                </td>
              </tr>
            ))}
          </tbody>
        </TableCard>
      )}

      {/*
        Named rather than left implicit: the review queue is the one job here
        that a plain admin cannot do, and a head coach who never opens the
        Catalogue page never learns it is waiting.
      */}
      {proposals.length > 0 ? (
        <>
          <SectionHead title="Movements waiting for review" count={proposals.length} />
          <Card>
            <p style={{ margin: '0 0 14px' }}>
              {proposals.length === 1
                ? 'A trainer has named a movement that is not in the catalogue yet.'
                : `Trainers have named ${proposals.length} movements that are not in the catalogue yet.`}{' '}
              They are usable today; reviewing them is how the catalogue avoids two entries for
              the same lift, which would split somebody&apos;s history in two.
            </p>
            <Link className="btn" to="/catalogue">
              Review the queue
            </Link>
          </Card>
        </>
      ) : null}
    </>
  );
}
