import { Link, Stack } from 'expo-router';
import { StyleSheet, Text, View } from 'react-native';

import { Button, Screen } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

/**
 * A route that does not exist.
 *
 * Without this file expo-router renders its own development "Unmatched Route"
 * screen — the one that lists raw route names like `(tabs)` as tappable rows.
 * That is a debugging aid, not a product screen, and shipping it meant a
 * mistyped push showed a member the app's internal file layout and a button
 * that did nothing.
 *
 * It should be unreachable. It is here so that when it is not, what a person
 * sees is a sentence and a way back.
 */
export default function NotFound() {
  const s = useStyles(styleFactory);
  return (
    <Screen>
      <Stack.Screen options={{ title: 'Not found' }} />
      <View style={s.body}>
        <Text style={s.title}>That screen has moved</Text>
        <Text style={s.lede}>
          The link you followed does not go anywhere any more. Nothing is wrong with your
          account.
        </Text>
        <Link href="/(app)/(tabs)" asChild>
          <Button label="Back to Today" onPress={() => {}} />
        </Link>
      </View>
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    body: { gap: t.space.lg, justifyContent: 'center', paddingTop: t.space.huge },
    title: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
    },
    lede: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
    },
  });
