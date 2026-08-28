/**
 * The header every tab screen wears.
 *
 * A gym kicker with the day on the right, then the screen title at display
 * weight, then an optional line of context. The kicker used to become tappable
 * when an account belonged to more than one gym; this is a single-gym
 * deployment (ADR-0023), so it is a plain label — a chevron on a control that
 * does nothing is a lie, and there is nothing left to switch to.
 *
 * The title is Bricolage at a size that scales down on small phones, because
 * "Your floor" wrapping to two lines on an SE is the difference between a
 * header and an accident.
 */

import Feather from '@expo/vector-icons/Feather';
import { Pressable, StyleSheet, Text, useWindowDimensions, View } from 'react-native';

import { capacityLabel, useActiveMembership } from '@/session/store';
import { Badge } from '@/ui/components';
import { useTokens } from '@/ui/theme-context';
import { displayFont, displayLineHeight, fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

export function GymHeader({
  eyebrow,
  title,
  subtitle,
  meta,
  action,
  onBack,
}: {
  /** A short line between kicker and title. Rarely used. */
  eyebrow?: string;
  title: string;
  subtitle?: string;
  /** Right-aligned kicker text, e.g. "Sun 20 Jul". */
  meta?: string;
  /** Optional trailing control next to the title. */
  action?: React.ReactNode;
  /**
   * Provided only where this header REPLACES the native one.
   *
   * A pushed screen that hides the native header hides its back arrow with it,
   * and this header had none of its own — so Programmes and the pushed Library
   * were both dead ends you could only leave by force-quitting. Passing this
   * puts the chevron back.
   *
   * Tab screens leave it undefined: there is nothing behind a tab, and a
   * chevron that goes nowhere is the lie the gym kicker above already avoids.
   */
  onBack?: () => void;
}) {
  const s = useStyles(styleFactory);
  const t = useTokens();
  const active = useActiveMembership();
  const { width } = useWindowDimensions();

  const size = displayFont(width);

  return (
    <View style={s.wrap}>
      <View style={s.kickerRow}>
        {active ? (
          <View style={s.gym} accessibilityLabel={`Acting in ${active.gymName}`}>
            <View style={s.mark} />
            <Text style={s.gymText} numberOfLines={1}>
              {active.gymName}
            </Text>
          </View>
        ) : (
          <View />
        )}
        {meta ? <Text style={s.meta}>{meta}</Text> : null}
      </View>

      {eyebrow ? <Text style={s.eyebrow}>{eyebrow}</Text> : null}

      <View style={s.titleRow}>
        {onBack ? (
          <Pressable
            onPress={onBack}
            accessibilityRole="button"
            accessibilityLabel="Go back"
            // Generous, because it is the only way off these screens: a 44pt
            // target, not a 16pt glyph.
            hitSlop={12}
            style={s.back}
          >
            <Feather name="chevron-left" size={26} color={t.color.accent} />
          </Pressable>
        ) : null}
        <Text
          style={[s.title, { fontSize: size, lineHeight: displayLineHeight(size) }]}
          numberOfLines={2}
        >
          {title}
        </Text>
        {action}
      </View>

      {subtitle ? <Text style={s.subtitle}>{subtitle}</Text> : null}
    </View>
  );
}

/** The capacities held in the active gym, as badges. */
export function CapacityBadges() {
  const s = useStyles(styleFactory);
  const active = useActiveMembership();
  if (!active) return null;

  return (
    <View style={s.badges}>
      {active.capacities.map((c) => (
        <Badge key={c} tone="accent">
          {capacityLabel(c)}
        </Badge>
      ))}
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    wrap: { gap: t.space.sm, paddingBottom: t.space.xs },
    kickerRow: {
      alignItems: 'center',
      flexDirection: 'row',
      gap: t.space.md,
      justifyContent: 'space-between',
      minHeight: 20,
    },
    gym: {
      alignItems: 'center',
      flexDirection: 'row',
      flexShrink: 1,
      gap: 7,
    },
    // A small accent dot standing in for the gym's mark. Not an icon: it is a
    // brand chip, and it is the only decoration in the header.
    mark: {
      backgroundColor: t.color.accent,
      borderRadius: 999,
      height: 9,
      width: 9,
    },
    gymText: {
      color: t.color.mut,
      flexShrink: 1,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    meta: {
      color: t.color.mut,
      fontFamily: fonts.bold,
      fontSize: t.font.xxs,
      letterSpacing: t.tracking.kicker,
      textTransform: 'uppercase',
    },
    eyebrow: { color: t.color.mut2, fontFamily: fonts.medium, fontSize: t.font.md },
    titleRow: { alignItems: 'center', flexDirection: 'row', gap: t.space.md },
    // Pulled left so the chevron sits on the gutter line, where a native back
    // arrow would be, rather than indented with the title.
    back: { marginLeft: -6, paddingVertical: 2 },
    title: {
      color: t.color.ink,
      flex: 1,
      fontFamily: fonts.displayHeavy,
      letterSpacing: t.tracking.display,
    },
    subtitle: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 19,
    },
    badges: { flexDirection: 'row', flexWrap: 'wrap', gap: t.space.sm },
  });
