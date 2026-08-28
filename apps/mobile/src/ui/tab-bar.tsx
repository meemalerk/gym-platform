/**
 * The tab bar — a floating capsule that sits above the content rather than a
 * bar welded to the bottom edge.
 *
 * Why a capsule: the old bar was a solid slab under a 2px ink rule, which made
 * the bottom of every screen look like the bottom of a form. Lifting it leaves
 * a strip of the screen visible underneath, so the content reads as continuing
 * past it — which is true, because it scrolls. The active tab is a filled pill,
 * so "where am I" is answered by a shape and not only by a colour.
 *
 * Hand-rolled rather than the default one so it can speak the token language,
 * and because expo-router vendors its own bottom-tabs (see the TabBarProps
 * note below).
 */

import Feather from '@expo/vector-icons/Feather';
import * as Haptics from 'expo-haptics';
import { Tabs } from 'expo-router';
import { Platform, Pressable, StyleSheet, Text, View } from 'react-native';
import Animated, { useAnimatedStyle, useSharedValue, withSpring } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { TAB_MANIFEST, type TabDefinition } from '@/navigation/tabs';
import { fonts, type Tokens } from '@/ui/theme';
import { useStyles, useTokens } from '@/ui/theme-context';

/**
 * Derived from what `Tabs` actually passes, rather than imported from
 * `@react-navigation/bottom-tabs` — expo-router vendors its own copy, and two
 * structurally-different `BottomTabBarProps` types cannot be reconciled.
 */
type TabBarProps = Parameters<NonNullable<React.ComponentProps<typeof Tabs>['tabBar']>>[0];

/** Fixed so the bar's height is arithmetic rather than a measurement. */
const ITEM_HEIGHT = 52;
/** The gap between the capsule and the screen edge, on all four sides. */
const INSET = 12;

/** How much room the bar takes; pad scrollable content by this. */
export function useTabBarHeight(): number {
  const insets = useSafeAreaInsets();
  return ITEM_HEIGHT + INSET * 2 + Math.max(insets.bottom - INSET, 0);
}

/** See the note on `tap` in components.tsx — haptics must never block a tap. */
const tick = () => {
  if (Platform.OS === 'web') return;
  try {
    Haptics.selectionAsync().catch(() => {});
  } catch {
    // Device cannot vibrate. Carry on.
  }
};

function TabItem({
  tab,
  focused,
  onPress,
}: {
  tab: TabDefinition;
  focused: boolean;
  onPress: () => void;
}) {
  const t = useTokens();
  const s = useStyles(styleFactory);
  const scale = useSharedValue(1);
  const animated = useAnimatedStyle(() => ({ transform: [{ scale: scale.value }] }));

  return (
    <Pressable
      onPress={onPress}
      onPressIn={() => (scale.value = withSpring(0.92, t.motion.press))}
      onPressOut={() => (scale.value = withSpring(1, t.motion.bounce))}
      style={s.item}
      accessibilityRole="tab"
      accessibilityState={{ selected: focused }}
      accessibilityLabel={tab.a11yLabel}
      hitSlop={6}
    >
      <Animated.View style={[animated, s.itemInner, focused && s.itemInnerOn]}>
        <Feather
          name={tab.icon}
          size={18}
          color={focused ? t.color.onAccent : t.color.mut}
        />
        {/*
          The label appears only on the active tab. Five permanent 9pt words
          under five glyphs is a lot of noise for information you only need
          about the one you are not on — and the icons carry the rest, with
          the accessibility label doing the real naming either way.
        */}
        {focused ? (
          <Text style={s.label} numberOfLines={1}>
            {tab.label}
          </Text>
        ) : null}
      </Animated.View>
    </Pressable>
  );
}

export function GymTabBar({ state, navigation }: TabBarProps) {
  const insets = useSafeAreaInsets();
  const s = useStyles(styleFactory);

  return (
    <View
      style={[s.container, { paddingBottom: Math.max(insets.bottom, INSET) }]}
      accessibilityRole="tablist"
      pointerEvents="box-none"
    >
      <View style={s.capsule}>
        {state.routes.map((route, index) => {
          const tab = TAB_MANIFEST.find((entry) => entry.name === route.name);
          // A route with no manifest entry is a mistake, but crashing the whole
          // bar over it would strand the user with no navigation at all.
          if (!tab) return null;

          const focused = state.index === index;

          return (
            <TabItem
              key={route.key}
              tab={tab}
              focused={focused}
              onPress={() => {
                const event = navigation.emit({
                  type: 'tabPress',
                  target: route.key,
                  canPreventDefault: true,
                });
                if (focused || event.defaultPrevented) return;
                tick();
                navigation.navigate(route.name);
              }}
            />
          );
        })}
      </View>
    </View>
  );
}

const styleFactory = (t: Tokens) =>
  StyleSheet.create({
    container: {
      backgroundColor: 'transparent',
      bottom: 0,
      left: 0,
      paddingHorizontal: INSET,
      paddingTop: INSET,
      position: 'absolute',
      right: 0,
    },
    capsule: {
      backgroundColor: t.color.surface2,
      borderColor: t.color.line,
      borderRadius: t.radius.pill,
      borderWidth: t.border.hair,
      flexDirection: 'row',
      height: ITEM_HEIGHT,
      paddingHorizontal: 5,
      ...t.elevation(3),
    },
    item: { alignItems: 'center', flex: 1, justifyContent: 'center' },
    itemInner: {
      alignItems: 'center',
      borderRadius: t.radius.pill,
      flexDirection: 'row',
      gap: 6,
      justifyContent: 'center',
      minHeight: 38,
      paddingHorizontal: 12,
    },
    itemInnerOn: { backgroundColor: t.color.accent },
    label: {
      color: t.color.onAccent,
      fontFamily: fonts.bold,
      fontSize: t.font.sm,
      letterSpacing: t.tracking.tight,
    },
  });
