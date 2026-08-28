/**
 * Documentation that cannot rot quietly.
 *
 * A dead link in a README is a small thing that erodes trust in every other
 * link beside it. This walks every markdown file, resolves every relative link,
 * and fails if one points at nothing — which is what lets the docs stay FLAT
 * and grouped by an index rather than by folders (see docs/README.md).
 *
 *   node scripts/verify-docs.mjs
 */

import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// fileURLToPath, never `.pathname`: on Windows a file URL's pathname is
// `/C:/Users/…` with percent-escapes, which `resolve` then turns into the
// nonexistent `C:\C:\Users\Big%20PC\…`. This script simply never ran there.
const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));

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

function markdown(dir, acc = []) {
  for (const name of readdirSync(dir)) {
    if (['node_modules', 'target', '.git', '.expo', 'dist'].includes(name)) continue;
    const full = join(dir, name);
    if (statSync(full).isDirectory()) markdown(full, acc);
    else if (name.endsWith('.md')) acc.push(full);
  }
  return acc;
}

const files = markdown(ROOT);
const rel = (f) => relative(ROOT, f).replaceAll('\\', '/');

console.log(`\n=== ${files.length} markdown files ===`);

// --------------------------------------------------------------- dead links
{
  const dead = [];
  for (const file of files) {
    const text = readFileSync(file, 'utf8');
    // Skip fenced code — a path inside an example is illustration, not a link.
    const prose = text.replace(/```[\s\S]*?```/g, '');

    for (const m of prose.matchAll(/\[[^\]]*\]\(([^)\s]+)\)/g)) {
      const href = m[1];
      if (/^(https?:|mailto:|#)/.test(href)) continue;

      const [path] = href.split('#');
      if (!path) continue; // pure anchor

      const target = resolve(dirname(file), path);
      if (!existsSync(target)) dead.push(`${rel(file)} → ${href}`);
    }
  }
  check('every relative link resolves', dead.length === 0, dead.join('\n'));
}

// ------------------------------------------------- every ADR is in the index
{
  const adrDir = join(ROOT, 'docs', 'adr');
  const adrs = readdirSync(adrDir)
    .filter((n) => /^\d{4}-/.test(n) && !n.startsWith('0000'))
    .sort();
  const index = readFileSync(join(adrDir, 'README.md'), 'utf8');
  const missing = adrs.filter((n) => !index.includes(n));
  check(
    `all ${adrs.length} ADRs are listed in adr/README.md`,
    missing.length === 0,
    missing.join('\n'),
  );

  // Numbering with a hole in it means someone wrote 0023 while 0022 was still
  // a branch — worth catching before both exist.
  const numbers = adrs.map((n) => Number.parseInt(n.slice(0, 4), 10));
  const gaps = numbers.filter((n, i) => i > 0 && n !== numbers[i - 1] + 1);
  check('ADR numbering has no gaps', gaps.length === 0, gaps.map((n) => `gap before ${n}`).join('\n'));
}

// --------------------------------------------- the entry points really exist
{
  const entry = [
    'README.md',
    'START-HERE.md',
    'START HERE.html',
    'CLAUDE.md',
    'docs/README.md',
    'docs/product-specification.md',
    'docs/developer-guide.md',
    'docs/delivery-stages.md',
  ];
  const missing = entry.filter((f) => !existsSync(join(ROOT, f)));
  check('the documented entry points exist', missing.length === 0, missing.join('\n'));
}

// ------------------------------------ the launchers a non-technical user needs
{
  // These filenames are promised by START HERE. If one is renamed without the
  // instructions being updated, the reader is told to double-click a file that
  // is not there — the single worst failure for a first-run experience.
  const launchers = [
    'start-demo.bat',
    'start-demo.command',
    'start-demo.sh',
    'stop-demo.bat',
    'stop-demo.command',
    'stop-demo.sh',
    'share-demo.bat',
    'share-demo.sh',
  ];
  const missing = launchers.filter((f) => !existsSync(join(ROOT, f)));
  check('every launcher named in the instructions exists', missing.length === 0, missing.join('\n'));
}

console.log(`\n  PASSED: ${passed}   FAILED: ${failed}`);
process.exit(failed > 0 ? 1 : 0);
