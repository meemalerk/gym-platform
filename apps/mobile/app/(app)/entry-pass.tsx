import { useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';
import QRCode from 'react-native-qrcode-svg';

import { getEntryPass } from '@/api/gym';
import { useSession } from '@/session/store';
import { Centered, ErrorBanner } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * The pass is short-lived by design (90s server-side) — screenshotting this
 * screen buys almost nothing, because by the time anyone acted on it the
 * server would already refuse the token. Refetching at a fixed interval,
 * comfortably inside that window, is what keeps the code on screen always
 * good for at least a few more seconds.
 */
const REFRESH_MS = 45_000;

export default function EntryPass() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const gymName = useSession((st) => st.membership?.gymName ?? '');
  const [now, setNow] = useState(() => Date.now());

  const pass = useQuery({
    queryKey: ['entry-pass', gymId],
    queryFn: () => getEntryPass(gymId!),
    enabled: Boolean(gymId),
    refetchInterval: REFRESH_MS,
    // A stale pass showing on screen is worse than a blank moment while a
    // fresh one arrives — never serve yesterday's QR code from cache.
    staleTime: 0,
  });

  // Redraws the "renews in Ns" line between fetches; does not itself fetch.
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  if (pass.isPending) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.ink} />
      </Centered>
    );
  }

  if (pass.isError || !pass.data) {
    return (
      <Centered>
        <ErrorBanner message="Could not load your entry pass. Pull down to try again." />
      </Centered>
    );
  }

  const fetchedAt = pass.dataUpdatedAt;
  const secondsLeft = Math.max(
    0,
    Math.round((fetchedAt + pass.data.expires_in_seconds * 1000 - now) / 1000),
  );

  return (
    <Centered>
      <View style={s.card}>
        {/*
          Black on white, in both schemes, from the same pair the viewfinder
          uses and for the same reason: a QR code is read by a sensor, not by
          a person. Following the theme here would have printed near-black
          modules on a near-black card every night — the code would have been
          on screen, looked deliberate, and scanned as nothing.
        */}
        <QRCode
          value={pass.data.token}
          size={232}
          backgroundColor={t.color.onViewfinder}
          color={t.color.viewfinder}
        />
      </View>
      <Text style={s.gymName}>{gymName}</Text>
      <Text style={s.hint}>Hold this up to the reader at the door</Text>
      <View style={s.countdownPill}>
        <Text style={s.countdown}>
          {secondsLeft > 0 ? `Renews in ${secondsLeft}s` : 'Renewing…'}
        </Text>
      </View>
    </Centered>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    card: {
      // White in both schemes — see the note beside the QR itself.
      backgroundColor: t.color.onViewfinder,
      borderRadius: t.radius.xl,
      padding: t.space.xl,
      ...t.elevation(3),
    },
    gymName: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      letterSpacing: t.tracking.display,
      marginTop: t.space.lg,
      textAlign: 'center',
    },
    hint: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      textAlign: 'center',
    },
    countdownPill: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.pill,
      marginTop: t.space.sm,
      paddingHorizontal: 12,
      paddingVertical: 6,
    },
    countdown: {
      color: t.color.mut2,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      fontVariant: ['tabular-nums'],
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
  });
