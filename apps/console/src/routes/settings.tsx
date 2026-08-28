import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { Link } from 'react-router-dom';

import { api } from '../lib/api';
import { Card, Chip, PageHead, SectionHead, Switch } from '../ui';

/**
 * Gym settings. One switch so far, and it is the consequential one.
 *
 * A settings page with a single control is a page that has to be honest about
 * being one, so the second half names what is designed but not built. A
 * settings screen that silently lacks the thing somebody came looking for
 * sends them hunting through every other screen; one that says "not built"
 * answers the question in a second.
 */
export function Settings({ gym }: { gym: string }) {
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);

  const settings = useQuery({ queryKey: ['settings', gym], queryFn: () => api.settings(gym) });

  const toggle = useMutation({
    mutationFn: (open: boolean) => api.setOpenRegistration(gym, open),
    onSuccess: (updated) => {
      setError(null);
      queryClient.setQueryData(['settings', gym], updated);
    },
    onError: () => setError('Could not change that. Please try again.'),
  });

  const open = settings.data?.open_registration ?? false;

  return (
    <>
      <PageHead title="Settings" lede="How this gym behaves." />

      {error ? <p className="banner">{error}</p> : null}

      <SectionHead title="Joining" />
      <Card>
        <div className="row" style={{ alignItems: 'flex-start', flexWrap: 'nowrap', gap: 24 }}>
          <div style={{ flex: 1 }}>
            <div className="row" style={{ gap: 10 }}>
              <h2>Anyone can join</h2>
              {open ? <Chip tone="paid">Open</Chip> : <Chip tone="void">Closed</Chip>}
            </div>
            <p className="muted" style={{ margin: '6px 0 0', maxWidth: '60ch' }}>
              {open
                ? 'People can find this gym in the app and join as members without a code.'
                : 'Nobody can join. Since invitations were removed this is the only way in, so a closed gym admits nobody at all \u2014 including people you want as staff.'}
            </p>
            {/* Stated next to the switch, not in a help page nobody opens: the
                open door cannot hand out staff standing, whatever it is set
                to. The capacity is hard-coded server-side (ADR-0026). */}
            <p className="muted" style={{ fontSize: 12.5, margin: '12px 0 0', maxWidth: '60ch' }}>
              This never grants trainer, admin or owner standing — only membership. Staff are
              made afterwards, from the roster on <Link to="/people">People</Link>.
            </p>
          </div>
          <Switch
            on={open}
            disabled={settings.isLoading || toggle.isPending}
            label="Let anyone join this gym as a member"
            onChange={(next) => {
              setError(null);
              toggle.mutate(next);
            }}
          />
        </div>
      </Card>

      <SectionHead title="Designed, not built yet" note="so you stop looking for it" />
      <Card>
        <dl className="notlist">
          <dt>Opening hours</dt>
          <dd>
            A weekly pattern plus dated closures is specified (ADR&#8209;0015) and the calendar
            it needs exists. Until it is wired up the app assumes the gym is always open.
          </dd>

          <dt>Dunning</dt>
          <dd>
            Nothing chases an overdue invoice automatically, and an owing member still trains.
            That is pinned by a test rather than left to chance, so the day suspension arrives
            it changes on purpose.
          </dd>

          <dt>Email</dt>
          <dd>
            Password resets and verification links are recorded rather than sent — there is a
            port for a mail provider and no adapter behind it yet. This is the reason
            invitations were removed rather than fixed (ADR&#8209;0031): a flow that cannot be
            finished without an email nobody receives is not a flow.
          </dd>
        </dl>
      </Card>
    </>
  );
}
