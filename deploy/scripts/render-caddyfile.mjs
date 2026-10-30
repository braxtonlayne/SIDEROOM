#!/usr/bin/env node
// Rewrites the public-surface block of deploy/compose/caddy/Caddyfile.prod (between `# surfaces:begin` and
// `# surfaces:end`) from apps/web/public-surfaces.json, using apps/web/src/security/csp.ts, so the Caddyfile, the
// built web app and sr-server cannot drift (D-385). The app policy is no longer in the Caddyfile; the activity and
// voice origins (H-27) reach the app through dist/app/csp.txt. Needs Node 22.6 or newer.
//
//   node deploy/scripts/render-caddyfile.mjs --public-url https://chat.example.org [--activities]
//        [--voice-url wss://voice.example.org] [--file path/to/Caddyfile.prod] [--check]
//
// With --check nothing is written; the exit code is 1 when the file is not what the settings produce.
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const here = dirname(fileURLToPath(import.meta.url));
const { values } = parseArgs({
  options: {
    'public-url': { type: 'string' },
    activities: { type: 'boolean', default: false },
    'voice-url': { type: 'string' },
    file: { type: 'string', default: resolve(here, '../compose/caddy/Caddyfile.prod') },
    check: { type: 'boolean', default: false },
  },
});
const { renderCaddyfile } = await import(
  pathToFileURL(resolve(here, '../../apps/web/src/security/csp.ts')).href
);
const table = JSON.parse(
  readFileSync(resolve(here, '../../apps/web/public-surfaces.json'), 'utf8'),
);
const current = readFileSync(values.file, 'utf8');
const next = renderCaddyfile(current, table);
if (values.check) {
  if (next !== current) {
    console.error(`${values.file} does not match the settings; run without --check to rewrite it.`);
    process.exit(1);
  }
} else if (next !== current) {
  writeFileSync(values.file, next);
  console.log(`updated ${values.file}`);
}
