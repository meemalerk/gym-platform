import { useQuery } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import { api, type AuditEntry } from '../lib/api';
import { Empty, PageHead, SectionHead, Segmented, TableCard } from '../ui';

/**
 * The audit trail: who did what, in one gym.
 *
 * The app has this too, rendered as sentences grouped by day, which is right
 * for a phone. This is the other reading of the same data — a filterable table
 * you scan when you are trying to answer a specific question, usually "when
 * did that change and who did it".
 */
export function Activity({ gym }: { gym: string }) {
  const [area, setArea] = useState('all');
  const [search, setSearch] = useState('');

  const audit = useQuery({ queryKey: ['audit', gym], queryFn: () => api.audit(gym) });

  const all = useMemo(() => audit.data ?? [], [audit.data]);

  // Areas come from the data, not a hard-coded list: a new action type should
  // appear in the filter the day it starts being recorded, without anyone
  // remembering to add it here.
  const areas = useMemo(() => {
    const tally = new Map<string, number>();
    for (const entry of all) {
      const key = entry.action.split('.')[0] ?? 'other';
      tally.set(key, (tally.get(key) ?? 0) + 1);
    }
    return [
      { key: 'all', label: 'Everything', count: all.length },
      ...[...tally.entries()]
        .sort((a, b) => a[0].localeCompare(b[0]))
        .map(([key, count]) => ({ key, label: key.replace(/_/g, ' '), count })),
    ];
  }, [all]);

  const rows = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return all
      .filter((e) => area === 'all' || e.action.startsWith(`${area}.`))
      .filter(
        (e) =>
          !needle ||
          e.action.toLowerCase().includes(needle) ||
          (e.actor_name ?? '').toLowerCase().includes(needle),
      );
  }, [all, area, search]);

  return (
    <>
      <PageHead
        title="Activity"
        lede="Every change anyone made in this gym. Written in the same transaction as the change itself, and append-only — not even the app can edit it."
      />

      <SectionHead title="Changes" count={rows.length} note="newest first">
        <Segmented label="Filter by area" value={area} onChange={setArea} options={areas} />
      </SectionHead>

      <div className="row" style={{ marginBottom: 12 }}>
        <input
          className="search"
          type="search"
          placeholder="Search a person or an action…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          aria-label="Search the audit trail"
        />
      </div>

      {rows.length === 0 ? (
        <Empty title="Nothing recorded here">
          {area === 'all' && !search
            ? 'Changes to this gym appear here as they happen.'
            : 'Clear the search, or switch back to Everything.'}
        </Empty>
      ) : (
        <TableCard>
          <thead>
            <tr>
              <th>When</th>
              <th>Who</th>
              <th>What</th>
              <th>Details</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((entry) => (
              <tr key={entry.id}>
                <td className="ref">
                  {new Date(entry.occurred_at).toLocaleString(undefined, {
                    day: 'numeric',
                    month: 'short',
                    hour: '2-digit',
                    minute: '2-digit',
                  })}
                </td>
                <td className={entry.actor_name ? 'strong' : 'muted'}>
                  {entry.actor_name ?? 'system'}
                </td>
                <td>
                  {/*
                    The raw action name, not a friendly sentence. The app
                    renders these as prose because a member is reading a story;
                    somebody here is usually matching an exact string against a
                    bug report, and translating it would take that away.
                  */}
                  <span className="ref">{entry.action}</span>
                </td>
                <td className="muted">
                  <Details entry={entry} />
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
 * Metadata as a readable line.
 *
 * Rendered generically rather than with a per-action formatter: there are
 * thirty-odd action types and growing, and a formatter that only knows twenty
 * of them silently shows nothing for the rest. A dumb key-value line is worse
 * for the common case and never wrong.
 */
function Details({ entry }: { entry: AuditEntry }) {
  const meta = entry.metadata as Record<string, unknown> | null;
  if (!meta || typeof meta !== 'object') return <>—</>;

  const parts = Object.entries(meta)
    // Ids are noise in a table read by a person; the name beside them is what
    // carries meaning, and the entity id is already the row's identity.
    .filter(([key]) => !key.endsWith('_id'))
    .map(([key, value]) => `${key.replace(/_/g, ' ')}: ${format(value)}`);

  return <>{parts.length > 0 ? parts.join(' · ') : '—'}</>;
}

function format(value: unknown): string {
  if (value === null || value === undefined) return '—';
  if (Array.isArray(value)) return value.join(', ');
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}
