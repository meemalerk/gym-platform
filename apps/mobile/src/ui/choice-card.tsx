import Feather from '@expo/vector-icons/Feather';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * A single, large, unambiguous choice: a title, one sentence of consequence,
 * and a chevron. Used where a fork in a flow deserves more than a list row —
 * "join with a code" versus "join this gym", say.
 *
 * The old version numbered its forks 01 / 02 / 03. Numbering says "do these in
 * order", and these are alternatives, so the numbers were decorating a claim
 * that was not true. They are gone; `selected` is the only state a choice has.
 */
export function ChoiceCard({
  title,
  description,
  onPress,
  selected,
}: {
  title: string;
  description: string;
  onPress: () => void;
  selected?: boolean;
}) {
  const t = useTokens();
  const s = useStyles(styleFactory);
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ selected: Boolean(selected) }}
      accessibilityLabel={`${title}. ${description}`}
      onPress={onPress}
      style={({ pressed }) => [s.card, selected && s.cardSelected, pressed && s.cardPressed]}
    >
      <View style={s.body}>
        <Text style={s.title}>{title}</Text>
        <Text style={s.description}>{description}</Text>
      </View>
      <Feather
        name="chevron-right"
        size={20}
        color={selected ? t.color.accentDeep : t.color.faint}
      />
    </Pressable>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    card: {
      alignItems: 'center',
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.lg,
      borderWidth: t.border.hair,
      flexDirection: 'row',
      gap: t.space.md,
      paddingHorizontal: t.space.lg,
      paddingVertical: 18,
      ...t.elevation(1),
    },
    cardSelected: { backgroundColor: t.color.accentHi, borderColor: t.color.accent },
    cardPressed: { backgroundColor: t.color.accentHi },
    body: { flex: 1, gap: 4 },
    title: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.lg,
      letterSpacing: t.tracking.tight,
    },
    description: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 19,
    },
  });
