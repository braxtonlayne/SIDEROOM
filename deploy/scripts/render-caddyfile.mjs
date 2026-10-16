#!/usr/bin/env node
// Rewrites the Content-Security-Policy line of deploy/compose/caddy/Caddyfile.prod from the same two settings
// the web build reads (activities.enabled, voice.public_url), using apps/web/src/security/csp.ts, so the
// Caddyfile and the built web app cannot drift (H-27, OQ-XC-3). Needs Node 22.6 or newer.
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
const { cspOptionsFromConfig, renderCaddyfile } = await import(
  pathToFileURL(resolve(here, '../../apps/web/src/security/csp.ts')).href
);
const opts = cspOptionsFromConfig({
  publicUrl: values['public-url'],
  activitiesEnabled: values.activities,
  voicePublicUrl: values['voice-url'],
});
const current = readFileSync(values.file, 'utf8');
const next = renderCaddyfile(current, opts);
if (values.check) {
  if (next !== current) {
    console.error(`${values.file} does not match the settings; run without --check to rewrite it.`);
    process.exit(1);
  }
} else if (next !== current) {
  writeFileSync(values.file, next);
  console.log(`updated ${values.file}`);
}
