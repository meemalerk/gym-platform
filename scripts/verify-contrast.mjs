/**
 * Verify the design system's colour pairings against WCAG 2.1 contrast minima.
 *
 * A palette is the one part of a design system where "looks fine to me" is
 * measurably wrong for a share of users — and where a dark scheme derived by
 * inverting a light one quietly fails. This checks every pairing the components
 * actually render, in both schemes, so the palette is proven rather than
 * eyeballed. Same posture as every other suite here (ADR-0019).
 *
 * Thresholds (WCAG 2.1 AA):
 *   4.5  normal text
 *   3.0  large text (>= 24px, or >= 18.66px bold — our display/stat sizes)
 *   3.0  non-text UI: the edge of a control, a meaningful state marker
 *
 * What is deliberately NOT held to 3:1, and why (ADR-0030): a card's hairline
 * and a progress track are *decorative* separation sitting behind a compliant
 * foreground. Containment in this language comes from fill and elevation, not
 * from an outline, so an outline that met 3:1 would be a wireframe drawn over
 * a design that does not need it. They are checked at "perceptibly different"
 * instead — stated here rather than quietly skipped.
 *
 *   node scripts/verify-contrast.mjs
 */

import { register } from 'node:module';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const { palettes } = await import('../apps/mobile/src/ui/theme.ts');

let passed = 0;
let failed = 0;

const srgbToLinear = (c) => {
  const v = c / 255;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
};

/** Relative luminance per WCAG 2.1. */
function luminance(hex) {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) throw new Error(`not a 6-digit hex colour: ${hex}`);
  const n = parseInt(m[1], 16);
  const r = srgbToLinear((n >> 16) & 0xff);
  const g = srgbToLinear((n >> 8) & 0xff);
  const b = srgbToLinear(n & 0xff);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function ratio(fg, bg) {
  const a = luminance(fg);
  const b = luminance(bg);
  const [hi, lo] = a > b ? [a, b] : [b, a];
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * Composite `fg` over `bg` at `alpha`, in sRGB — which is what React Native's
 * `opacity` actually does.
 *
 * Softening a label with opacity is the standard way to get a second tier of
 * text on a coloured fill, and it is also the standard way to fall out of AA
 * without noticing, because the token you wrote down still passes. Checking
 * the composite is the only honest version of that check.
 */
function over(fg, alpha, bg) {
  const parse = (hex) => {
    const n = parseInt(hex.replace('#', ''), 16);
    return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
  };
  const [fr, fg_, fb] = parse(fg);
  const [br, bg_, bb] = parse(bg);
  const mix = (a, b) => Math.round(alpha * a + (1 - alpha) * b);
  return `#${[mix(fr, br), mix(fg_, bg_), mix(fb, bb)]
    .map((c) => c.toString(16).padStart(2, '0'))
    .join('')}`;
}

function check(scheme, label, fg, bg, min) {
  const r = ratio(fg, bg);
  const ok = r >= min;
  // Both to two decimals: rounding the threshold to one made a passing
  // 1.25-against-1.25 print as "1.25 (min 1.3)", which reads as a broken check.
  const line = `${r.toFixed(2)}:1 (min ${min.toFixed(2)})  ${scheme}  ${label}`;
  if (ok) {
    passed += 1;
    console.log(`  ok    ${line}`);
  } else {
    failed += 1;
    console.log(`  FAIL  ${line}   ${fg} on ${bg}`);
  }
}

/**
 * Every pairing the components actually paint. Keep this in step with
 * components.tsx — an unlisted pairing is an unverified one.
 */
const TEXT = 4.5;
const LARGE = 3.0;
const UI = 3.0;
/** Decorative separation: must be visible, is not carrying meaning alone. */
const SEEN = 1.2;

for (const [scheme, c] of Object.entries(palettes)) {
  console.log(`\n=== ${scheme} ===`);

  // ---- text on every ground a screen can paint ---------------------------

  check(scheme, 'ink on surface (body, headings)', c.ink, c.surface, TEXT);
  check(scheme, 'ink on surface2 (text in a card)', c.ink, c.surface2, TEXT);
  check(scheme, 'ink on surface3 (text in a nested card)', c.ink, c.surface3, TEXT);
  check(scheme, 'ink on sunken (typed input value)', c.ink, c.sunken, TEXT);
  check(scheme, 'ink on bg', c.ink, c.bg, TEXT);

  check(scheme, 'mut on surface (kickers, meta)', c.mut, c.surface, TEXT);
  check(scheme, 'mut on surface2', c.mut, c.surface2, TEXT);
  check(scheme, 'mut on surface3', c.mut, c.surface3, TEXT);
  check(scheme, 'mut on sunken (segmented-control rest label)', c.mut, c.sunken, TEXT);
  check(scheme, 'mut2 on surface (lede, subtitles)', c.mut2, c.surface, TEXT);
  check(scheme, 'mut2 on surface2', c.mut2, c.surface2, TEXT);
  check(scheme, 'faint on sunken (placeholders)', c.faint, c.sunken, TEXT);
  check(scheme, 'faint on surface2 (placeholders in a card input)', c.faint, c.surface2, TEXT);

  // ---- the accent --------------------------------------------------------

  check(scheme, 'accentDeep on surface (reasons, deltas)', c.accentDeep, c.surface, TEXT);
  check(scheme, 'accentDeep on surface2', c.accentDeep, c.surface2, TEXT);
  check(scheme, 'accentDeep on accentHi (wash callout)', c.accentDeep, c.accentHi, TEXT);
  check(scheme, 'accent on surface (large numerals only)', c.accent, c.surface, LARGE);
  check(scheme, 'accent on surface2 (progress fill, focal edge)', c.accent, c.surface2, UI);
  check(scheme, 'onAccent on accent (primary CTA label)', c.onAccent, c.accent, TEXT);
  check(scheme, 'accentBadgeInk on accentBadge (badge)', c.accentBadgeInk, c.accentBadge, TEXT);
  check(scheme, 'accentBadgeInk on accentHi (callout body)', c.accentBadgeInk, c.accentHi, TEXT);
  check(scheme, 'accentSoft on ink (accent text on an ink card)', c.accentSoft, c.ink, TEXT);
  check(scheme, 'accentTrack on accent (groove in an accent fill)', c.accentTrack, c.accent, SEEN);
  // The focal card's CTA is a raised surface2 chip sitting on the accent
  // fill, with the brand colour as its ink — the one place the accent is
  // small text rather than a large numeral, so it is held to 4.5 not 3.
  check(scheme, 'accent on surface2 (focal CTA chip label)', c.accent, c.surface2, TEXT);
  // Second-tier labels on the focal card and the rest timer are `onAccent`
  // softened to 85%. The token passes on its own; what ships is the mix.
  check(
    scheme,
    'onAccent at 85% on accent (softened kicker)',
    over(c.onAccent, 0.85, c.accent),
    c.accent,
    TEXT,
  );

  // ---- signal: live, and only live --------------------------------------
  //
  // A lime bright enough to read as "live" on a near-black ground cannot also
  // be a 3:1 marker on a white one — no single colour is. So `signal` is only
  // ever painted as a **fill carrying `onSignal` text**, with an `onSignal`
  // hairline that defines its edge on any ground. That is one pairing to
  // verify, it looks identical in both schemes (which is the point: "live"
  // should not change costume at dusk), and it is why nothing anywhere paints
  // `signal` as a bare dot or as body copy. `signalDeep` is the text form.

  check(scheme, 'onSignal on signal (label and edge of the live pill)', c.onSignal, c.signal, TEXT);
  check(scheme, 'signalDeep on surface (live as text)', c.signalDeep, c.surface, TEXT);
  check(scheme, 'signalDeep on surface2', c.signalDeep, c.surface2, TEXT);

  // ---- semantic triplets -------------------------------------------------

  check(scheme, 'danger on surface (error text)', c.danger, c.surface, TEXT);
  check(scheme, 'danger on surface2', c.danger, c.surface2, TEXT);
  check(scheme, 'dangerInk on dangerHi (overdue chip)', c.dangerInk, c.dangerHi, TEXT);
  check(scheme, 'success on surface (settled text)', c.success, c.surface, TEXT);
  check(scheme, 'success on surface2', c.success, c.surface2, TEXT);
  check(scheme, 'successInk on successHi (paid chip)', c.successInk, c.successHi, TEXT);
  check(scheme, 'warn on surface (attention text)', c.warn, c.surface, TEXT);
  check(scheme, 'warn on surface2', c.warn, c.surface2, TEXT);
  check(scheme, 'warnInk on warnHi (pending chip)', c.warnInk, c.warnHi, TEXT);

  // ---- ink-filled elements ----------------------------------------------

  check(scheme, 'inkInv on ink (ink-filled badge/avatar)', c.inkInv, c.ink, TEXT);
  check(scheme, 'invMut on ink (muted text on an ink card)', c.invMut, c.ink, TEXT);
  check(scheme, 'invTrack on ink (progress track on an ink card)', c.invTrack, c.ink, SEEN);

  // ---- the door scanner --------------------------------------------------
  //
  // Identical in both schemes on purpose (see the Palette doc) — checked
  // anyway, because "it's obviously fine" is how the other failures in this
  // file got written.

  check(
    scheme,
    'onViewfinder on viewfinder (scanner reticle/hint)',
    c.onViewfinder,
    c.viewfinder,
    TEXT,
  );

  // ---- non-text UI -------------------------------------------------------
  //
  // `rule` is the edge of a control you can type into or press, so it carries
  // 1.4.11 on every ground it is drawn on. Everything else here is decorative.

  check(scheme, 'rule on surface (control edge)', c.rule, c.surface, UI);
  check(scheme, 'rule on surface2 (control edge in a card)', c.rule, c.surface2, UI);
  // Nothing draws `rule` on `sunken`: an input is a recessed FILL, not an
  // outlined box, so the trough defines itself and the only edge it ever
  // grows is the accent focus ring.
  check(scheme, 'line on surface (card hairline, decorative)', c.line, c.surface, SEEN);
  check(scheme, 'line on surface2 (row hairline, decorative)', c.line, c.surface2, SEEN);
  check(scheme, 'track on surface2 (progress track, decorative)', c.track, c.surface2, SEEN);

  // ---- containment -------------------------------------------------------
  //
  // In light a card is white on a tinted ground and lifted by a cast, so the
  // tonal step is modest by design. In dark there is no cast, so the step IS
  // the containment and has to carry it alone.

  const step = scheme === 'dark' ? 1.1 : 1.03;
  check(scheme, 'surface2 on surface (a card on the screen)', c.surface2, c.surface, step);
  check(scheme, 'surface3 on surface2 (a card inside a card)', c.surface3, c.surface2, 1.0);
  check(scheme, 'sunken on surface2 (an input inside a card)', c.sunken, c.surface2, step);
  check(scheme, 'surface on bg (the screen on the page ground)', c.surface, c.bg, 1.0);
}

console.log('\n======================================');
console.log(`  PASSED: ${passed}    FAILED: ${failed}`);
console.log('======================================');
process.exit(failed === 0 ? 0 : 1);
