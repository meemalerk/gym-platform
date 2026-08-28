/**
 * Generate the console's CSS custom properties from the app's design tokens.
 *
 * ADR-0020's palette is verified against WCAG AA by `verify-contrast.mjs`, and
 * that verification reads `apps/mobile/src/ui/theme.ts`. A second client with a
 * hand-copied palette would be a second palette: unverified, and drifting from
 * the first the moment anyone tunes a value. Several of those values are what
 * they are *because* the obvious choice failed a contrast check, and losing
 * that history to a copy-paste is exactly how a design system rots.
 *
 * So the CSS is generated, and the generated file is checked in with a header
 * saying not to edit it. One source of truth, one contrast suite, two clients.
 *
 *   node scripts/generate-console-tokens.mjs
 */

import { writeFileSync } from 'node:fs';
import { register } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

register('./lib/ts-alias-loader.mjs', import.meta.url);

const { palettes, webFonts } = await import('../apps/mobile/src/ui/theme.ts');

const ROOT = dirname(fileURLToPath(new URL('..', import.meta.url + '/')));
const OUT = join(fileURLToPath(new URL('..', import.meta.url)), 'apps', 'console', 'src', 'tokens.css');

/** `accentDeep` -> `--accent-deep`. */
const kebab = (name) => `--${name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;

const block = (palette, indent = '  ') =>
  Object.entries(palette)
    .map(([name, value]) => `${indent}${kebab(name)}: ${value};`)
    .join('\n');

const css = `/*
 * GENERATED — do not edit.
 *
 * Source: apps/mobile/src/ui/theme.ts
 * Regenerate: node scripts/generate-console-tokens.mjs
 *
 * The palette lives in one place because it is verified in one place:
 * scripts/verify-contrast.mjs checks every pair against WCAG 2.1 AA, and a
 * hand-copied second palette would be an unverified one. Change the values in
 * theme.ts, re-run the contrast check, then regenerate this.
 */

:root {
${block(palettes.light)}

  /* Structure — ADR-0030's Signal language. Containment is fill + elevation;
     a border is for something you can type into or press. */
  --radius-xs: 6px;
  --radius-sm: 10px;
  --radius-md: 14px;
  --radius-lg: 18px;
  --radius-xl: 24px;
  --radius-pill: 999px;
  --border-ink: 1.5px;
  --border-hair: 1px;

  /* 4pt rhythm, matching the app's space scale. */
  --space-xs: 4px;
  --space-sm: 8px;
  --space-md: 12px;
  --space-lg: 16px;
  --space-gutter: 20px;
  --space-xl: 24px;
  --space-xxl: 32px;
  --space-huge: 48px;

  --font-body: ${webFonts.body}, -apple-system, BlinkMacSystemFont,
    "Segoe UI", Roboto, sans-serif;
  --font-display: ${webFonts.display}, ${webFonts.body}, -apple-system,
    BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  --tracking-kicker: 0.8px;
  --tracking-display: -0.9px;

  /* Elevation. Light lifts a white card off a tinted ground; dark cannot cast
     a shadow on near-black, so the block below flattens these to none and the
     surface steps do the containing instead. */
  --lift-1: 0 1px 3px rgb(25 25 38 / 6%);
  --lift-2: 0 4px 12px rgb(25 25 38 / 8%);
  --lift-3: 0 10px 26px rgb(25 25 38 / 12%);
}

/*
 * The scheme follows the OS, like the app. There is no toggle: a console used
 * beside the phone app should not be able to disagree with it about whether it
 * is night.
 */
@media (prefers-color-scheme: dark) {
  :root {
${block(palettes.dark, '    ')}

    /* No cast shadow on near-black — it is invisible and only muddies edges.
       Containment falls to the surface steps, exactly as it does in the app
       (see makeElevation in theme.ts). */
    --lift-1: none;
    --lift-2: none;
    --lift-3: none;
  }
}
`;

writeFileSync(OUT, css, 'utf8');
console.log(`wrote ${OUT}`);
console.log(
  `  ${Object.keys(palettes.light).length} tokens x 2 schemes, from apps/mobile/src/ui/theme.ts`,
);
