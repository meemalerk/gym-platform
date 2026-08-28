import { Stack, useRouter } from 'expo-router';
import { StyleSheet, Text, View } from 'react-native';

import { Appear, Button, Kicker, Screen } from '@/ui/components';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles } from '@/ui/theme-context';

/**
 * "How do you want to train?" — asked once, right after joining.
 *
 * Two things worth understanding about this screen.
 *
 * **Solo is not a mode.** There is no `is_solo` column and there should never
 * be one. A member training on their own is simply a member with no coach
 * relationship, which the model already expresses perfectly well — and which
 * means changing your mind later costs nothing and unlocks nothing. Adding a
 * flag would create a second source of truth for a question `coach_relationships`
 * already answers, and the first bug would be someone marked "solo" who has a
 * coach.
 *
 * So this screen writes **nothing**. It is a signpost: one branch goes to the
 * coach directory, the other goes to Today. That is the entire mechanism, and
 * it is why the copy can honestly promise you can switch whenever you like.
 *
 * **It is not a gate either.** Skipping it leaves you a perfectly ordinary
 * member. Someone who force-quits here and reopens the app lands on Today with
 * everything working — the same place the "on my own" branch would have put
 * them.
 */
export default function TrainingStyle() {
  const s = useStyles(styleFactory);
  const router = useRouter();

  return (
    <Screen scroll edges={['bottom']}>
      {/* No back arrow: there is nothing useful behind this — the gym has
          already been joined — and a back gesture into a resolved onboarding
          flow is how people end up looking at a stale "join" screen. */}
      <Stack.Screen options={{ title: '', headerBackVisible: false }} />

      <View style={s.intro}>
        <Appear>
          <Kicker tone="accent">You&apos;re a member</Kicker>
        </Appear>
        <Appear index={1}>
          <Text style={s.h1}>How do you{'\n'}want to train?</Text>
        </Appear>
        <Appear index={2}>
          <Text style={s.lede}>
            You can change this whenever you like — nothing here is locked in.
          </Text>
        </Appear>
      </View>

      <Appear index={3}>
        <View style={s.option}>
          <Text style={s.optionTitle}>With a trainer</Text>
          <Text style={s.optionBody}>
            Pick a coach and ask them to take you on. They write your programme, watch your
            logs and adjust as you go. They&apos;ll see your training history — so it takes
            both of you to agree.
          </Text>
          <Button
            label="Browse coaches"
            onPress={() => router.replace('/(app)/find-coach')}
          />
        </View>
      </Appear>

      <Appear index={4}>
        <View style={s.option}>
          <Text style={s.optionTitle}>On my own</Text>
          <Text style={s.optionBody}>
            Train solo. You get the gym&apos;s published programmes, the full exercise
            library, your own logging and progress — everything except a coach. Ask for one
            later from the People tab.
          </Text>
          <Button
            label="Start training"
            variant="secondary"
            onPress={() => router.replace('/(app)/(tabs)')}
          />
        </View>
      </Appear>
    </Screen>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    intro: { gap: 6 },
    h1: {
      color: t.color.ink,
      fontFamily: fonts.displayHeavy,
      fontSize: t.font.xxl,
      letterSpacing: t.tracking.display,
      lineHeight: 32,
      marginTop: 4,
    },
    lede: { color: t.color.mut2, fontFamily: fonts.regular, fontSize: t.font.sm + 0.5, lineHeight: 20 },
    option: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.lg,
      borderWidth: t.border.hair,
      gap: t.space.md,
      padding: t.space.lg,
      ...t.elevation(1),
    },
    optionTitle: {
      color: t.color.ink,
      fontFamily: fonts.display,
      fontSize: t.font.lg,
      letterSpacing: t.tracking.tight,
    },
    optionBody: {
      color: t.color.mut2,
      fontFamily: fonts.regular,
      fontSize: t.font.sm + 0.5,
      lineHeight: 20,
    },
  });
