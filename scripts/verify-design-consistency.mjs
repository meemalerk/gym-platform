/**
 * The design system, enforced instead of remembered.
 *
 * ADR-0030 (the Signal language, superseding ADR-0020) fixes a small number of
 * visual rules. They are easy to state, easy to agree with, and easy to break
 * six screens later when you are thinking about something else — which is
 * exactly what happened under the old language: three screens written before
 * the redesign still had rounded corners and a hardcoded white, and nothing
 * complained.
 *
 * So the rules are assertions. Note that one of them **inverted** when the
 * language changed: the old rule was "no radius anywhere", the new one is
 * "every radius comes from the scale". That is the point of writing them down
 * as code — the check changed in the same commit as the decision, instead of
 * quietly outliving it.
 *
 *   node scripts/verify-design-consistency.mjs
 */

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

// fileURLToPath, never `.pathname`: on Windows a file URL's pathname is
// `/C:/Users/…` with percent-escapes, which `join` then turns into the
// nonexistent `C:\C:\Users\Big%20PC\…`. This script simply never ran there.
const ROOT = fileURLToPath(new URL('..', import.meta.url));
const MOBILE = join(ROOT, 'apps', 'mobile');

let passed = 0;
let failed = 0;

function check(label, ok, detail) {
  if (ok) {
    passed += 1;
    console.log(`  ok    ${label}`);
  } else {
    failed += 1;
    console.log(`  FAIL  ${label}`);
    if (detail) console.log(String(detail).replace(/^/gm, '          '));
  }
}

/** Every .tsx under app/ and src/ — the screens and the shared UI. */
function sources(dir, acc = []) {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules' || name.startsWith('.')) continue;
    const full = join(dir, name);
    if (statSync(full).isDirectory()) sources(full, acc);
    else if (name.endsWith('.tsx') || name.endsWith('.ts')) acc.push(full);
  }
  return acc;
}

const files = [...sources(join(MOBILE, 'app')), ...sources(join(MOBILE, 'src'))];
const rel = (f) => relative(MOBILE, f).replaceAll('\\', '/');

/** Strip comments so a rule discussed in prose is not read as a violation. */
function code(text) {
  return text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/^\s*\/\/.*$/gm, '');
}

const loaded = files.map((f) => ({ file: f, raw: readFileSync(f, 'utf8') }));
const scanned = loaded.map((x) => ({ ...x, body: code(x.raw) }));

console.log('\n=== one visual language ===');

// ADR-0030: corners are round, and the radii come from `t.radius`. A typed
// number is how a design ends up with 8, 10 and 12 on three cards that sit
// next to each other — which reads as sloppiness long before anyone can say
// why. `999` is allowed inline only where a shape is a literal capsule and
// naming it would be worse than showing it.
{
  const offenders = [];
  for (const x of scanned) {
    for (const m of x.body.matchAll(/borderRadius(?:\w*)?\s*:\s*([^,\n}]+)/g)) {
      const value = m[1].trim();
      if (/t\.radius\./.test(value)) continue;
      if (/^999$/.test(value)) continue;
      // A radius derived from a measured size (a circle of `size / 2`) is
      // arithmetic, not a magic number.
      if (/\/\s*2\b/.test(value)) continue;
      offenders.push(`${rel(x.file)}: borderRadius: ${value}`);
    }
  }
  check('every radius comes from t.radius', offenders.length === 0, offenders.join('\n'));
}

// The old language drew a 2px near-black box around every container. The new
// one contains with fill and elevation, so 2 is never the right border width:
// a control edge is `t.border.ink` (1.5) and a hairline is `t.border.hair`.
{
  const offenders = [];
  for (const x of scanned) {
    for (const m of x.body.matchAll(/border\w*Width\s*:\s*([^,\n}]+)/g)) {
      const value = m[1].trim();
      if (/t\.border\./.test(value)) continue;
      if (/^[01]$/.test(value)) continue;
      // The platform's own hairline is thinner than 1px on a retina screen,
      // which is exactly what a decorative separator wants to be.
      if (/StyleSheet\.hairlineWidth/.test(value)) continue;
      offenders.push(`${rel(x.file)}: ${m[0].trim()}`);
    }
  }
  check('border widths come from t.border', offenders.length === 0, offenders.join('\n'));
}

// Elevation is scheme-aware (`makeElevation` flattens it in dark, where a cast
// shadow on near-black is invisible and only smears the edge). A hand-written
// shadow cannot know which scheme it is in.
{
  const offenders = scanned
    .filter((x) => /shadowOpacity\s*:/.test(x.body) && !rel(x.file).endsWith('ui/theme.ts'))
    .map((x) => rel(x.file));
  check(
    'elevation comes from t.elevation(), not a hand-written shadow',
    offenders.length === 0,
    offenders.join('\n'),
  );
}

// Colour comes from tokens, so both schemes stay in step. A literal is a
// colour that cannot follow the theme.
{
  const offenders = [];
  for (const x of scanned) {
    const hits = x.body.match(/['"]#[0-9a-fA-F]{3,8}['"]/g);
    // theme.ts IS the palette — literals are its whole job.
    if (hits && !rel(x.file).endsWith('ui/theme.ts')) {
      offenders.push(`${rel(x.file)}: ${[...new Set(hits)].join(', ')}`);
    }
  }
  check('no hardcoded colours outside the palette', offenders.length === 0, offenders.join('\n'));
}

// rgba() is the same problem wearing a different hat: it does not follow the
// scheme either, and it is how a "just this once" overlay becomes permanent.
{
  const offenders = scanned
    .filter((x) => /rgba?\(/.test(x.body) && !rel(x.file).endsWith('ui/theme.ts'))
    .map((x) => rel(x.file));
  check('no inline rgb()/rgba() outside the palette', offenders.length === 0, offenders.join('\n'));
}

console.log('\n=== one icon family ===');

{
  const families = new Set();
  for (const x of scanned) {
    for (const m of x.body.matchAll(/@expo\/vector-icons\/(\w+)/g)) families.add(m[1]);
  }
  check(
    `exactly one icon set in use (${[...families].join(', ') || 'none'})`,
    families.size <= 1,
    families.size > 1 ? `mixing: ${[...families].join(', ')}` : '',
  );
}

console.log('\n=== typography comes from the scale ===');

{
  // fontWeight alongside a loaded font family fights it: Archivo ships as
  // separate files per weight, so a numeric weight either does nothing or
  // triggers a fake-bold the design never asked for.
  const offenders = scanned
    .filter((x) => /\bfontWeight\s*:/.test(x.body))
    .map((x) => rel(x.file));
  check(
    'no fontWeight — weight is chosen by picking the font file',
    offenders.length === 0,
    offenders.join('\n'),
  );
}

{
  /*
    A text style with a size and a colour but no family renders in the
    device's system font. It looks *almost* right next to a neutral grotesque,
    which is how five of them survived a whole design system in
    program/[version].tsx, and eight more in invite.tsx and new-exercise.tsx,
    without anyone noticing. Against a characterful display face they are
    obvious — but only once shipped.

    Multi-line AND single-line objects, because the first version of this check
    only matched the multi-line form and `label: { color, fontSize }` on one
    line sailed straight past it.

    A style with a size and no colour is a size-only override applied on top of
    a base style (`labelSmall: { fontSize: … }`), which is correct and common,
    so the pair is what gets flagged rather than the size alone.
  */
  const MULTILINE = /^(\s+)([A-Za-z0-9_]+): \{\n([\s\S]*?)\n\1\},$/gm;
  const SINGLELINE = /^\s+([A-Za-z0-9_]+): \{ ([^{}\n]*) \},$/gm;
  const offenders = [];
  const flag = (file, key, inner) => {
    if (!/fontSize\s*:/.test(inner)) return;
    if (/fontFamily\s*:/.test(inner)) return;
    if (!/(^|[,{\s])color\s*:/.test(inner)) return;
    offenders.push(`${rel(file)}: ${key}`);
  };
  for (const x of scanned) {
    for (const m of x.body.matchAll(MULTILINE)) flag(x.file, m[2], m[3]);
    for (const m of x.body.matchAll(SINGLELINE)) flag(x.file, m[1], m[2]);
  }
  check(
    'every sized text style names its font file',
    offenders.length === 0,
    offenders.join('\n'),
  );
}

console.log('\n=== styles are theme-aware ===');

{
  // A StyleSheet.create built outside a styleFactory(t) cannot see the tokens,
  // so it silently ignores the light scheme.
  const offenders = [];
  for (const x of scanned) {
    if (!/StyleSheet\.create/.test(x.body)) continue;
    const hasFactory = /styleFactory|rowStyles|=>\s*\n?\s*StyleSheet\.create|\(t: Tokens\)/.test(
      x.body,
    );
    if (!hasFactory) offenders.push(rel(x.file));
  }
  check('every stylesheet is built from tokens', offenders.length === 0, offenders.join('\n'));
}

console.log('\n=== horizontal scrollers ===');

{
  // A horizontal ScrollView sizes to its content and clips on sub-pixel
  // rounding — it sheared the top border off the Activity filter chips. Where
  // the content fits, a wrapping row is correct and cannot clip.
  const offenders = scanned
    .filter((x) => /<ScrollView[\s\S]{0,200}?horizontal/.test(x.body))
    .map((x) => rel(x.file));
  check(
    'no horizontal ScrollView holding a short chip row',
    offenders.length === 0,
    offenders.join('\n'),
  );
}

console.log(`\n  PASSED: ${passed}   FAILED: ${failed}`);
process.exit(failed > 0 ? 1 : 0);
