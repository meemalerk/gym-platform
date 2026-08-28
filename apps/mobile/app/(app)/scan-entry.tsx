import { CameraView, useCameraPermissions } from 'expo-camera';
import { useState } from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';

import { ApiError } from '@/api/client';
import { scanEntry, type ScanResult } from '@/api/gym';
import { useSession } from '@/session/store';
import { Button, Centered } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/** How long a result stays on screen before the camera goes live again. */
const RESULT_DISPLAY_MS = 3000;

export default function ScanEntry() {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const gymId = useSession((st) => st.membership?.gymId ?? null);
  const [permission, requestPermission] = useCameraPermissions();
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<ScanResult | { error: string } | null>(null);

  // A QR code sits in frame for many video frames in a row; without this the
  // same read fires the scan handler dozens of times before a person can
  // move the camera away.
  const onScanned = async ({ data }: { data: string }) => {
    if (busy || result) return;
    setBusy(true);
    try {
      const outcome = await scanEntry(gymId!, data);
      setResult(outcome);
    } catch (err) {
      setResult({
        error:
          err instanceof ApiError
            ? err.message
            : 'Could not reach the server. Check your connection and try again.',
      });
    } finally {
      setBusy(false);
      setTimeout(() => setResult(null), RESULT_DISPLAY_MS);
    }
  };

  if (!permission) {
    return (
      <Centered>
        <ActivityIndicator color={t.color.ink} />
      </Centered>
    );
  }

  if (!permission.granted) {
    return (
      <Centered>
        <Text style={s.permissionTitle}>Camera access needed</Text>
        <Text style={s.permissionBody}>
          Scanning a member&apos;s entry pass needs the camera. Nothing is recorded except the
          scan itself.
        </Text>
        <Button label="Allow camera" onPress={() => void requestPermission()} />
      </Centered>
    );
  }

  return (
    <View style={s.screen}>
      <CameraView
        style={StyleSheet.absoluteFill}
        facing="back"
        barcodeScannerSettings={{ barcodeTypes: ['qr'] }}
        onBarcodeScanned={result ? undefined : onScanned}
      />

      <View pointerEvents="none" style={s.frame} />

      {!result ? (
        <Text style={s.hint}>{busy ? 'Checking…' : 'Point the camera at a member’s pass'}</Text>
      ) : null}

      {result ? (
        (() => {
          /*
            Three outcomes, three grounds, each a verified text-on-fill pair:

              admitted  lime, near-black text. The same colour the rest timer
                        turns when a rest is over, and for the same reason —
                        somebody standing at a turnstile reads a colour before
                        they read a word.
              refused   the rose wash. Loud enough to stop the queue moving,
                        quiet enough that it is plainly not the "go" state.
              unscanned the neutral surface: nothing was decided, so nothing
                        should look decided.
          */
          const admitted = !('error' in result) && result.allowed;
          const refused = !('error' in result) && !result.allowed;
          return (
            <View
              style={[
                s.resultCard,
                admitted && s.resultAllowed,
                refused && s.resultRefused,
                'error' in result && s.resultNeutral,
              ]}
            >
              {'error' in result ? (
                <>
                  <Text style={s.resultTitle}>Couldn&apos;t read that</Text>
                  <Text style={s.resultBody}>{result.error}</Text>
                </>
              ) : (
                <>
                  <Text
                    style={[
                      s.resultTitle,
                      admitted && s.onSignalText,
                      refused && s.onRefusedText,
                    ]}
                  >
                    {result.allowed ? 'Welcome in' : 'Not admitted'}
                  </Text>
                  <Text
                    style={[
                      s.resultName,
                      admitted && s.onSignalText,
                      refused && s.onRefusedText,
                    ]}
                  >
                    {result.member_name}
                  </Text>
                  <Text
                    style={[
                      s.resultBody,
                      admitted && s.onSignalText,
                      refused && s.onRefusedText,
                    ]}
                  >
                    {result.reason}
                  </Text>
                </>
              )}
            </View>
          );
        })()
      ) : null}
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    screen: { backgroundColor: t.color.viewfinder, flex: 1 },
    frame: {
      position: 'absolute',
      top: '28%',
      left: '12%',
      right: '12%',
      bottom: '40%',
      borderColor: t.color.onViewfinder,
      borderRadius: t.radius.xl,
      // Twice a control edge. The reticle is drawn over live video rather than
      // over a surface the app controls, so it has to survive a bright doorway.
      borderWidth: t.border.ink * 2,
    },
    hint: {
      position: 'absolute',
      bottom: 64,
      left: t.space.xl,
      right: t.space.xl,
      color: t.color.onViewfinder,
      fontFamily: fonts.semibold,
      fontSize: t.font.md,
      textAlign: 'center',
    },
    resultCard: {
      borderRadius: t.radius.xl,
      bottom: 48,
      left: t.space.lg,
      padding: t.space.xl,
      position: 'absolute',
      right: t.space.lg,
      ...t.elevation(3),
    },
    resultAllowed: { backgroundColor: t.color.signal },
    resultRefused: { backgroundColor: t.color.dangerHi },
    resultNeutral: { backgroundColor: t.color.surface2 },
    resultTitle: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
    },
    resultName: {
      color: t.color.ink,
      fontFamily: fonts.bold,
      fontSize: t.font.md,
      marginTop: 4,
    },
    resultBody: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      marginTop: 6,
    },
    onSignalText: { color: t.color.onSignal },
    onRefusedText: { color: t.color.dangerInk },
    permissionTitle: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xl,
      letterSpacing: t.tracking.display,
      textAlign: 'center',
    },
    permissionBody: {
      color: t.color.mut,
      fontFamily: fonts.regular,
      fontSize: t.font.sm,
      lineHeight: 19,
      marginTop: 8,
      marginBottom: t.space.md,
      textAlign: 'center',
    },
  });
