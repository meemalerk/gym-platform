import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { useMemo, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  RefreshControl,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import {
  archivePlan,
  listInvoices,
  listPlans,
  listSubscriptions,
  recordPayment,
  voidInvoice,
  type Invoice,
} from '@/api/gym';
import { summariseGrants } from '@/features/billing/entitlements';
import { useSession } from '@/session/store';
import {
  Badge,
  Card,
  Centered,
  EmptyState,
  ErrorBanner,
  ListRow,
  Section,
  StatRow,
  Touchable,
} from '@/ui/components';
import { GymHeader } from '@/ui/gym-header';
import { useTabBarHeight } from '@/ui/tab-bar';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/** Today in the gym's local calendar, as the API's YYYY-MM-DD. */
function localToday(): string {
  const d = new Date();
  return `${d.getFullYear()}-${`${d.getMonth() + 1}`.padStart(2, '0')}-${`${d.getDate()}`.padStart(2, '0')}`;
}

const shortDate = (iso: string) => {
  const d = new Date(`${iso}T00:00:00`);
  return Number.isNaN(d.getTime())
    ? iso
    : d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
};

/**
 * What an invoice's row says on the right. `is_overdue` is computed by the
 * server against today, never stored — see the billing module note.
 */
function invoiceTone(invoice: Invoice): {
  label: string;
  tone: 'success' | 'warn' | 'danger' | 'muted';
} {
  if (invoice.status.state === 'void') return { label: 'Void', tone: 'muted' };
  if (invoice.status.state === 'paid') return { label: 'Paid', tone: 'success' };
  if (invoice.is_overdue) return { label: `${invoice.days_overdue}d late`, tone: 'danger' };
  return { label: 'Due', tone: 'warn' };
}

export default function Billing() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const router = useRouter();
  const insets = useSafeAreaInsets();
  const tabBarHeight = useTabBarHeight();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => (st.membership?.gymId ?? null));
  const [error, setError] = useState<string | null>(null);

  const plans = useQuery({
    queryKey: ['plans', gymId],
    queryFn: () => listPlans(gymId!),
    enabled: Boolean(gymId),
  });
  const invoices = useQuery({
    queryKey: ['invoices', gymId],
    queryFn: () => listInvoices(gymId!),
    enabled: Boolean(gymId),
  });
  const subscriptions = useQuery({
    queryKey: ['subscriptions', gymId],
    queryFn: () => listSubscriptions(gymId!),
    enabled: Boolean(gymId),
  });

  const refreshAll = () => {
    void queryClient.invalidateQueries({ queryKey: ['invoices', gymId] });
    void queryClient.invalidateQueries({ queryKey: ['plans', gymId] });
    void queryClient.invalidateQueries({ queryKey: ['subscriptions', gymId] });
  };

  const settle = useMutation({
    mutationFn: (invoice: Invoice) =>
      recordPayment(gymId!, invoice.id, {
        // The balance, not the full amount: a part-paid invoice settles with
        // what is left, and paying it twice would overstate the takings.
        amountMinor: invoice.amount_minor - invoice.paid_minor,
        provider: 'cash',
        receivedOn: localToday(),
      }),
    onSuccess: () => {
      setError(null);
      refreshAll();
    },
    onError: () => setError('Could not record that payment. Please try again.'),
  });

  const discard = useMutation({
    mutationFn: (invoice: Invoice) => voidInvoice(gymId!, invoice.id, 'issued in error'),
    onSuccess: () => {
      setError(null);
      refreshAll();
    },
    onError: () => setError('Could not void that invoice. A paid one must be refunded instead.'),
  });

  const rows = invoices.data ?? [];

  const archive = useMutation({
    mutationFn: (planId: string) => archivePlan(gymId!, planId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['plans', gymId] });
    },
    onError: () => Alert.alert('Could not archive', 'Please try again.'),
  });

  /** The money summary. All derived — nothing here is a stored aggregate. */
  const summary = useMemo(() => {
    const active = (subscriptions.data ?? []).filter((x) => x.is_active);
    // Monthly recurring revenue: what the active subscriptions bill each month.
    const mrr = active.reduce((n, x) => n + x.price_minor, 0);
    const outstanding = rows.filter((i) => i.status.state === 'due');
    const overdue = outstanding.filter((i) => i.is_overdue);
    const owed = overdue.reduce((n, i) => n + (i.amount_minor - i.paid_minor), 0);
    return { activeCount: active.length, mrr, overdue, owed };
  }, [subscriptions.data, rows]);

  const currency = (plans.data ?? [])[0]?.currency ?? 'GBP';
  const money = (minor: number) => {
    const symbol = currency === 'GBP' ? '£' : currency === 'USD' ? '$' : currency === 'EUR' ? '€' : '';
    return `${symbol}${(minor / 100).toFixed(minor % 100 === 0 ? 0 : 2)}`;
  };

  const loading = plans.isLoading || invoices.isLoading || subscriptions.isLoading;

  return (
    <View style={s.screen}>
      <View style={[s.header, { paddingTop: insets.top + t.space.md }]}>
        <GymHeader title="Billing" meta="Owner" subtitle="What this gym sells, who is on it, and what is owed." />
      </View>

      {invoices.isError ? (
        <View style={s.bannerWrap}>
          <ErrorBanner message="Could not load billing." />
        </View>
      ) : null}
      {error ? (
        <View style={s.bannerWrap}>
          <ErrorBanner message={error} />
        </View>
      ) : null}

      {loading ? (
        <Centered>
          <ActivityIndicator color={t.color.accent} />
        </Centered>
      ) : (
        <ScrollView
          contentContainerStyle={[s.content, { paddingBottom: tabBarHeight + t.space.xl }]}
          showsVerticalScrollIndicator={false}
          refreshControl={
            <RefreshControl
              refreshing={invoices.isRefetching}
              onRefresh={refreshAll}
              tintColor={t.color.mut}
            />
          }
        >
          <StatRow
            tone={summary.overdue.length > 0 ? 'focal' : 'quiet'}
            stats={[
              {
                label: 'Monthly',
                value: money(summary.mrr),
                context: `${summary.activeCount} active`,
              },
              {
                label: 'Plans',
                value: String((plans.data ?? []).filter((p) => p.is_offered).length),
                context: 'offered',
              },
              {
                label: 'Overdue',
                value: money(summary.owed),
                context: `${summary.overdue.length} ${summary.overdue.length === 1 ? 'invoice' : 'invoices'}`,
                alert: summary.overdue.length > 0,
              },
            ]}
          />

          {summary.overdue.length > 0 ? (
            <View>
              <Section label="Needs chasing" count={summary.overdue.length} />
              <Card padded={false} tone="focal">
                {summary.overdue.map((invoice, i) => (
                  <ListRow
                    key={invoice.id}
                    title={invoice.member_name || invoice.description}
                    subtitle={`${invoice.description} · ${money(invoice.amount_minor - invoice.paid_minor)} outstanding`}
                    subtitleTone="reason"
                    last={i === summary.overdue.length - 1}
                    onPress={() => confirmSettle(invoice)}
                    right={<Badge tone="danger">{`${invoice.days_overdue}d late`}</Badge>}
                  />
                ))}
              </Card>
            </View>
          ) : null}

          <View>
            <Section
              label="Plans"
              meta={`${(plans.data ?? []).length} total`}
              action={
                <Touchable
                  onPress={() => router.push('/(app)/new-plan')}
                  accessibilityLabel="Add a plan"
                  style={s.addLink}
                >
                  <Text style={s.addLinkText}>New plan</Text>
                </Touchable>
              }
            />
            {(plans.data ?? []).length === 0 ? (
              <EmptyState
                glyph="◫"
                title="Nothing on sale yet"
                hint="Add what this gym sells — open gym, coaching, a drop-in. Until you do, nobody is billed and nobody is gated."
              />
            ) : (
              <Card padded={false}>
              {(plans.data ?? []).map((plan, i, arr) => (
                <ListRow
                  key={plan.id}
                  title={plan.name}
                  // What it unlocks, not what the owner wrote about it: the
                  // grants are what the gate actually reads.
                  subtitle={`${plan.price_label}${plan.interval === 'monthly' ? '/mo' : ' once'} · ${summariseGrants(plan.grants)}`}
                  last={i === arr.length - 1}
                  // Long-press, not a visible delete: stopping a plan being sold
                  // sits one slip away from a row people tap to read.
                  onLongPress={
                    plan.is_offered
                      ? () =>
                          Alert.alert(
                            `Stop offering ${plan.name}?`,
                            plan.active_subscribers > 0
                              ? `${plan.active_subscribers} ${
                                  plan.active_subscribers === 1 ? 'member keeps' : 'members keep'
                                } their membership and price. It just stops being sold to anyone new.`
                              : 'It stops being sold. Nothing else changes.',
                            [
                              { text: 'Keep selling it', style: 'cancel' },
                              {
                                text: 'Stop offering',
                                style: 'destructive',
                                onPress: () => archive.mutate(plan.id),
                              },
                            ],
                          )
                      : undefined
                  }
                  right={
                    plan.is_offered ? (
                      <Badge tone={plan.active_subscribers > 0 ? 'accent' : 'outline'}>
                        {`${plan.active_subscribers} on it`}
                      </Badge>
                    ) : (
                      <Badge tone="muted">Archived</Badge>
                    )
                  }
                />
              ))}
              </Card>
            )}
          </View>

          <View>
            <Section
              label="Memberships"
              meta={`${(subscriptions.data ?? []).filter((x) => x.is_active).length} active`}
              action={
                <Touchable
                  onPress={() => router.push('/(app)/subscribe')}
                  accessibilityLabel="Put a member on a plan"
                >
                  <Text style={s.sectionAction}>Start one</Text>
                </Touchable>
              }
            />
            {(subscriptions.data ?? []).filter((x) => x.is_active).length === 0 ? (
              <EmptyState
                glyph="○"
                title="Nobody on a plan yet"
                hint="Members you have not put on a plan cannot start a workout once you sell anything."
              />
            ) : (
              <Card padded={false}>
                {(subscriptions.data ?? [])
                  .filter((x) => x.is_active)
                  .map((sub, i, arr) => (
                    <ListRow
                      key={sub.id}
                      title={sub.member_name ?? 'Member'}
                      subtitle={`${sub.plan_name} · ${sub.price_label} · since ${shortDate(sub.started_on)}`}
                      last={i === arr.length - 1}
                      right={<Badge tone="success">Active</Badge>}
                    />
                  ))}
              </Card>
            )}
          </View>

          <View>
            <Section label="Payments" meta={`${rows.length} invoices`} />
            {rows.length === 0 ? (
              <EmptyState
                glyph="◷"
                title="Nothing billed yet"
                hint="Put a member on a plan and their first invoice is issued straight away."
              />
            ) : (
              <Card padded={false}>
              {rows.map((invoice, i) => {
                const badge = invoiceTone(invoice);
                return (
                  <ListRow
                    key={invoice.id}
                    title={invoice.member_name || invoice.reference}
                    subtitle={`${invoice.description} · ${shortDate(invoice.issued_on)}${invoice.paid_minor > 0 && invoice.status.state === 'due' ? ` · ${money(invoice.paid_minor)} of ${money(invoice.amount_minor)} paid` : ''}`}
                    last={i === rows.length - 1}
                    onPress={() => confirmSettle(invoice)}
                    right={
                      <View style={s.amountCell}>
                        <Text style={s.amount}>{invoice.amount_label}</Text>
                        <Badge tone={badge.tone}>{badge.label}</Badge>
                      </View>
                    }
                  />
                );
              })}
              </Card>
            )}
          </View>

          <Text style={s.footnote}>
            Cash and card-at-the-desk are recorded here. Card-on-file arrives with Stripe —
            the record shape does not change when it does.
          </Text>
        </ScrollView>
      )}
    </View>
  );

  /**
   * One sheet for the two things a manager does to an invoice, with the
   * consequence spelled out: settling states the amount, voiding says it
   * cannot be undone.
   */
  function confirmSettle(invoice: Invoice) {
    if (invoice.status.state !== 'due') return;
    const balance = invoice.amount_minor - invoice.paid_minor;

    Alert.alert(
      invoice.reference,
      `${invoice.member_name} · ${invoice.description}\n${money(balance)} outstanding.`,
      [
        { text: 'Close', style: 'cancel' },
        {
          text: `Record ${money(balance)} cash`,
          onPress: () => settle.mutate(invoice),
        },
        {
          text: 'Void',
          style: 'destructive',
          onPress: () =>
            Alert.alert(
              'Void this invoice?',
              'It stays on the record as void. This cannot be undone — a paid invoice is refunded instead.',
              [
                { text: 'Keep it', style: 'cancel' },
                { text: 'Void', style: 'destructive', onPress: () => discard.mutate(invoice) },
              ],
            ),
        },
      ],
    );
  }
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    screen: { backgroundColor: t.color.surface, flex: 1 },
    header: { paddingHorizontal: t.space.gutter },
    bannerWrap: { paddingHorizontal: t.space.gutter, paddingTop: t.space.md },
    content: { gap: t.space.xl, paddingHorizontal: t.space.gutter, paddingTop: t.space.md },

    addLink: { paddingVertical: 2 },
    addLinkText: {
      color: t.color.accentDeep,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
    },

    amountCell: { alignItems: 'flex-end', gap: 4 },
    amount: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.md,
      fontVariant: ['tabular-nums'],
      letterSpacing: t.tracking.tight,
    },

    sectionAction: { color: t.color.accentDeep, fontFamily: fonts.bold, fontSize: t.font.sm },
    footnote: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      lineHeight: 17,
    },
  });
