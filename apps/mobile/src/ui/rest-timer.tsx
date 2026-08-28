import * as Haptics from 'expo-haptics';
import { useEffect, useRef } from 'react';
import { Platform, Pressable, StyleSheet, Text, View } from 'react-native';
import Animated, { FadeInDown, FadeOutDown } from 'react-native-reanimated';

import { REST_PRESETS, clockLabel, remainingSeconds, restProgress } from '@/features/timer/clock';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';
import { useNow } from '@/ui/use-now';

/** A rest is over: one firm buzz. Guarded like every haptic — never blocks. */
const buzz = () => {
  if (Platform.OS === 'web') return;
  try {
    Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success).catch(() => {});
  } catch {
    // Device cannot vibrate. The visual state still says "rest done".
  }
};

/**
 * The between-sets rest bar.
 *
 * While it runs it is the loudest thing on the screen — accent-filled, the
 * countdown at display size — because for ninety seconds it is the only thing
 * you care about, and you are reading it from arm's length with the phone on a
 * bench. Everything else on the session screen is deliberately quieter.
 *
 * When the rest ends the whole bar goes **lime**. That is the one place in the
 * app where the colour change *is* the notification: a glance from across the
 * rack has to answer "can I go yet" with no reading at all, and a word that
 * changed from "Rest" to "Go" does not survive being seen out of the corner of
 * an eye. It is also the only element that uses `signal` at full bleed, which
 * is what keeps the colour meaning something.
 *
 * Anchored to `endsAt`, not a countdown variable: pocket the phone for forty
 * seconds and it comes back showing the right number.
 */
export function RestTimer({
  endsAt,
  durationSeconds,
  onRestart,
  onDismiss,
}: {
  endsAt: number;
  durationSeconds: number;
  /** Restart at a (possibly new) length — the preset chips and +15 s both land here. */
  onRestart: (seconds: number) => void;
  onDismiss: () => void;
}) {
  const s = useStyles(styleFactory);
  const now = useNow(250, true);
  const remaining = remainingSeconds(endsAt, now);
  const done = remaining === 0;

  // Buzz exactly once per rest, then bow out on a short delay.
  const buzzed = useRef(false);
  useEffect(() => {
    if (!done) {
      buzzed.current = false;
      return undefined;
    }
    if (!buzzed.current) {
      buzzed.current = true;
      buzz();
    }
    const dismiss = setTimeout(onDismiss, 2500);
    return () => clearTimeout(dismiss);
  }, [done, onDismiss]);

  return (
    <Animated.View entering={FadeInDown.duration(220)} exiting={FadeOutDown.duration(180)}>
      <View
        style={[s.bar, done && s.barDone]}
        accessibilityRole="timer"
        accessibilityLiveRegion="polite"
      >
        <View style={s.topRow}>
          <Text style={[s.label, done && s.onSignal]}>{done ? 'Rest done' : 'Resting'}</Text>
          <Pressable
            onPress={onDismiss}
            accessibilityRole="button"
            accessibilityLabel={done ? 'Dismiss rest timer' : 'Skip the rest'}
            style={s.skip}
            hitSlop={10}
          >
            <Text style={[s.skipText, done && s.onSignal]}>{done ? 'Dismiss' : 'Skip'}</Text>
          </Pressable>
        </View>

        <View style={s.mainRow}>
          <Text style={[s.clock, done && s.onSignal]} numberOfLines={1} adjustsFontSizeToFit>
            {done ? 'Go' : clockLabel(remaining)}
          </Text>

          {!done ? (
            <View style={s.controls}>
              <Pressable
                onPress={() => onRestart(remaining + 15)}
                accessibilityRole="button"
                accessibilityLabel="Add fifteen seconds of rest"
                style={s.control}
              >
                <Text style={s.controlText}>+15s</Text>
              </Pressable>
              {REST_PRESETS.map((preset) => (
                <Pressable
                  key={preset}
                  onPress={() => onRestart(preset)}
                  accessibilityRole="button"
                  accessibilityLabel={`Restart rest at ${clockLabel(preset)}`}
                  style={[s.control, preset === durationSeconds && s.controlActive]}
                >
                  <Text style={s.controlText}>{clockLabel(preset)}</Text>
                </Pressable>
              ))}
            </View>
          ) : null}
        </View>

        {!done ? (
          <View style={s.track}>
            <View
              style={[
                s.fill,
                { width: `${Math.round(restProgress(endsAt, durationSeconds, now) * 100)}%` },
              ]}
            />
          </View>
        ) : null}
      </View>
    </Animated.View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    bar: {
      backgroundColor: t.color.accent,
      borderRadius: t.radius.lg,
      gap: t.space.sm,
      paddingHorizontal: t.space.lg,
      paddingVertical: t.space.lg,
      ...t.elevation(2),
    },
    barDone: { backgroundColor: t.color.signal },
    /** Text on the lime state. Applied on top of the accent-ground styles. */
    onSignal: { color: t.color.onSignal },
    topRow: { alignItems: 'center', flexDirection: 'row', justifyContent: 'space-between' },
    label: {
      color: t.color.onAccent,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      opacity: 0.85,
      textTransform: 'uppercase',
    },
    skip: { alignItems: 'center', flexDirection: 'row', gap: 5, minHeight: 30 },
    skipText: {
      color: t.color.onAccent,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    mainRow: { alignItems: 'center', flexDirection: 'row', gap: t.space.md },
    clock: {
      color: t.color.onAccent,
      fontFamily: fonts.displayHeavy,
      fontSize: 44,
      fontVariant: ['tabular-nums'],
      letterSpacing: -1.4,
    },
    controls: { flex: 1, flexDirection: 'row', gap: 6, justifyContent: 'flex-end' },
    control: {
      alignItems: 'center',
      borderColor: t.color.onAccent,
      borderRadius: t.radius.pill,
      borderWidth: t.border.hair,
      justifyContent: 'center',
      minHeight: 34,
      opacity: 0.65,
      paddingHorizontal: 11,
    },
    controlActive: { opacity: 1 },
    controlText: {
      color: t.color.onAccent,
      fontFamily: fonts.bold,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
    },
    // A groove recessed into the accent fill. A black wash happened to work
    // in both schemes by luck; the token says what it means and stays right
    // if either accent moves.
    track: {
      backgroundColor: t.color.accentTrack,
      borderRadius: 999,
      height: 5,
      marginTop: 2,
      overflow: 'hidden',
    },
    fill: { backgroundColor: t.color.onAccent, borderRadius: 999, height: 5 },
  });
