/**
 * The Signal primitives (ADR-0030).
 *
 * The language in one sentence: soft-cornered surfaces that stack by fill and
 * elevation, a violet-indigo accent, one electric lime that only means *live*,
 * Bricolage Grotesque on the numbers and Plus Jakarta Sans on everything else.
 *
 * Three rules hold the whole thing together, and each is enforced by
 * `scripts/verify-design-consistency.mjs` rather than remembered:
 *
 *   1. **Containment is fill, not outline.** A card is a lighter ground with a
 *      hairline and a cast; it is never a box drawn in ink. So `Card` has no
 *      "bordered" variant to reach for.
 *   2. **One focal element per screen.** `tone="focal"` exists once per screen.
 *      If two things shout, nothing does.
 *   3. **`signal` is a fill, never a mark.** It appears as `LivePill` and
 *      nowhere else — see the note in verify-contrast.mjs for the measurement
 *      that forces this.
 *
 * Every primitive resolves its scheme through `useTokens()`.
 */

import Feather from '@expo/vector-icons/Feather';
import * as Haptics from 'expo-haptics';
import { useState } from 'react';
import {
  ActivityIndicator,
  Platform,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
  type TextInputProps,
} from 'react-native';
import Animated, {
  FadeInDown,
  useAnimatedStyle,
  useSharedValue,
  withSpring,
} from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import Svg, { Circle } from 'react-native-svg';

import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

const AnimatedPressable = Animated.createAnimatedComponent(Pressable);

/** Fire a haptic tick, and **never** let it break the button. */
const tap = (style: Haptics.ImpactFeedbackStyle = Haptics.ImpactFeedbackStyle.Light) => {
  if (Platform.OS === 'web') return;
  try {
    Haptics.impactAsync(style).catch(() => {});
  } catch {
    // Device cannot vibrate. Carry on.
  }
};

// ---------------------------------------------------------------- structure

/** A screen with correct safe-area padding on the Signal ground. */
export function Screen({
  children,
  scroll = false,
  edges = ['top', 'bottom'],
}: {
  children: React.ReactNode;
  scroll?: boolean;
  edges?: ('top' | 'bottom')[];
}) {
  const t = useTokens();
  const s = useStyles(screenStyles);
  const insets = useSafeAreaInsets();
  const padding = {
    paddingTop: edges.includes('top') ? insets.top + t.space.md : t.space.lg,
    paddingBottom: edges.includes('bottom') ? insets.bottom + t.space.lg : t.space.lg,
  };

  if (scroll) {
    return (
      <ScrollView
        style={s.flex}
        contentContainerStyle={[s.inner, padding]}
        keyboardShouldPersistTaps="handled"
        showsVerticalScrollIndicator={false}
      >
        {children}
      </ScrollView>
    );
  }
  return <View style={[s.flex, s.inner, padding]}>{children}</View>;
}

const screenStyles = (t: Tokens) =>
  StyleSheet.create({
    flex: { flex: 1, backgroundColor: t.color.surface },
    inner: {
      backgroundColor: t.color.surface,
      gap: t.space.lg,
      paddingHorizontal: t.space.gutter,
    },
  });

export function Centered({ children }: { children: React.ReactNode }) {
  const s = useStyles(centeredStyles);
  return <View style={s.centered}>{children}</View>;
}

const centeredStyles = (t: Tokens) =>
  StyleSheet.create({
    centered: {
      alignItems: 'center',
      backgroundColor: t.color.surface,
      flex: 1,
      gap: t.space.md,
      justifyContent: 'center',
      padding: t.space.xl,
    },
  });

/** Staggered entrance. Small, quick — motion should reward, not delay. */
export function Appear({ children, index = 0 }: { children: React.ReactNode; index?: number }) {
  return (
    <Animated.View entering={FadeInDown.delay(index * 45).duration(320).springify()}>
      {children}
    </Animated.View>
  );
}

/** A hairline between things that are already inside the same container. */
export function Divider({ inset = 0 }: { inset?: number }) {
  const t = useTokens();
  return (
    <View style={{ backgroundColor: t.color.line, height: StyleSheet.hairlineWidth, marginLeft: inset }} />
  );
}

// ------------------------------------------------------------------- text

/**
 * The uppercase micro-label that opens a section or annotates a figure.
 * `tone`: muted context (default), accent, ink.
 */
export function Kicker({
  children,
  tone = 'muted',
}: {
  children: React.ReactNode;
  tone?: 'muted' | 'accent' | 'ink';
}) {
  const t = useTokens();
  const s = useStyles(textStyles);
  const color =
    tone === 'accent' ? t.color.accentDeep : tone === 'ink' ? t.color.ink : t.color.mut;
  return <Text style={[s.kicker, { color }]}>{children}</Text>;
}

/** The one big thing at the top of a screen. */
export function Display({ children }: { children: React.ReactNode }) {
  const s = useStyles(textStyles);
  return <Text style={s.display}>{children}</Text>;
}

/** A heading inside a screen — the title of a card or a block. */
export function Heading({ children }: { children: React.ReactNode }) {
  const s = useStyles(textStyles);
  return <Text style={s.heading}>{children}</Text>;
}

/** Body copy. */
export function Muted({ children }: { children: React.ReactNode }) {
  const s = useStyles(textStyles);
  return <Text style={s.muted}>{children}</Text>;
}

/** A form label. Sentence case — a field label is a word, not a sign. */
export function Label({ children }: { children: React.ReactNode }) {
  const s = useStyles(textStyles);
  return <Text style={s.label}>{children}</Text>;
}

const textStyles = (t: Tokens) =>
  StyleSheet.create({
    kicker: {
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    display: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: 32,
      letterSpacing: t.tracking.display,
      lineHeight: 35,
    },
    heading: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.lg,
      letterSpacing: t.tracking.tight,
    },
    muted: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
    },
    label: {
      color: t.color.mut2,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm,
    },
  });

// ------------------------------------------------------------------ inputs

/**
 * A label over a recessed input.
 *
 * The input is a **fill**, not an outlined box: a trough in the card reads as
 * "put something here" without adding another rectangle to a screen that
 * already has several. The one border it ever grows is the focus ring, which
 * is the moment a quiet control should assert itself.
 */
export function Field({
  label,
  error,
  inputStyle,
  hint,
  ...inputProps
}: TextInputProps & { label: string; error?: string; hint?: string; inputStyle?: object }) {
  const t = useTokens();
  const s = useStyles(fieldStyles);
  const [focused, setFocused] = useState(false);

  return (
    <View style={s.field}>
      <Label>{label}</Label>
      <TextInput
        style={[s.input, inputStyle, focused && s.inputFocused, error ? s.inputError : null]}
        placeholderTextColor={t.color.faint}
        selectionColor={t.color.accent}
        onFocus={() => setFocused(true)}
        {...inputProps}
        onBlur={(e) => {
          setFocused(false);
          inputProps.onBlur?.(e);
        }}
      />
      {error ? (
        <Text style={s.error}>{error}</Text>
      ) : hint ? (
        <Text style={s.hint}>{hint}</Text>
      ) : null}
    </View>
  );
}

const fieldStyles = (t: Tokens) =>
  StyleSheet.create({
    field: { gap: 7 },
    input: {
      backgroundColor: t.color.sunken,
      borderColor: t.color.sunken,
      borderRadius: t.radius.md,
      borderWidth: t.border.ink,
      color: t.color.ink,
      fontFamily: fonts.medium,
      fontSize: t.font.md,
      paddingHorizontal: 14,
      paddingVertical: 13,
    },
    inputFocused: { backgroundColor: t.color.surface2, borderColor: t.color.accent },
    inputError: { borderColor: t.color.danger },
    error: { color: t.color.danger, fontFamily: fonts.semibold, fontSize: t.font.sm },
    hint: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.sm },
  });

// ----------------------------------------------------------------- buttons

/**
 * The primary action: a full-width accent bar with a centred label.
 *
 * The old language trailed an arrow on every button, including the ones that
 * did not go anywhere. An arrow is a promise about navigation; on "Save" it is
 * a small lie, and forty small lies is why an interface stops being read. The
 * arrow now lives only on `ListRow`, which really does open something.
 *
 * `secondary` is a bordered box on the card ground; `ghost` is bare accent
 * text; `danger` is for the one destructive action on a screen.
 */
export function Button({
  label,
  onPress,
  busy,
  variant = 'primary',
  disabled,
  detail,
  size = 'regular',
}: {
  label: string;
  onPress: () => void;
  busy?: boolean;
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
  disabled?: boolean;
  /** Right-side preview text, e.g. "70 kg × 8". Pushes the label left. */
  detail?: string;
  size?: 'regular' | 'small';
}) {
  const t = useTokens();
  const s = useStyles(buttonStyles);
  const scale = useSharedValue(1);
  const animated = useAnimatedStyle(() => ({ transform: [{ scale: scale.value }] }));
  const inert = busy || disabled;

  const labelStyle = inert
    ? s.labelInert
    : variant === 'primary'
      ? s.labelPrimary
      : variant === 'danger'
        ? s.labelDanger
        : variant === 'secondary'
          ? s.labelSecondary
          : s.labelGhost;

  return (
    <AnimatedPressable
      accessibilityRole="button"
      accessibilityState={{ disabled: Boolean(inert) }}
      disabled={inert}
      onPressIn={() => (scale.value = withSpring(0.975, t.motion.press))}
      onPressOut={() => (scale.value = withSpring(1, t.motion.press))}
      onPress={() => {
        tap();
        onPress();
      }}
      style={[
        animated,
        s.base,
        size === 'small' && s.small,
        variant === 'primary' && (inert ? s.primaryInert : s.primary),
        variant === 'danger' && (inert ? s.primaryInert : s.danger),
        variant === 'secondary' && s.secondary,
        variant === 'ghost' && s.ghost,
        inert && variant !== 'primary' && variant !== 'danger' && s.inert,
        detail ? s.spread : s.centred,
      ]}
    >
      {busy ? (
        <ActivityIndicator
          color={variant === 'primary' || variant === 'danger' ? t.color.onAccent : t.color.accent}
        />
      ) : (
        <>
          <Text style={[s.label, labelStyle, size === 'small' && s.labelSmall]}>{label}</Text>
          {detail ? <Text style={[s.detail, labelStyle]}>{detail}</Text> : null}
        </>
      )}
    </AnimatedPressable>
  );
}

const buttonStyles = (t: Tokens) =>
  StyleSheet.create({
    base: {
      alignItems: 'center',
      borderRadius: t.radius.md,
      flexDirection: 'row',
      gap: t.space.sm,
      minHeight: 52,
      paddingHorizontal: t.space.lg,
      paddingVertical: 15,
    },
    small: { minHeight: 40, paddingHorizontal: t.space.md, paddingVertical: 9 },
    centred: { justifyContent: 'center' },
    spread: { justifyContent: 'space-between' },
    primary: { backgroundColor: t.color.accent },
    danger: { backgroundColor: t.color.danger },
    // A washed-out accent reads as muddy rather than "unavailable", so a
    // disabled primary drops to the recessed ground — where the fill alone is
    // enough to keep it reading as a control, because `sunken` is a step off
    // every surface it can sit on.
    primaryInert: { backgroundColor: t.color.sunken },
    secondary: {
      backgroundColor: 'transparent',
      borderColor: t.color.rule,
      borderWidth: t.border.ink,
    },
    ghost: { minHeight: 40, paddingHorizontal: t.space.sm, paddingVertical: 8 },
    inert: { opacity: 0.5 },
    label: { fontFamily: fonts.bold, fontSize: t.font.md, letterSpacing: t.tracking.tight },
    labelSmall: { fontSize: t.font.sm + 0.5 },
    labelPrimary: { color: t.color.onAccent },
    labelDanger: { color: t.color.onAccent },
    labelSecondary: { color: t.color.ink },
    labelGhost: { color: t.color.accentDeep },
    labelInert: { color: t.color.faint },
    detail: { fontFamily: fonts.bold, fontSize: t.font.md, fontVariant: ['tabular-nums'] },
  });

/** A large tappable surface with its own spring — cards, rows, choices. */
export function Touchable({
  children,
  onPress,
  onLongPress,
  disabled,
  style,
  accessibilityRole = 'button',
  accessibilityState,
  accessibilityHint,
  accessibilityLabel,
}: {
  children: React.ReactNode;
  /** Optional so a row can be long-press-only — see ListRow. */
  onPress?: () => void;
  onLongPress?: () => void;
  disabled?: boolean;
  style?: object | object[];
  accessibilityRole?: 'button' | 'radio' | 'checkbox';
  accessibilityState?: object;
  accessibilityHint?: string;
  accessibilityLabel?: string;
}) {
  const t = useTokens();
  const scale = useSharedValue(1);
  const animated = useAnimatedStyle(() => ({ transform: [{ scale: scale.value }] }));

  return (
    <AnimatedPressable
      accessibilityRole={accessibilityRole}
      accessibilityState={accessibilityState}
      accessibilityHint={accessibilityHint}
      accessibilityLabel={accessibilityLabel}
      disabled={disabled}
      onPressIn={() => !disabled && (scale.value = withSpring(0.99, t.motion.press))}
      onPressOut={() => (scale.value = withSpring(1, t.motion.press))}
      onPress={() => {
        if (disabled) return;
        tap(Haptics.ImpactFeedbackStyle.Soft);
        onPress?.();
      }}
      onLongPress={
        onLongPress
          ? () => {
              if (disabled) return;
              // Heavier than a tap: the press has done something different, and
              // the hand should know before the eye does.
              tap(Haptics.ImpactFeedbackStyle.Medium);
              onLongPress();
            }
          : undefined
      }
      style={[animated, style]}
    >
      {children}
    </AnimatedPressable>
  );
}

// ------------------------------------------------------------------ pieces

/**
 * Container tones — the hierarchy contract.
 *
 * A screen may have **one** `focal` container. Everything else is `quiet`.
 * Uniform framing (the mistake in the old language) means no emphasis at all:
 * if every box is outlined, the eye has nowhere to land.
 *
 * How containment is drawn differs by scheme, deliberately — see the palette
 * note in theme.ts. Light lifts a white card off a tinted ground; dark cannot
 * cast a shadow on near-black, so it steps the surface instead.
 */
export type ContainerTone = 'quiet' | 'focal';

export function containerStyle(t: Tokens, tone: ContainerTone = 'quiet') {
  if (tone === 'focal') {
    return {
      backgroundColor: t.color.surface2,
      borderColor: t.color.accent,
      borderRadius: t.radius.lg,
      borderWidth: t.border.ink,
      ...t.elevation(2),
    };
  }
  return {
    backgroundColor: t.color.surface2,
    borderColor: t.color.line,
    borderRadius: t.radius.lg,
    borderWidth: t.border.hair,
    ...t.elevation(1),
  };
}

/** A contained box. `focal` is the one element on the screen that shouts. */
export function Card({
  children,
  style,
  tone = 'quiet',
  padded = true,
}: {
  children: React.ReactNode;
  style?: object;
  tone?: ContainerTone;
  /** Off for a card whose children are full-bleed rows. */
  padded?: boolean;
}) {
  const t = useTokens();
  const s = useStyles(cardStyles);
  return (
    <View style={[padded ? s.padded : s.bare, containerStyle(t, tone), style]}>
      {children}
    </View>
  );
}

const cardStyles = (t: Tokens) =>
  StyleSheet.create({
    padded: { gap: t.space.md, padding: t.space.lg },
    // No gap: a full-bleed card holds rows that draw their own hairline, and a
    // gap would leave the line floating in the middle of the space instead of
    // sitting on the join.
    bare: { gap: 0, paddingHorizontal: t.space.lg },
  });

/**
 * Section heading: a title, a count, and an optional action.
 *
 * The old one drew a 2px ink rule under every label, which is how a screen
 * ends up looking like a form. Grouping is now the card the rows sit in; this
 * just names the group, so it stays quiet and gets out of the way.
 */
export function Section({
  label,
  count,
  meta,
  action,
}: {
  label: string;
  count?: number;
  /** Right-aligned context, e.g. "3 days", "A – Z". Overrides `count`. */
  meta?: string;
  action?: React.ReactNode;
}) {
  const s = useStyles(sectionStyles);
  return (
    <View style={s.section}>
      <Text style={s.label} accessibilityRole="header">
        {label}
      </Text>
      {typeof count === 'number' && meta == null ? (
        <View style={s.count}>
          <Text style={s.countText}>{count}</Text>
        </View>
      ) : null}
      <View style={s.spacer} />
      {meta != null ? <Text style={s.meta}>{meta}</Text> : null}
      {action}
    </View>
  );
}

const sectionStyles = (t: Tokens) =>
  StyleSheet.create({
    section: {
      alignItems: 'center',
      flexDirection: 'row',
      gap: t.space.sm,
      paddingBottom: t.space.sm,
    },
    label: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.md,
      letterSpacing: t.tracking.tight,
    },
    count: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.pill,
      minWidth: 22,
      paddingHorizontal: 7,
      paddingVertical: 1,
    },
    countText: {
      color: t.color.mut2,
      fontFamily: fonts.bold,
      fontSize: t.font.xs,
      fontVariant: ['tabular-nums'],
      textAlign: 'center',
    },
    spacer: { flex: 1 },
    meta: {
      color: t.color.mut,
      fontFamily: fonts.semibold,
      fontSize: t.font.xs,
    },
  });

/**
 * The badge family — a pill, because a status is a word you glance at rather
 * than a box you read.
 *
 * `tone` names the *meaning*, never the colour, so the palette can be retuned
 * without hunting through screens: `success` (settled, approved), `warn`
 * (waiting on somebody), `danger` (overdue, failed), `accent` (highlighted
 * standing), `outline` / `muted` (neutral), `ink` (hard, e.g. "owner").
 */
export function Badge({
  children,
  tone = 'outline',
}: {
  children: React.ReactNode;
  tone?: 'accent' | 'outline' | 'ink' | 'muted' | 'success' | 'warn' | 'danger';
}) {
  const s = useStyles(badgeStyles);
  return (
    <View style={[s.base, s[tone]]}>
      <Text style={[s.text, s[`${tone}Text`]]} numberOfLines={1}>
        {children}
      </Text>
    </View>
  );
}

/** Legacy alias — old call sites say Pill with tone muted|accent. */
export function Pill({
  children,
  tone = 'muted',
}: {
  children: React.ReactNode;
  tone?: 'muted' | 'accent';
}) {
  return <Badge tone={tone === 'accent' ? 'accent' : 'muted'}>{children}</Badge>;
}

const badgeStyles = (t: Tokens) =>
  StyleSheet.create({
    base: {
      alignSelf: 'flex-start',
      borderRadius: t.radius.pill,
      paddingHorizontal: 9,
      paddingVertical: 3,
    },
    accent: { backgroundColor: t.color.accentBadge },
    outline: { borderColor: t.color.rule, borderWidth: t.border.hair },
    ink: { backgroundColor: t.color.ink },
    muted: { backgroundColor: t.color.sunken },
    success: { backgroundColor: t.color.successHi },
    warn: { backgroundColor: t.color.warnHi },
    danger: { backgroundColor: t.color.dangerHi },
    text: {
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.wide,
      textTransform: 'uppercase',
    },
    accentText: { color: t.color.accentBadgeInk },
    outlineText: { color: t.color.mut2 },
    inkText: { color: t.color.inkInv },
    mutedText: { color: t.color.mut2 },
    successText: { color: t.color.successInk },
    warnText: { color: t.color.warnInk },
    dangerText: { color: t.color.dangerInk },
  });

/**
 * **Live.** The one place `signal` is painted.
 *
 * A lime bright enough to read as "now" on a near-black screen cannot also be
 * a 3:1 mark on a white one, so it is never a bare dot — it is a fill with
 * near-black text and a near-black hairline, which is legible on any ground
 * and looks the same in both schemes. That sameness is the feature: "live"
 * should not change costume at dusk.
 */
export function LivePill({ children = 'Live' }: { children?: React.ReactNode }) {
  const s = useStyles(liveStyles);
  return (
    <View style={s.pill}>
      <View style={s.dot} />
      <Text style={s.text}>{children}</Text>
    </View>
  );
}

const liveStyles = (t: Tokens) =>
  StyleSheet.create({
    pill: {
      alignItems: 'center',
      alignSelf: 'flex-start',
      backgroundColor: t.color.signal,
      borderColor: t.color.onSignal,
      borderRadius: t.radius.pill,
      borderWidth: t.border.hair,
      flexDirection: 'row',
      gap: 5,
      paddingHorizontal: 8,
      paddingVertical: 3,
    },
    dot: {
      backgroundColor: t.color.onSignal,
      borderRadius: 999,
      height: 5,
      width: 5,
    },
    text: {
      color: t.color.onSignal,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.wide,
      textTransform: 'uppercase',
    },
  });

/**
 * A row of stat tiles inside one card, separated by hairlines: label on top,
 * a big tabular value pinned to the bottom, an optional context line.
 */
export function StatRow({
  stats,
  tone = 'quiet',
}: {
  stats: {
    label: string;
    value: string;
    unit?: string;
    context?: string;
    /** Accent the label + value (the "needs attention" tile). */
    alert?: boolean;
    /** Render the context line in accentDeep (deltas: "+2.4 this week"). */
    delta?: boolean;
  }[];
  tone?: ContainerTone;
}) {
  const t = useTokens();
  const s = useStyles(statStyles);
  return (
    <View style={[s.frame, containerStyle(t, tone)]}>
      {stats.map((stat, i) => (
        <View key={stat.label} style={[s.tile, i > 0 && s.tileDivided]}>
          {/*
            Label on top, value pinned to the bottom of a fixed-height tile.
            Anything else lets a two-line label ("Trained this wk") shove its
            own number down while its neighbours stay put — the row then reads
            as a rendering fault rather than as data. Labels get two lines to
            wrap into; the numbers still line up across the row.
          */}
          <Text style={[s.label, stat.alert && s.labelAlert]} numberOfLines={2}>
            {stat.label}
          </Text>
          <View style={s.tileBottom}>
            <Text style={[s.value, stat.alert && s.valueAlert]} numberOfLines={1}>
              {stat.value}
              {stat.unit ? <Text style={s.unit}> {stat.unit}</Text> : null}
            </Text>
            {stat.context ? (
              <Text style={[s.context, stat.delta && s.delta]} numberOfLines={1}>
                {stat.context}
              </Text>
            ) : null}
          </View>
        </View>
      ))}
    </View>
  );
}

const statStyles = (t: Tokens) =>
  StyleSheet.create({
    frame: { flexDirection: 'row', overflow: 'hidden' },
    tile: {
      flex: 1,
      // Two lines of label plus the value block. Fixed so every tile in the
      // row is the same height and the numbers share a baseline.
      justifyContent: 'space-between',
      minHeight: 86,
      paddingHorizontal: 13,
      paddingVertical: 14,
    },
    tileBottom: { gap: 2 },
    tileDivided: { borderLeftColor: t.color.line, borderLeftWidth: t.border.hair },
    label: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.wide,
      lineHeight: 13,
      textTransform: 'uppercase',
    },
    labelAlert: { color: t.color.warn },
    value: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: 25,
      fontVariant: ['tabular-nums'],
      letterSpacing: t.tracking.display,
    },
    valueAlert: { color: t.color.accentDeep },
    unit: { color: t.color.mut, fontFamily: fonts.bold, fontSize: t.font.sm },
    context: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.xs, marginTop: 2 },
    delta: { color: t.color.accentDeep, fontFamily: fonts.semibold },
  });

/** The thin progress bar: rounded track plus accent fill. */
export function BarProgress({
  fraction,
  height = 6,
  onInk = false,
  tone = 'accent',
}: {
  fraction: number;
  height?: number;
  /** On an ink/accent ground, tracks and fills invert. */
  onInk?: boolean;
  tone?: 'accent' | 'success' | 'warn';
}) {
  const t = useTokens();
  const clamped = Math.max(0, Math.min(1, fraction));
  const fill = onInk
    ? t.color.accentSoft
    : tone === 'success'
      ? t.color.success
      : tone === 'warn'
        ? t.color.warn
        : t.color.accent;
  return (
    <View
      style={{
        backgroundColor: onInk ? t.color.invTrack : t.color.track,
        borderRadius: 999,
        height,
        overflow: 'hidden',
      }}
      accessibilityRole="progressbar"
      accessibilityValue={{ min: 0, max: 100, now: Math.round(clamped * 100) }}
    >
      <View
        style={{
          backgroundColor: fill,
          borderRadius: 999,
          height,
          width: `${clamped * 100}%`,
        }}
      />
    </View>
  );
}

/**
 * A progress ring with the figure inside it.
 *
 * Used where progress is the *subject* rather than a detail — a goal tile, the
 * share of a programme completed. A bar of the same information takes a strip
 * of width and reads as a component; a ring reads as a score, which is what a
 * goal is. Everywhere progress is incidental, `BarProgress` is still right.
 */
export function Ring({
  fraction,
  size = 62,
  thickness = 6,
  label,
  tone = 'accent',
}: {
  fraction: number;
  size?: number;
  thickness?: number;
  /** Centre text. Defaults to the rounded percentage; '—' when unknown. */
  label?: string;
  tone?: 'accent' | 'success';
}) {
  const t = useTokens();
  const s = useStyles(ringStyles);
  const clamped = Math.max(0, Math.min(1, fraction));
  const radius = (size - thickness) / 2;
  const circumference = 2 * Math.PI * radius;
  const centre = size / 2;

  return (
    <View
      style={{ height: size, width: size }}
      accessibilityRole="progressbar"
      accessibilityValue={{ min: 0, max: 100, now: Math.round(clamped * 100) }}
    >
      <Svg width={size} height={size}>
        <Circle
          cx={centre}
          cy={centre}
          r={radius}
          stroke={t.color.track}
          strokeWidth={thickness}
          fill="none"
        />
        <Circle
          cx={centre}
          cy={centre}
          r={radius}
          stroke={tone === 'success' ? t.color.success : t.color.accent}
          strokeWidth={thickness}
          strokeLinecap="round"
          strokeDasharray={`${circumference * clamped} ${circumference}`}
          // Start at twelve o'clock rather than three: a dial that begins on
          // the right reads as already part-finished.
          transform={`rotate(-90 ${centre} ${centre})`}
          fill="none"
        />
      </Svg>
      <View style={[s.centre, { height: size, width: size }]}>
        <Text style={[s.value, { fontSize: Math.round(size * 0.27) }]} numberOfLines={1}>
          {label ?? `${Math.round(clamped * 100)}%`}
        </Text>
      </View>
    </View>
  );
}

const ringStyles = (t: Tokens) =>
  StyleSheet.create({
    centre: { alignItems: 'center', justifyContent: 'center', position: 'absolute' },
    value: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontVariant: ['tabular-nums'],
      letterSpacing: t.tracking.display,
    },
  });

/**
 * A soft accent wash for one sentence of consequence.
 *
 * No left bar: a 4px rule beside a rounded wash is two containment devices
 * arguing, and the wash already says "this is set apart".
 */
export function Callout({
  children,
  tone = 'accent',
}: {
  children: React.ReactNode;
  tone?: 'accent' | 'warn' | 'danger' | 'success';
}) {
  const s = useStyles(calloutStyles);
  return (
    <View style={[s.callout, s[tone]]}>
      <Text style={[s.text, s[`${tone}Text`]]}>{children}</Text>
    </View>
  );
}

/** Bold run inside a Callout / Muted sentence. */
export function Strong({ children }: { children: React.ReactNode }) {
  const t = useTokens();
  return <Text style={{ color: t.color.ink, fontFamily: fonts.bold }}>{children}</Text>;
}

const calloutStyles = (t: Tokens) =>
  StyleSheet.create({
    callout: {
      borderRadius: t.radius.md,
      paddingHorizontal: 14,
      paddingVertical: 12,
    },
    accent: { backgroundColor: t.color.accentHi },
    warn: { backgroundColor: t.color.warnHi },
    danger: { backgroundColor: t.color.dangerHi },
    success: { backgroundColor: t.color.successHi },
    text: { fontFamily: fonts.medium, fontSize: t.font.sm, lineHeight: 19 },
    accentText: { color: t.color.accentBadgeInk },
    warnText: { color: t.color.warnInk },
    dangerText: { color: t.color.dangerInk },
    successText: { color: t.color.successInk },
  });

export function ErrorBanner({ message }: { message: string }) {
  const s = useStyles(bannerStyles);
  return (
    <Animated.View
      entering={FadeInDown.duration(220)}
      style={s.banner}
      accessibilityLiveRegion="polite"
    >
      <Text style={s.text}>{message}</Text>
    </Animated.View>
  );
}

const bannerStyles = (t: Tokens) =>
  StyleSheet.create({
    banner: {
      backgroundColor: t.color.dangerHi,
      borderRadius: t.radius.md,
      padding: 14,
    },
    text: {
      color: t.color.dangerInk,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm,
      lineHeight: 19,
    },
  });

/**
 * Empty states: a glyph in a soft accent tile over a plain statement.
 *
 * The hint is an instruction, not an apology — an empty screen is the best
 * moment to say what to do next, and the worst moment to say "nothing here".
 */
export function EmptyState({
  glyph,
  title,
  hint,
  action,
}: {
  glyph: string;
  title: string;
  hint?: string;
  action?: React.ReactNode;
}) {
  const s = useStyles(emptyStyles);
  return (
    <View style={s.empty}>
      <View style={s.box}>
        <Text style={s.glyph}>{glyph}</Text>
      </View>
      <Text style={s.title}>{title}</Text>
      {hint ? <Text style={s.hint}>{hint}</Text> : null}
      {action ? <View style={s.action}>{action}</View> : null}
    </View>
  );
}

const emptyStyles = (t: Tokens) =>
  StyleSheet.create({
    empty: { alignItems: 'center', gap: t.space.md, paddingVertical: t.space.huge },
    box: {
      alignItems: 'center',
      backgroundColor: t.color.accentHi,
      borderRadius: t.radius.xl,
      height: 68,
      justifyContent: 'center',
      width: 68,
    },
    // Named even though these are geometric characters most families do not
    // carry: without a family the platform picks one, and "the platform picks"
    // is exactly the state the check exists to prevent. Per-glyph fallback
    // still happens, and now it happens from a known starting point.
    glyph: { color: t.color.accentDeep, fontFamily: fonts.regular, fontSize: 27 },
    title: { color: t.color.ink, fontFamily: fonts.display, fontSize: t.font.lg },
    hint: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
      maxWidth: 300,
      textAlign: 'center',
    },
    action: { paddingTop: t.space.sm },
  });

/**
 * A list row: title plus subtitle, an optional right element (a chevron when
 * pressable), and a hairline below unless it is the last one. The repeating
 * unit of nearly every screen.
 */
export function ListRow({
  title,
  subtitle,
  subtitleTone = 'muted',
  onPress,
  onLongPress,
  right,
  left,
  last = false,
}: {
  title: string;
  subtitle?: string;
  /** 'reason' renders the subtitle in accentDeep — suggestion reasons, alerts. */
  subtitleTone?: 'muted' | 'reason';
  onPress?: () => void;
  /**
   * Secondary, deliberately awkward action — archiving, closing, ending.
   * A long press is the right home for something irreversible sitting in a
   * list people otherwise tap to read.
   */
  onLongPress?: () => void;
  right?: React.ReactNode;
  left?: React.ReactNode;
  last?: boolean;
}) {
  const t = useTokens();
  const s = useStyles(rowStyles);

  const body = (
    <View style={[s.row, !last && s.rowLine]}>
      {left}
      <View style={s.rowBody}>
        <Text style={s.rowTitle} numberOfLines={1}>
          {title}
        </Text>
        {subtitle ? (
          <Text
            style={[s.rowSubtitle, subtitleTone === 'reason' && s.rowReason]}
            numberOfLines={2}
          >
            {subtitle}
          </Text>
        ) : null}
      </View>
      {right ??
        (onPress ? <Feather name="chevron-right" size={18} color={t.color.faint} /> : null)}
    </View>
  );

  // A row with only a long-press is still interactive — returning the bare body
  // would make it unreachable to a screen reader and to a finger.
  if (!onPress && !onLongPress) return body;
  return (
    <Touchable
      onPress={onPress}
      onLongPress={onLongPress}
      accessibilityLabel={subtitle ? `${title}. ${subtitle}` : title}
      accessibilityHint={onLongPress ? 'Long press for more options' : undefined}
    >
      {body}
    </Touchable>
  );
}

const rowStyles = (t: Tokens) =>
  StyleSheet.create({
    row: {
      alignItems: 'center',
      flexDirection: 'row',
      gap: t.space.md,
      paddingVertical: 14,
    },
    rowLine: { borderBottomColor: t.color.line, borderBottomWidth: StyleSheet.hairlineWidth },
    rowBody: { flex: 1, gap: 3 },
    rowTitle: { color: t.color.ink, fontFamily: fonts.semibold, fontSize: t.font.md },
    rowSubtitle: { color: t.color.mut, fontFamily: fonts.regular, fontSize: t.font.sm },
    rowReason: { color: t.color.accentDeep, fontFamily: fonts.medium },
  });

/**
 * Initials in a rounded tile.
 *
 * Washed accent rather than solid ink: a roster of twelve black squares is a
 * barcode, and these sit next to names that are the thing being read.
 */
export function InitialsSquare({ name, size = 38 }: { name: string; size?: number }) {
  const t = useTokens();
  const initials = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]?.toUpperCase() ?? '')
    .join('');
  return (
    <View
      style={{
        alignItems: 'center',
        backgroundColor: t.color.accentHi,
        borderRadius: t.radius.md,
        height: size,
        justifyContent: 'center',
        width: size,
      }}
    >
      <Text
        style={{
          color: t.color.accentBadgeInk,
          fontFamily: fonts.bold,
          fontSize: Math.round(size * 0.36),
          letterSpacing: t.tracking.tight,
        }}
      >
        {initials}
      </Text>
    </View>
  );
}

/**
 * A segmented control — the replacement for the ad-hoc chip rows that every
 * filtering screen used to grow its own version of.
 *
 * A recessed trough with a raised selected item, which is the shape people
 * already read as "pick exactly one of these". Wraps rather than scrolling,
 * because a horizontal scroller hides options and clips its own edges (see
 * the note in verify-design-consistency.mjs).
 */
export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
}: {
  options: { key: T; label: string; count?: number }[];
  value: T;
  onChange: (key: T) => void;
  /** Read aloud in place of the group; e.g. "Filter invoices". */
  label?: string;
}) {
  const s = useStyles(segmentedStyles);
  return (
    <View style={s.trough} accessibilityRole="tablist" accessibilityLabel={label}>
      {options.map((option) => {
        const on = option.key === value;
        return (
          <Pressable
            key={option.key}
            onPress={() => {
              if (!on) tap(Haptics.ImpactFeedbackStyle.Soft);
              onChange(option.key);
            }}
            accessibilityRole="tab"
            accessibilityState={{ selected: on }}
            style={[s.item, on && s.itemOn]}
          >
            <Text style={[s.text, on && s.textOn]} numberOfLines={1}>
              {option.label}
              {option.count != null ? `  ${option.count}` : ''}
            </Text>
          </Pressable>
        );
      })}
    </View>
  );
}

const segmentedStyles = (t: Tokens) =>
  StyleSheet.create({
    trough: {
      backgroundColor: t.color.sunken,
      borderRadius: t.radius.md,
      flexDirection: 'row',
      flexWrap: 'wrap',
      gap: 3,
      padding: 3,
    },
    item: {
      alignItems: 'center',
      borderRadius: t.radius.sm,
      flexGrow: 1,
      justifyContent: 'center',
      minHeight: 34,
      paddingHorizontal: 12,
    },
    itemOn: { backgroundColor: t.color.surface2, ...t.elevation(1) },
    text: {
      color: t.color.mut,
      fontFamily: fonts.semibold,
      fontSize: t.font.sm,
      fontVariant: ['tabular-nums'],
    },
    textOn: { color: t.color.ink, fontFamily: fonts.bold },
  });

/**
 * A round icon button — the only place in the app an icon stands alone.
 *
 * Always carries an accessibility label, because a glyph is not a name.
 */
export function IconButton({
  icon,
  onPress,
  label,
  tone = 'quiet',
  size = 42,
}: {
  icon: React.ComponentProps<typeof Feather>['name'];
  onPress: () => void;
  label: string;
  tone?: 'quiet' | 'accent';
  size?: number;
}) {
  const t = useTokens();
  return (
    <Touchable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={label}
      style={{
        alignItems: 'center',
        backgroundColor: tone === 'accent' ? t.color.accent : t.color.sunken,
        borderRadius: t.radius.md,
        height: size,
        justifyContent: 'center',
        width: size,
      }}
    >
      <Feather
        name={icon}
        size={Math.round(size * 0.45)}
        color={tone === 'accent' ? t.color.onAccent : t.color.ink}
      />
    </Touchable>
  );
}
