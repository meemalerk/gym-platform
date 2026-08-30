#!/usr/bin/env node
// Regenerate a client's API types from the running server's OpenAPI document.
//
// This exists because the npm script it replaces read
//
//   openapi-typescript http://localhost:${API_PORT:-8080}/... -o schema.d.ts
//
// and npm runs package scripts through cmd.exe on Windows, which does not do
// POSIX parameter expansion. The literal "${API_PORT:-8080}" was handed to
// openapi-typescript, which died with ERR_INVALID_URL — so the one command the
// README gives for keeping client types in step with the backend could not be
// run on Windows at all. Node reads process.env identically on every platform.
//
//   node scripts/codegen-api.mjs <output-file> [default-port]
//
// The default port differs per client on purpose: the console proxies :8080
// (Quick start), while the mobile app's phone workflow puts the API on :8092
// (scripts/dev-phone.sh). API_PORT overrides either.
import { spawnSync } from 'node:child_process'

const [out, fallbackPort = '8080'] = process.argv.slice(2)
if (!out) {
  console.error('usage: codegen-api.mjs <output-file> [default-port]')
  process.exit(2)
}

const port = process.env.API_PORT || fallbackPort
const url = `http://localhost:${port}/api-docs/openapi.json`
console.log(`generating ${out} from ${url}`)

// shell on Windows: npx is npx.cmd there, which spawnSync cannot exec directly.
const result = spawnSync('npx', ['--yes', 'openapi-typescript@7', url, '-o', out], {
  stdio: 'inherit',
  shell: process.platform === 'win32',
})

if (result.error) {
  console.error(result.error.message)
  process.exit(1)
}
if (result.status !== 0) {
  console.error(`\nCould not reach ${url} — is the server running? (see README, Quick start)`)
}
process.exit(result.status ?? 1)
