/**
 * Resolve the mobile app's `@/…` path alias for plain Node.
 *
 * Node honours neither tsconfig `paths` nor Metro's resolver, so a pure module
 * that imports `@/session/capabilities` cannot be loaded by a test script
 * without this. Node 23.6+ strips the types itself; only the alias is missing.
 *
 * Used via `register()` — see scripts/verify-nav.mjs.
 */

import path from 'node:path';
import { pathToFileURL } from 'node:url';

const SRC = path.resolve(import.meta.dirname, '../../apps/mobile/src');

export function resolve(specifier, context, next) {
  if (!specifier.startsWith('@/')) return next(specifier, context);

  const target = path.join(SRC, specifier.slice(2));
  // Mirrors Metro's extension search. Extensionless imports are the norm in the
  // app, so guessing wrong here would look like a missing module.
  for (const candidate of [`${target}.ts`, `${target}.tsx`, path.join(target, 'index.ts')]) {
    try {
      return next(pathToFileURL(candidate).href, context);
    } catch {
      // Try the next extension.
    }
  }
  return next(specifier, context);
}
