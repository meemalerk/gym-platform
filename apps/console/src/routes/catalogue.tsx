import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import { api, type Exercise } from '../lib/api';
import { Card, Chip, Empty, PageHead, SectionHead, Segmented, TableCard } from '../ui';

type Lens = 'all' | 'approved' | 'proposed' | 'retired';

/**
 * The catalogue, and the curation queue that keeps it honest (ADR-0024).
 *
 * Reviewing a proposal is comparison work — "is this the same movement as one
 * of the four hundred we already have?" — and that is exactly what a phone
 * cannot do. So each proposal is a card with the likely duplicates printed
 * inside it, next to the two buttons. The decision and the evidence for it are
 * never more than an inch apart.
 */
export function Catalogue({ gym }: { gym: string }) {
  const queryClient = useQueryClient();
  const [search, setSearch] = useState('');
  const [lens, setLens] = useState<Lens>('all');

  const all = useQuery({ queryKey: ['exercises', gym], queryFn: () => api.exercises(gym) });
  const pending = useQuery({
    queryKey: ['pending-exercises', gym],
    queryFn: () => api.pendingExercises(gym),
  });

  const curate = useMutation({
    mutationFn: ({ id, decision }: { id: string; decision: 'approve' | 'retire' }) =>
      api.curate(gym, id, decision),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['exercises', gym] });
      void queryClient.invalidateQueries({ queryKey: ['pending-exercises', gym] });
    },
  });

  const everything = all.data ?? [];

  const counts = useMemo(
    () => ({
      all: everything.length,
      approved: everything.filter((e) => e.status === 'approved').length,
      proposed: everything.filter((e) => e.status === 'proposed').length,
      retired: everything.filter((e) => e.status === 'retired').length,
    }),
    [everything],
  );

  const items = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return everything
      .filter((e) => (lens === 'all' ? true : e.status === lens))
      .filter(
        (e) =>
          !needle ||
          e.name.toLowerCase().includes(needle) ||
          (e.notes ?? '').toLowerCase().includes(needle),
      )
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [everything, search, lens]);

  const queue = pending.data ?? [];

  return (
    <>
      <PageHead
        title="Catalogue"
        lede={
          <>
            The movements this gym programmes. Progress is computed per movement, so a
            duplicate splits somebody&apos;s history in two — which is what the review queue is
            for.
          </>
        }
      />

      {pending.isError ? (
        <p className="banner">Only head coaches and above can review proposals.</p>
      ) : null}

      {queue.length > 0 ? (
        <>
          <SectionHead
            title="Waiting for review"
            count={queue.length}
            note="usable already — reviewing decides whether they stay"
          />
          {queue.map((proposal) => (
            <Card key={proposal.id}>
              <div className="row" style={{ alignItems: 'flex-start' }}>
                <div style={{ flex: 1, minWidth: 220 }}>
                  <h2>{proposal.name}</h2>
                  <p className="muted" style={{ margin: '4px 0 0' }}>
                    Measured in {proposal.modality.kind}
                    {proposal.notes ? ` · ${proposal.notes}` : ''}
                  </p>
                </div>
                <div className="rowactions">
                  <button
                    type="button"
                    disabled={curate.isPending}
                    onClick={() => curate.mutate({ id: proposal.id, decision: 'approve' })}
                  >
                    Approve
                  </button>
                  <button
                    type="button"
                    className="ghost"
                    disabled={curate.isPending}
                    onClick={() => curate.mutate({ id: proposal.id, decision: 'retire' })}
                  >
                    Retire
                  </button>
                </div>
              </div>
              {/* The whole reason this screen is worth having: the likely
                  duplicate, printed next to the decision. */}
              <Similar all={everything} to={proposal} />
            </Card>
          ))}
        </>
      ) : null}

      <SectionHead title="Everything" count={items.length} note="A – Z">
        <Segmented
          label="Filter the catalogue"
          value={lens}
          onChange={setLens}
          options={[
            { key: 'all', label: 'All', count: counts.all },
            { key: 'approved', label: 'In use', count: counts.approved },
            { key: 'proposed', label: 'Proposed', count: counts.proposed },
            { key: 'retired', label: 'Retired', count: counts.retired },
          ]}
        />
      </SectionHead>

      <div className="row" style={{ marginBottom: 12 }}>
        <input
          className="search"
          type="search"
          placeholder="Search a movement or a cue…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          aria-label="Search the catalogue"
        />
      </div>

      {items.length === 0 ? (
        <Empty title={search || lens !== 'all' ? 'Nothing matches' : 'The catalogue is empty'}>
          {search || lens !== 'all'
            ? 'Try a different word, or switch back to All.'
            : 'Add the first movement from the phone app — it becomes the vocabulary every programme is written in.'}
        </Empty>
      ) : (
        <TableCard>
          <thead>
            <tr>
              <th>Name</th>
              <th>Measured as</th>
              <th>Cues</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            {items.map((e) => (
              <tr key={e.id}>
                <td className="strong">{e.name}</td>
                <td className="muted">{e.modality.kind}</td>
                <td className="muted">{e.notes ?? '—'}</td>
                <td>
                  {e.status === 'proposed' ? <Chip tone="due">Proposed</Chip> : null}
                  {e.status === 'approved' ? <Chip tone="paid">In use</Chip> : null}
                  {e.status === 'retired' ? <Chip tone="void">Retired</Chip> : null}
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
 * Movements that share a word with a proposal.
 *
 * Deliberately dumb — a shared significant word, nothing cleverer. The job is
 * to put a plausible duplicate in front of a human, not to decide for them;
 * fuzzy matching that was occasionally wrong would be trusted and then be
 * wrong about somebody's training history.
 */
function Similar({ all, to }: { all: Exercise[]; to: Exercise }) {
  const words = to.name
    .toLowerCase()
    .split(/\s+/)
    // "the", "a", "of" match everything and mean nothing.
    .filter((w) => w.length > 3);

  const matches = all
    .filter((e) => e.id !== to.id && e.status !== 'retired')
    .filter((e) => words.some((w) => e.name.toLowerCase().includes(w)))
    .slice(0, 4);

  return (
    <div className="notice" style={{ marginTop: 4 }}>
      {matches.length === 0 ? (
        <>Nothing already in the catalogue shares a word with this one.</>
      ) : (
        <>
          <strong>Already in the catalogue:</strong> {matches.map((m) => m.name).join(' · ')}.
          Retire the proposal if one of these is the same movement.
        </>
      )}
    </div>
  );
}
