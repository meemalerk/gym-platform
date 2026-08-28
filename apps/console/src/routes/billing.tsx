import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useMemo, useState } from 'react';

import { ApiError, api, type Invoice } from '../lib/api';
import { Chip, Empty, PageHead, SectionHead, Segmented, Stat, TableCard } from '../ui';

type Filter = 'due' | 'overdue' | 'paid' | 'all';

/**
 * The billing ledger — the screen that most justifies a console existing.
 *
 * A phone can show a member their own three invoices. It cannot show an owner
 * two hundred rows they need to scan, sort and act on, and the app's Billing
 * tab has been doing an honest impression of trying.
 *
 * The page opens with the three numbers an owner is actually tracking, because
 * the ledger below answers "which row" and never answers "how are we doing".
 */
export function Billing({ gym }: { gym: string }) {
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState<Filter>('due');
  const [error, setError] = useState<string | null>(null);

  const invoices = useQuery({ queryKey: ['invoices', gym], queryFn: () => api.invoices(gym) });
  const subscriptions = useQuery({
    queryKey: ['subscriptions', gym],
    queryFn: () => api.subscriptions(gym),
  });
  const plans = useQuery({ queryKey: ['plans', gym], queryFn: () => api.plans(gym) });

  const currency = invoices.data?.[0]?.currency ?? 'GBP';
  const money = (minor: number) =>
    new Intl.NumberFormat(undefined, {
      style: 'currency',
      currency,
      maximumFractionDigits: minor % 100 === 0 ? 0 : 2,
    }).format(minor / 100);

  const all = useMemo(() => invoices.data ?? [], [invoices.data]);
  const active = useMemo(
    () => (subscriptions.data ?? []).filter((s) => s.is_active),
    [subscriptions.data],
  );

  const counts = useMemo(
    () => ({
      due: all.filter((i) => i.status.state === 'due').length,
      overdue: all.filter((i) => i.is_overdue).length,
      paid: all.filter((i) => i.status.state === 'paid').length,
      all: all.length,
    }),
    [all],
  );

  const rows = useMemo(() => {
    const chosen =
      filter === 'due'
        ? all.filter((i) => i.status.state === 'due')
        : filter === 'overdue'
          ? all.filter((i) => i.is_overdue)
          : filter === 'paid'
            ? all.filter((i) => i.status.state === 'paid')
            : all;
    // Whatever the filter, the thing most in need of a person comes first.
    return [...chosen].sort((a, b) => {
      if (a.is_overdue !== b.is_overdue) return a.is_overdue ? -1 : 1;
      return a.due_on.localeCompare(b.due_on);
    });
  }, [all, filter]);

  /** Monthly recurring revenue: what the active subscriptions bill each month. */
  const mrr = active.reduce((n, s) => n + s.price_minor, 0);
  const owedTotal = all
    .filter((i) => i.status.state === 'due')
    .reduce((n, i) => n + i.amount_minor - i.paid_minor, 0);
  const lateTotal = all
    .filter((i) => i.is_overdue)
    .reduce((n, i) => n + i.amount_minor - i.paid_minor, 0);

  /**
   * Take a cash payment at the desk.
   *
   * Recorded at the FULL outstanding balance and nothing else. A part-payment
   * field here would be a free money-entry box on a dashboard, and the one
   * thing worse than not supporting part payments is supporting them with a
   * mistyped amount. The API takes them; a gym that needs one can use it.
   */
  const takeCash = useMutation({
    mutationFn: (invoice: Invoice) =>
      api.recordPayment(gym, invoice.id, {
        amount_minor: invoice.amount_minor - invoice.paid_minor,
        provider: 'cash',
        received_on: new Date().toISOString().slice(0, 10),
        note: 'Taken at the desk',
      }),
    onSuccess: () => {
      setError(null);
      void queryClient.invalidateQueries({ queryKey: ['invoices', gym] });
    },
    onError: (e: Error) =>
      setError(
        e instanceof ApiError ? e.message : 'Could not record that payment. Please try again.',
      ),
  });

  return (
    <>
      <PageHead
        title="Billing"
        lede="Every invoice this gym has raised, what is still owed, and who is on a plan."
      />

      {error ? <p className="banner">{error}</p> : null}

      <div className="stats">
        <Stat
          label="Monthly"
          value={money(mrr)}
          hint={`${active.length} active ${active.length === 1 ? 'membership' : 'memberships'}`}
        />
        <Stat label="Outstanding" value={money(owedTotal)} hint={`${counts.due} unpaid`} />
        <Stat
          label="Late"
          value={money(lateTotal)}
          hint={`${counts.overdue} past the due date`}
          alert={counts.overdue > 0}
        />
        <Stat
          label="On sale"
          value={String((plans.data ?? []).filter((p) => p.is_offered).length)}
          hint="plans a member can join"
        />
      </div>

      <SectionHead title="Invoices" count={rows.length}>
        <Segmented
          label="Filter invoices"
          value={filter}
          onChange={setFilter}
          options={[
            { key: 'due', label: 'Due', count: counts.due },
            { key: 'overdue', label: 'Overdue', count: counts.overdue },
            { key: 'paid', label: 'Paid', count: counts.paid },
            { key: 'all', label: 'All', count: counts.all },
          ]}
        />
      </SectionHead>

      {rows.length === 0 ? (
        <Empty title={filter === 'all' ? 'Nothing billed yet' : `Nothing ${filter}`}>
          {filter === 'all'
            ? 'Put a member on a plan and their first invoice is issued straight away.'
            : 'Try another filter.'}
        </Empty>
      ) : (
        <TableCard>
          <thead>
            <tr>
              <th>Member</th>
              <th>For</th>
              <th>Reference</th>
              <th>Due</th>
              <th className="num">Amount</th>
              <th className="num">Outstanding</th>
              <th>Status</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {rows.map((invoice) => {
              const outstanding = invoice.amount_minor - invoice.paid_minor;
              return (
                <tr key={invoice.id}>
                  <td className="strong">{invoice.member_name}</td>
                  <td>{invoice.description}</td>
                  <td className="ref">{invoice.reference}</td>
                  <td className="muted">{invoice.due_on}</td>
                  <td className="num">{money(invoice.amount_minor)}</td>
                  <td className="num strong">
                    {outstanding > 0 ? money(outstanding) : <span className="muted">—</span>}
                  </td>
                  <td>
                    <StatusChip invoice={invoice} />
                  </td>
                  <td>
                    <div className="rowactions">
                      {invoice.status.state === 'due' ? (
                        <button
                          type="button"
                          className="ghost small"
                          disabled={takeCash.isPending}
                          onClick={() => takeCash.mutate(invoice)}
                        >
                          Take {money(outstanding)} cash
                        </button>
                      ) : null}
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </TableCard>
      )}

      <SectionHead
        title="Memberships"
        count={active.length}
        note="cancelled ones keep their history"
      />
      {(subscriptions.data ?? []).length === 0 ? (
        <Empty title="Nobody is on a plan">
          Until somebody is, this gym bills nobody and withholds nothing — a gym with nothing on
          sale does not gate its members.
        </Empty>
      ) : (
        <TableCard>
          <thead>
            <tr>
              <th>Member</th>
              <th>Plan</th>
              <th className="num">Price</th>
              <th>Started</th>
              <th>Next charge</th>
              <th>Status</th>
            </tr>
          </thead>
          <tbody>
            {(subscriptions.data ?? []).map((s) => (
              <tr key={s.id}>
                <td className="strong">{s.member_name}</td>
                <td>{s.plan_name}</td>
                <td className="num">{s.price_label}</td>
                <td className="muted">{s.started_on}</td>
                {/* Null once cancelled, and that IS the information: it is how
                    you see at a glance that the nightly tick will now leave
                    them alone. */}
                <td className="muted">
                  {s.next_charge_on ?? <span className="muted">not billing</span>}
                </td>
                <td>
                  {s.is_active ? <Chip tone="paid">Active</Chip> : <Chip tone="void">Cancelled</Chip>}
                </td>
              </tr>
            ))}
          </tbody>
        </TableCard>
      )}

      <SectionHead title="Plans" count={(plans.data ?? []).length} />
      {(plans.data ?? []).length === 0 ? (
        <Empty title="Nothing on sale">
          A plan is what this gym sells — open gym, coaching, a drop-in. Add the first one from
          the phone app.
        </Empty>
      ) : (
        <TableCard>
          <thead>
            <tr>
              <th>Name</th>
              <th className="num">Price</th>
              <th>Billed</th>
              <th>Unlocks</th>
              <th className="num">On it</th>
            </tr>
          </thead>
          <tbody>
            {(plans.data ?? []).map((plan) => (
              <tr key={plan.id}>
                <td>
                  <span className="strong">{plan.name}</span>{' '}
                  {plan.is_offered ? null : <Chip tone="void">Archived</Chip>}
                </td>
                <td className="num">{plan.price_label}</td>
                <td className="muted">{plan.interval}</td>
                <td className="muted">{plan.grants.join(', ') || '—'}</td>
                <td className="num">
                  {
                    (subscriptions.data ?? []).filter((s) => s.plan_id === plan.id && s.is_active)
                      .length
                  }
                </td>
              </tr>
            ))}
          </tbody>
        </TableCard>
      )}

      <p className="muted footnote">
        Cash taken at the desk is recorded here. An issued invoice is never edited — a
        correction is a void plus a new invoice, and a refund is a negative payment, so the
        ledger always adds up to what actually happened.
      </p>
    </>
  );
}

function StatusChip({ invoice }: { invoice: Invoice }) {
  if (invoice.status.state === 'void') return <Chip tone="void">Void</Chip>;
  if (invoice.status.state === 'paid') return <Chip tone="paid">Paid</Chip>;
  // Overdue is derived, never stored — the server computes it from the due
  // date and today, and this just renders what it was told.
  if (invoice.is_overdue) return <Chip tone="late">{invoice.days_overdue}d late</Chip>;
  return <Chip tone="due">Due</Chip>;
}
