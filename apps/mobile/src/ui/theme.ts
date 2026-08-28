/**
 * Design tokens — the **Signal** language.
 *
 * Supersedes the "Modernist" reference (`gym-app/Gym App Views.dc.html`) and
 * the palette ADR-0020 shipped with it. See ADR-0030 for why. In one sentence:
 *
 *   a violet-indigo brand, one electric lime that only ever means *live*,
 *   softly rounded surfaces that stack by elevation rather than by outline,
 *   Bricolage Grotesque over Plus Jakarta Sans.
 *
 * What changed, and what it means at a call site:
 *
 *   - **Corners are round.** `radius` is a real scale now, not a row of zeros
 *     kept for compatibility. Pick from it; never type a number.
 *   - **`rule` is a control edge, `line` is a hairline.** The old language drew
 *     a 2px near-black box around everything, which is a wireframe. Containment
 *     now comes from fill and elevation; a border is for things you can *type
 *     into or press*, where WCAG 1.4.11 asks for 3:1.
 *   - **Semantic colours are triplets** — `danger`/`dangerHi`/`dangerInk`, and
 *     the same for success and warn — so a status chip is a token lookup rather
 *     than a per-screen invention. The old palette had one `danger` and every
 *     surface improvised a ground for it.
 *   - **`signal` is not a second accent.** It is reserved for *happening right
 *     now*: a live session, a running rest timer, someone on the floor. If it
 *     starts appearing on ordinary buttons, the language is broken.
 *
 * This module stays React-free on purpose: pure functions and constants, so the
 * node-run verify scripts can import it without a renderer. The React context
 * lives in `theme-context.tsx`.
 */

export type Scheme = 'light' | 'dark';

export type Palette = {
  /** Page ground — sits behind screens (rarely visible on phone). */
  bg: string;
  /** The screen surface. Cards sit *on* this. */
  surface: string;
  /** A raised container: cards, sheets, list groups. */
  surface2: string;
  /** One step further — a card on a card, or the focal container. */
  surface3: string;
  /** Recessed ground: inputs, wells, segmented-control troughs. */
  sunken: string;

  /** The ink: body text, and ink-*filled* elements (avatars, hard badges). */
  ink: string;
  /** Text/icon on an ink-filled element. */
  inkInv: string;
  /** Muted text — metadata, subtitles. */
  mut: string;
  /** Stronger muted — body copy, ledes. */
  mut2: string;
  /** Faintest legible — placeholders. Still AA against `sunken`. */
  faint: string;
  /** Muted text on an ink-filled element. */
  invMut: string;
  /** Progress track on an ink-filled element. */
  invTrack: string;

  /**
   * A **control edge**: input outlines, secondary buttons, segmented controls.
   * Clears 3:1 against every ground it is drawn on, because it is what tells
   * you where a control begins (WCAG 1.4.11).
   */
  rule: string;
  /** Hairline between rows and around cards. Decorative separation. */
  line: string;
  /** Progress-bar / chart-marker track. */
  track: string;

  /** Brand fill and marks. */
  accent: string;
  /** Label on an accent fill. Tuned per scheme — white fails on a light accent. */
  onAccent: string;
  /** Pressed state of an accent fill. */
  accentPress: string;
  /** Accent as text on an ink-filled element. */
  accentSoft: string;
  /** Barely-there accent wash — selected rows, callout grounds. */
  accentHi: string;
  /**
   * A groove recessed INTO an accent fill.
   * Not accentPress: that means 'pressed', and dark makes pressed BRIGHTER,
   * which would render the empty part of a progress bar as the loud part.
   */
  accentTrack: string;
  /** Filled badge ground. */
  accentBadge: string;
  /** Text on `accentBadge`; also callout body text. */
  accentBadgeInk: string;
  /** Accent as text on the screen — reasons, deltas, inline emphasis. */
  accentDeep: string;

  /**
   * **Live.** The one colour that is never decorative: a session in progress,
   * a running timer, somebody on the floor right now. Reserved, so that when
   * it appears the eye has already learned what it means.
   */
  signal: string;
  /** Text on a signal fill. */
  onSignal: string;
  /** Signal as text on a screen ground — darkened in light, or it is illegible. */
  signalDeep: string;

  /** Failure, arrears, destructive. Triplet: fill / wash / text-on-wash. */
  danger: string;
  dangerHi: string;
  dangerInk: string;
  /** Settled, approved, healthy. */
  success: string;
  successHi: string;
  successInk: string;
  /** Waiting on somebody, nearly late, needs review. */
  warn: string;
  warnHi: string;
  warnInk: string;

  /** Cast colour for elevation. Never painted directly. */
  shadow: string;
  /** Scrim behind a sheet or dialog. Never painted directly. */
  overlay: string;

  /**
   * Ground behind a live camera feed, and the marks drawn over one.
   *
   * The only pair in the palette that is deliberately IDENTICAL in both
   * schemes, and it is here rather than hard-coded so that the reason is
   * written down: a viewfinder is not a surface the app paints, it is video
   * the sensor paints. A reticle that followed the app's scheme would render
   * near-black lines over a dim gym doorway in light mode and vanish. So it
   * is always white-on-black, chosen once, named, and contrast-checked like
   * everything else.
   */
  viewfinder: string;
  onViewfinder: string;
};

/**
 * Both schemes are verified by `scripts/verify-contrast.mjs` against WCAG 2.1
 * AA. Do not hand-tune a value here without re-running it — several of these
 * numbers are what they are *because* the obvious choice failed.
 *
 * The two schemes are not mirror images, and the asymmetry is deliberate:
 *
 *   light — a card is **white, lifted off a tinted ground** by a soft cast and
 *           a hairline. Light falls from above, so the ground is the darker
 *           thing and the content floats on it.
 *   dark  — a cast shadow is invisible on near-black, so containment is
 *           **tone**: surface → surface2 → surface3, each step perceptible on
 *           its own, with the hairline present only to stop edges smearing.
 *
 * Neutrals are not grey. Every one carries a few degrees of the accent's hue,
 * which is most of the difference between a palette and a set of colours.
 */
export const palettes: Record<Scheme, Palette> = {
  light: {
    bg: '#e4e4ee',
    surface: '#f1f1f7',
    surface2: '#ffffff',
    surface3: '#ffffff',
    sunken: '#e9e9f2',

    ink: '#16161f',
    inkInv: '#ffffff',
    mut: '#5f6070',
    mut2: '#414252',
    faint: '#65667a',
    invMut: '#a9a9bc',
    invTrack: '#4a4b5e',

    rule: '#82839a',
    line: '#dadae8',
    track: '#d5d5e4',

    accent: '#5127d8',
    onAccent: '#ffffff',
    accentPress: '#3f1cb2',
    accentSoft: '#b3a0ff',
    accentHi: '#efecff',
    accentTrack: '#3f1cb2',
    accentBadge: '#e3dcff',
    accentBadgeInk: '#3a1a9e',
    accentDeep: '#4a24c8',

    signal: '#c9f24a',
    onSignal: '#1a2205',
    signalDeep: '#4a6000',

    danger: '#c01029',
    dangerHi: '#fde9ec',
    dangerInk: '#9c0c20',
    success: '#0c7350',
    successHi: '#e0f6ee',
    successInk: '#0a5c40',
    warn: '#9a5300',
    warnHi: '#fcefd9',
    warnInk: '#7d4300',

    shadow: '#191926',
    overlay: '#16161f',

    viewfinder: '#000000',
    onViewfinder: '#ffffff',
  },
  dark: {
    bg: '#07070b',
    surface: '#0d0d13',
    surface2: '#1a1a24',
    surface3: '#242531',
    sunken: '#0b0b11',

    ink: '#f0f0f6',
    inkInv: '#0d0d13',
    mut: '#9596a8',
    mut2: '#c0c1d0',
    faint: '#84859a',
    invMut: '#585a6c',
    invTrack: '#b1b2c2',

    rule: '#6a6c82',
    line: '#2c2d3a',
    track: '#31323f',

    accent: '#a48cff',
    onAccent: '#120c26',
    accentPress: '#bcaaff',
    accentSoft: '#5a2fd6',
    accentHi: '#1e193a',
    accentTrack: '#6a4fd6',
    accentBadge: '#2a2352',
    accentBadgeInk: '#c8b8ff',
    accentDeep: '#b09cff',

    signal: '#c9f24a',
    onSignal: '#1a2205',
    signalDeep: '#bde63e',

    danger: '#ff8f97',
    dangerHi: '#3a1218',
    dangerInk: '#ffb4ba',
    success: '#5ed3a0',
    successHi: '#0e2b21',
    successInk: '#7fe0b5',
    warn: '#f2b862',
    warnHi: '#34230d',
    warnInk: '#f7d094',

    shadow: '#000000',
    overlay: '#000000',

    viewfinder: '#000000',
    onViewfinder: '#ffffff',
  },
};

/**
 * Two families, each doing one job.
 *
 * **Bricolage Grotesque** for display: screen titles, stat values, the weight
 * on the bar. Its slightly compressed, high-energy skeleton gives a big numeral
 * personality without a decorative face's legibility cost — which matters here,
 * because the largest thing on almost every screen is a number somebody is
 * reading mid-set.
 *
 * **Plus Jakarta Sans** for everything else: UI, body, labels. Humanist, open
 * apertures, real tabular figures for the ledger.
 *
 * One family name per weight — React Native does not synthesize weights for
 * custom fonts, so `fontWeight` alone silently falls back.
 */
export const fonts = {
  regular: 'PlusJakartaSans_400Regular',
  medium: 'PlusJakartaSans_500Medium',
  semibold: 'PlusJakartaSans_600SemiBold',
  bold: 'PlusJakartaSans_700Bold',
  extrabold: 'PlusJakartaSans_800ExtraBold',

  /** Display face. Titles, stat values, timers, money. */
  display: 'BricolageGrotesque_700Bold',
  displayHeavy: 'BricolageGrotesque_800ExtraBold',
  displayMedium: 'BricolageGrotesque_600SemiBold',
} as const;

/**
 * CSS family names for the console, which loads the same two faces as a
 * webfont rather than a bundled asset. Exported here so the generator has one
 * source for the type as well as for the colour — see
 * `scripts/generate-console-tokens.mjs`.
 */
export const webFonts = {
  body: "'Plus Jakarta Sans'",
  display: "'Bricolage Grotesque'",
} as const;

const shared = {
  /** 4pt rhythm; `gutter` is the screen's horizontal inset. */
  space: {
    xs: 4,
    sm: 8,
    md: 12,
    lg: 16,
    gutter: 20,
    xl: 24,
    xxl: 32,
    huge: 48,
  } as const,

  /**
   * The corner scale. `sm` is for something inside something else (a chip in a
   * card); `lg` is the default container; `xl` is a sheet or a hero. `pill` is
   * only for genuinely capsule-shaped things — a filter chip, a live dot, the
   * tab bar.
   */
  radius: { xs: 6, sm: 10, md: 14, lg: 18, xl: 24, pill: 999 } as const,

  /** Control edges are 1.5px; hairlines are 1px. Nothing is 2px any more. */
  border: { ink: 1.5, hair: 1 } as const,

  font: {
    xxs: 10,
    xs: 11.5,
    sm: 13,
    md: 15,
    lg: 17,
    xl: 20,
    xxl: 26,
    display: 34,
  } as const,

  tracking: {
    display: -0.9,
    tight: -0.3,
    normal: 0,
    wide: 0.4,
    kicker: 0.8,
  } as const,

  motion: {
    press: { damping: 18, stiffness: 320, mass: 0.5 },
    enter: { damping: 20, stiffness: 180, mass: 0.7 },
    /** For anything that should feel springy rather than merely fast. */
    bounce: { damping: 12, stiffness: 260, mass: 0.6 },
  } as const,

  fonts,
};

export type ElevationStyle = {
  shadowColor: string;
  shadowOffset: { width: number; height: number };
  shadowOpacity: number;
  shadowRadius: number;
  elevation: number;
};

export type Tokens = typeof shared & {
  scheme: Scheme;
  color: Palette;
  /** Elevation for this scheme — see `makeElevation`. */
  elevation: (level: 0 | 1 | 2 | 3) => ElevationStyle;
};

/**
 * Elevation, scheme-aware.
 *
 * In light, a card is lifted by a soft, low, wide cast — the thing that makes a
 * white card on a tinted ground read as *above* it rather than as a hole cut in
 * it. In dark, a shadow against near-black is invisible, so the numbers collapse
 * to nothing and containment falls to the surface steps instead. One asymmetry,
 * encoded once, rather than a per-screen judgement call.
 */
function makeElevation(scheme: Scheme, color: Palette) {
  const dark = scheme === 'dark';
  const flat: ElevationStyle = {
    shadowColor: color.shadow,
    shadowOffset: { width: 0, height: 0 },
    shadowOpacity: 0,
    shadowRadius: 0,
    elevation: 0,
  };
  return (level: 0 | 1 | 2 | 3): ElevationStyle => {
    if (level === 0 || dark) return flat;
    const spec = {
      1: { height: 1, opacity: 0.06, radius: 3, android: 1 },
      2: { height: 4, opacity: 0.08, radius: 12, android: 3 },
      3: { height: 10, opacity: 0.12, radius: 26, android: 8 },
    }[level];
    return {
      shadowColor: color.shadow,
      shadowOffset: { width: 0, height: spec.height },
      shadowOpacity: spec.opacity,
      shadowRadius: spec.radius,
      elevation: spec.android,
    };
  };
}

export function makeTokens(scheme: Scheme): Tokens {
  const color = palettes[scheme];
  return { ...shared, scheme, color, elevation: makeElevation(scheme, color) };
}

/*
 * There is deliberately NO static palette export here.
 *
 * One existed while screens were being converted — the dark palette under the
 * old colour names — and it worked, but it also pinned the app to dark: a
 * module-level constant cannot answer "which scheme is this?", so any file
 * still importing it would have rendered dark inside a light app. Everything
 * reads tokens through `useTokens()` / `useStyles()`, which is what makes the
 * light scheme reachable at all.
 */

/**
 * Display type, sized to the screen — small phones must not truncate names.
 * Pure so `scripts/verify-nav.mjs` can pin the bounds without a device.
 */
export function displayFont(width: number): number {
  if (width < 340) return 26;
  if (width < 380) return 29;
  if (width < 420) return 32;
  return shared.font.display;
}

/** Line height for display type. Bricolage sits tight: 1.05–1.12. */
export const displayLineHeight = (size: number): number => Math.round(size * 1.08);
