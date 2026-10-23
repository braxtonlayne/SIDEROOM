#!/usr/bin/env node
// Signs in as the Owner through the public URL (Caddy, TLS) and reads Admin > System, for the production smoke
// (.github/workflows/prod-smoke.yml, RDY-OPS-001/002). It uses only the public HTTP API, the way the web app does.
//
//   node deploy/scripts/smoke-admin.mjs signin
//        sign in, check the session (/sr/csrf, /admin/system) and that the database and storage probes are ok
//   node deploy/scripts/smoke-admin.mjs backup-time [--dir /var/backups/sideroom] [--max-age-min 30] [--expect-verify]
//        point the platform setting ops.backup_dir at the backup folder the server can read, then check that
//        Admin > System reports a backup made within the last --max-age-min minutes (and, with --expect-verify,
//        a restore-drill time)
//
//   SMOKE_URL             public URL, for example https://smoke.localhost
//   SMOKE_OWNER_USERNAME  default owner
//   SMOKE_OWNER_PASSWORD_FILE  file holding the Owner password (never pass it as an argument)
//   NODE_EXTRA_CA_CERTS   the Caddy root certificate (internal CA), so Node trusts the site
import { readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';

const { positionals, values } = parseArgs({
  allowPositionals: true,
  options: {
    dir: { type: 'string', default: '/var/backups/sideroom' },
    'max-age-min': { type: 'string', default: '30' },
    'expect-verify': { type: 'boolean', default: false },
  },
});
const mode = positionals[0];
if (!['signin', 'backup-time'].includes(mode ?? '')) {
  console.error('usage: smoke-admin.mjs signin | backup-time [--dir D] [--max-age-min N] [--expect-verify]');
  process.exit(2);
}
const base = (process.env.SMOKE_URL ?? '').replace(/\/$/, '');
if (!base) throw new Error('set SMOKE_URL');
const username = process.env.SMOKE_OWNER_USERNAME ?? 'owner';
const password = readFileSync(
  process.env.SMOKE_OWNER_PASSWORD_FILE ??
    (() => {
      throw new Error('set SMOKE_OWNER_PASSWORD_FILE');
    })(),
  'utf8',
).trim();

const jar = new Map();
async function call(method, path, body, extra = {}) {
  const cookie = [...jar].map(([k, v]) => `${k}=${v}`).join('; ');
  const res = await fetch(`${base}/api/v1${path}`, {
    method,
    headers: {
      Origin: base,
      ...(cookie ? { Cookie: cookie } : {}),
      ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
      ...extra,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  for (const line of res.headers.getSetCookie()) {
    const [pair] = line.split(';');
    const i = pair.indexOf('=');
    const name = pair.slice(0, i).trim();
    const value = pair.slice(i + 1).trim();
    if (value === '' || /;\s*max-age=0/i.test(line)) jar.delete(name);
    else jar.set(name, value);
  }
  const text = await res.text();
  let json;
  try {
    json = text ? JSON.parse(text) : undefined;
  } catch {
    json = undefined;
  }
  if (!res.ok) throw new Error(`${method} ${path} -> ${res.status}: ${text.slice(0, 300)}`);
  return json;
}

const login = await call('POST', '/sr/auth/login', { login: username, password });
if (login?.status !== 'ok') {
  throw new Error(`sign-in did not complete: ${JSON.stringify(login)}`);
}
console.log(`signed in as ${username} through ${base}`);
const { token } = await call('GET', '/sr/csrf');
if (!token) throw new Error('no CSRF token for the signed-in session');

async function system() {
  return call('GET', '/admin/system');
}

let sys = await system();
console.log(`Admin > System: version ${sys.version}`);
const bad = (sys.probes ?? []).filter((p) => !p.ok);
for (const p of sys.probes ?? []) console.log(`  probe ${p.name}: ${p.ok ? 'ok' : `FAILED ${p.error ?? ''}`}`);
if (bad.length) throw new Error(`readiness probes failing: ${bad.map((p) => p.name).join(', ')}`);

if (mode === 'backup-time') {
  await call('PATCH', '/admin/settings', { values: { 'ops.backup_dir': values.dir } }, { 'X-SR-CSRF': token });
  sys = await system();
  const last = sys.backups?.last_run_ms;
  if (typeof last !== 'number') {
    throw new Error(`Admin > System shows no backup time for ${values.dir}: ${JSON.stringify(sys.backups)}`);
  }
  const ageMin = (Date.now() - last) / 60000;
  console.log(`Admin > System: last backup ${new Date(last).toISOString()} (${ageMin.toFixed(1)} min ago)`);
  if (ageMin > Number(values['max-age-min']) || ageMin < -5) {
    throw new Error(`last backup time is implausible (${ageMin.toFixed(1)} min ago)`);
  }
  if (values['expect-verify']) {
    const verify = sys.backups?.last_verify_ms;
    if (typeof verify !== 'number') throw new Error('Admin > System shows no restore-drill time');
    console.log(`Admin > System: last restore check ${new Date(verify).toISOString()}`);
  }
}
console.log('smoke-admin: ok');
