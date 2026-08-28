import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useLocalSearchParams, useRouter } from 'expo-router';
import * as Linking from 'expo-linking';
import { useEffect, useState } from 'react';
import { Alert, Linking as RNLinking, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import {
  cancelSubscription,
  createCheckoutSession,
  listInvoices,
  listPlans,
  listSubscriptions,
  type Invoice,
} from '@/api/gym';
import { useSession } from '@/session/store';
import {
  Badge,
  Button,
  Callout,
  Card,
  EmptyState,
  ErrorBanner,
  ListRow,
  Screen,
  Section,
} from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

/**
 * Does the plan this member is on include coaching?
 *
 * Read off the PLAN's grants rather than its name: "Coaching" is a label a gym
 * chose, and a gym that calls it "Full" would otherwise never be offered the
 * step down. `grants` is the thing the entitlement resolver actually uses.
 */
function grantsCoaching(
  planName: string,
  plans: { name: string; grants: string[] }[] | undefined,
): boolean {
  return Boolean(
    plans?.find((p) => p.name === planName)?.grants.includes('coached_programming'),
  );
}

const shortDate = (iso: string) => {
  const d = new Date(`${iso}T00:00:00`);
  return Number.isNaN(d.getTime())
    ? iso
    : d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
};

/** Matches billing.tsx's manager-facing version — the vocabulary should agree. */
function invoiceTone(invoice: Invoice): {
  label: string;
  tone: 'success' | 'warn' | 'danger' | 'muted';
} {
  if (invoice.status.state === 'void') return { label: 'Void', tone: 'muted' };
  if (invoice.status.state === 'paid') return { label: 'Paid', tone: 'success' };
  if (invoice.is_overdue) return { label: 'Overdue', tone: 'danger' };
  return { label: 'Due', tone: 'warn' };
}

/**
 * Self-service billing: what a member owes, and a way to pay it.
 *
 * Reuses the same `listInvoices`/`listSubscriptions` calls the manager-facing
 * Billing tab uses — the server already narrows both to "your own" for
 * anyone who is not a manager (`BillingService::list_invoices`), so there is
 * no separate "my invoices" endpoint to keep in step.
 *
 * Paying does not mark anything paid HERE. It opens a real Stripe Checkout
 * page; the invoice only settles once Stripe confirms payment to the server
 * directly, via a webhook this screen has no part in (ADR-0010).
 */
export default function Membership() {
  const s = useStyles(styleFactory);
  const router = useRouter();
  const queryClient = useQueryClient();
  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const params = useLocalSearchParams<{ status?: string }>();

  const [error, setError] = useState<string | null>(null);
  const [payingId, setPayingId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

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

  // Returning from Stripe: refetch so a payment that landed while the browser
  // was open shows up immediately, and say something rather than nothing —
  // "cancel" is a real, common outcome, not a failure to apologise for.
  useEffect(() => {
    if (params.status === 'success') {
      setNotice('Payment received — this can take a few seconds to show as paid below.');
      void queryClient.invalidateQueries({ queryKey: ['invoices', gymId] });
    } else if (params.status === 'cancel') {
      setNotice('Checkout was cancelled — nothing was charged.');
    }
    // Only meant to react to the return-from-browser param, once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [params.status]);

  const pay = useMutation({
    mutationFn: async (invoice: Invoice) => {
      setPayingId(invoice.id);
      const returnUrl = Linking.createURL('membership');
      return createCheckoutSession(gymId!, invoice.id, returnUrl);
    },
    onSuccess: ({ checkout_url }) => {
      setError(null);
      void RNLinking.openURL(checkout_url);
    },
    onError: (err) => {
      setError(
        err instanceof ApiError && err.status === 503
          ? 'Card payment is not set up for this gym yet. Ask at the desk.'
          : 'Could not start checkout. Please try again.',
      );
    },
    onSettled: () => setPayingId(null),
  });

  /*
    What is on sale, so a member ending a coached plan can be shown the solo one
    in the same breath rather than being dropped onto an empty screen. Only
    fetched when they actually hold something to leave.
  */
  const plans = useQuery({
    queryKey: ['plans', gymId],
    queryFn: () => listPlans(gymId!),
    enabled: Boolean(gymId),
  });

  /*
    Ending your own membership.

    This had no client function at all, because the endpoint was managers-only:
    the app could SELL a membership self-service and not stop one. That is not a
    billing safeguard, it is a retention tactic enforced by a missing button.

    The money does not change — access runs to the end of the period already
    paid for, and the server returns that date.
  */
  const leave = useMutation({
    mutationFn: (subscriptionId: string) => cancelSubscription(gymId!, subscriptionId),
    onSuccess: (ended) => {
      setError(null);
      void queryClient.invalidateQueries({ queryKey: ['subscriptions', gymId] });
      void queryClient.invalidateQueries({ queryKey: ['entitlements', gymId] });
      const until =
        ended.status.state === 'cancelled' && 'ends_on' in ended.status
          ? shortDate(ended.status.ends_on)
          : null;
      setNotice(
        until
          ? `Cancelled. You can still train until ${until} — it is already paid for.`
          : 'Cancelled. Nothing further will be charged.',
      );
    },
    onError: (err) => {
      setError(
        err instanceof ApiError
          ? err.message
          : 'Could not cancel that. Please try again.',
      );
    },
  });

  const activeSub = (subscriptions.data ?? []).find((x) => x.is_active);

  /*
    The cheapest thing on sale that opens the door but buys no coaching — the
    "solo training" a member is stepping down TO. Picked by grants, not by
    name, and cheapest-first so the step down is actually a saving.
  */
  const soloPlan = (plans.data ?? [])
    .filter(
      (p) =>
        p.grants.includes('gym_access') &&
        !p.grants.includes('coached_programming') &&
        p.interval !== 'once',
    )
    .sort((a, b) => a.price_minor - b.price_minor)[0];

  const payable = (invoices.data ?? []).filter((i) => i.status.state === 'due');
  const settled = (invoices.data ?? []).filter((i) => i.status.state !== 'due');

  return (
    <Screen scroll>
      <View style={s.content}>
        {notice ? (
          <Callout tone={params.status === 'success' ? 'success' : 'accent'}>{notice}</Callout>
        ) : null}
        {error ? <ErrorBanner message={error} /> : null}

        <View>
          <Section label="Your plan" />
          {activeSub ? (
            <>
              <Card padded={false}>
                <ListRow
                  title={activeSub.plan_name}
                  subtitle={`${activeSub.price_label} · started ${shortDate(activeSub.started_on)}`}
                  last
                  right={<Badge tone="success">Active</Badge>}
                />
              </Card>

              {/*
                Leaving, and the step down.

                Two separate things on purpose. "Cancel" ends the billing;
                "switch to solo" is that plus picking a cheaper plan, which is
                what most people actually mean by "I want to stop paying for
                coaching" — they are not leaving the gym. Offering only the
                first would make dropping coaching look like quitting.
              */}
              <View style={s.leave}>
                {soloPlan && grantsCoaching(activeSub.plan_name, plans.data) ? (
                  <Button
                    label={`Switch to ${soloPlan.name}`}
                    detail={`${soloPlan.price_label} · train on your own`}
                    variant="secondary"
                    disabled={leave.isPending}
                    onPress={() =>
                      Alert.alert(
                        `Switch to ${soloPlan.name}?`,
                        `Ends ${activeSub.plan_name} and puts you on ${soloPlan.name} at ${soloPlan.price_label}. You keep gym access and lose coaching. You can still train on what you have already paid for.`,
                        [
                          { text: 'Keep coaching', style: 'cancel' },
                          {
                            text: 'Switch',
                            onPress: () => {
                              leave.mutate(activeSub.id, {
                                onSuccess: () => router.push('/(app)/join-plan'),
                              });
                            },
                          },
                        ],
                      )
                    }
                  />
                ) : null}

                <Button
                  label={leave.isPending ? 'Cancelling…' : 'Cancel membership'}
                  variant="ghost"
                  disabled={leave.isPending}
                  onPress={() =>
                    Alert.alert(
                      'Cancel your membership?',
                      'Nothing further will be charged. You can keep training until the end of the period you have already paid for, and rejoin whenever you like.',
                      [
                        { text: 'Keep it', style: 'cancel' },
                        {
                          text: 'Cancel membership',
                          style: 'destructive',
                          onPress: () => leave.mutate(activeSub.id),
                        },
                      ],
                    )
                  }
                />
                <Text style={s.leaveSays}>
                  You can do this yourself, any time. Cancelling does not remove
                  you from the gym — your history and your programmes stay.
                </Text>
              </View>
            </>
          ) : (
            <EmptyState
              glyph="—"
              title="No active plan"
              hint="If this gym sells memberships you can join one yourself, and start training straight away."
              action={
                <Button
                  label="See what is on sale"
                  onPress={() => router.push('/(app)/join-plan')}
                />
              }
            />
          )}
        </View>

        <View>
          <Section label="Outstanding" count={payable.length} />
          {payable.length === 0 ? (
            <EmptyState glyph="✓" title="Nothing due" hint="You are paid up." />
          ) : (
            <Card padded={false}>
              {payable.map((invoice, index) => (
                <ListRow
                  key={invoice.id}
                  title={invoice.description}
                  subtitle={`${invoice.amount_label} · due ${shortDate(invoice.due_on)}`}
                  subtitleTone={invoice.is_overdue ? 'reason' : 'muted'}
                  last={index === payable.length - 1}
                  right={
                    <Button
                      label="Pay"
                      size="small"
                      busy={payingId === invoice.id}
                      onPress={() => pay.mutate(invoice)}
                    />
                  }
                />
              ))}
            </Card>
          )}
        </View>

        {settled.length > 0 ? (
          <View>
            <Section label="History" count={settled.length} />
            <Card padded={false}>
              {settled.map((invoice, index) => {
                const tone = invoiceTone(invoice);
                return (
                  <ListRow
                    key={invoice.id}
                    title={invoice.description}
                    subtitle={`${invoice.amount_label} · ${shortDate(invoice.due_on)}`}
                    last={index === settled.length - 1}
                    right={<Badge tone={tone.tone}>{tone.label}</Badge>}
                  />
                );
              })}
            </Card>
          </View>
        ) : null}
      </View>
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    leave: { gap: t.space.sm, paddingTop: t.space.md },
    leaveSays: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.xs,
      lineHeight: 17,
    },
    content: { gap: t.space.xl },
  });
