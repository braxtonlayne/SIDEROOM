// XC-OPS-002: deploy/install.sh, run against a throwaway copy of deploy/ with a fake `docker` on PATH that
// records every call. Nothing here needs Docker. Run with: node --test "scripts/*.test.mjs"
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  chmodSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const posix = (p) => p.replaceAll('\\', '/');
// An absolute POSIX path for the install script (Git Bash sees C:/x as /c/x).
const shPath = (p) => posix(p).replace(/^([A-Za-z]):/, (_, d) => `/${d.toLowerCase()}`);

function sandbox() {
  const dir = mkdtempSync(join(tmpdir(), 'sr-install-'));
  cpSync(join(root, 'deploy', 'install.sh'), join(dir, 'deploy', 'install.sh'));
  cpSync(join(root, 'deploy', 'compose'), join(dir, 'deploy', 'compose'), { recursive: true });
  const bin = join(dir, 'bin');
  mkdirSync(bin);
  const log = join(dir, 'docker.log');
  writeFileSync(
    join(bin, 'docker'),
    `#!/bin/sh
echo "$*" >> "${posix(log)}"
case "$*" in
  *"node id"*) echo "abc123def@127.0.0.1:3901" ;;
  *"/garage status"*) echo "abc123def  host  dc1  1G" ;;
esac
exit 0
`,
  );
  chmodSync(join(bin, 'docker'), 0o755);
  return {
    dir,
    bin,
    log,
    script: join(dir, 'deploy', 'install.sh'),
    env: join(dir, 'deploy', 'compose', '.env.prod'),
    garage: join(dir, 'deploy', 'compose', 'garage', 'garage.prod.toml'),
  };
}

const answers = {
  SR_DOMAIN: 'chat.example.org',
  SR_IMAGE: 'ghcr.io/acme/sideroom-server:1.2.3',
  SR_WEB_DIR: '/srv/sideroom-web',
  SMTP_HOST: 'mail.example.org',
  SMTP_PORT: '465',
  SMTP_TLS: 'tls',
  SMTP_USERNAME: 'mailer',
  SMTP_PASSWORD: 'plain-smtp-password',
  SR_OWNER_USERNAME: 'boss',
  SR_OWNER_EMAIL: 'boss@example.org',
  SR_OWNER_PASSWORD: 'correct horse battery staple',
};

function run(sb, args, extra = {}) {
  const r = spawnSync('sh', [posix(sb.script), ...args], {
    env: {
      ...process.env,
      PATH: `${posix(sb.bin)}:${process.env.PATH}`,
      ...answers,
      SR_INGEST_DIR: shPath(join(sb.dir, 'ingest')),
      ...extra,
    },
    encoding: 'utf8',
    input: '',
  });
  return { ...r, out: `${r.stdout}${r.stderr}` };
}

const envOf = (path) => {
  const out = {};
  for (const line of readFileSync(path, 'utf8').split('\n')) {
    const m = /^([A-Z_]+)=(.*)$/.exec(line);
    if (m) out[m[1]] = m[2];
  }
  return out;
};

test('--no-start writes a complete, secret-filled env file and garage config', () => {
  const sb = sandbox();
  const r = run(sb, ['--yes', '--no-start']);
  assert.equal(r.status, 0, r.out);
  const env = envOf(sb.env);
  const example = envOf(join(root, 'deploy', 'compose', '.env.prod.example'));
  assert.deepEqual(
    Object.keys(env).sort(),
    Object.keys(example).sort(),
    'same keys as the example',
  );
  assert.equal(env.SR_DOMAIN, 'chat.example.org');
  assert.equal(env.SR_IMAGE, 'ghcr.io/acme/sideroom-server:1.2.3');
  assert.equal(env.SMTP_PORT, '465');
  assert.equal(env.SMTP_TLS, 'tls');
  assert.match(env.SR_SECRET_KEY, /^[0-9a-f]{64}$/);
  assert.match(env.GARAGE_ACCESS_KEY, /^GK[0-9a-f]{24}$/);
  assert.match(env.GARAGE_SECRET_KEY, /^[0-9a-f]{64}$/);
  assert.match(env.POSTGRES_PASSWORD, /^[0-9a-f]{48}$/);
  assert.ok(!/^[A-Z_]+=.*CHANGE_ME/m.test(readFileSync(sb.env, 'utf8')));
  const toml = readFileSync(sb.garage, 'utf8');
  assert.ok(!/^[a-z_]+ *=.*CHANGE_ME/m.test(toml));
  assert.match(toml, /^rpc_secret = "[0-9a-f]{64}"$/m);
  assert.match(toml, /^admin_token = "[0-9a-f]{64}"$/m);
  assert.match(toml, /^metrics_token = "[0-9a-f]{64}"$/m);
  // Nothing ran: --no-start only prints the commands.
  const ran = existsSync(sb.log) ? readFileSync(sb.log, 'utf8') : '';
  assert.ok(
    !/ (pull|up|run|exec)/.test(ran),
    `--no-start ran docker:
${ran}`,
  );
  assert.match(r.out, /Not started/);
  if (process.platform !== 'win32') {
    assert.equal((statSync(sb.env).mode & 0o777).toString(8), '600');
  }
});

test('secrets are fresh on every install and an existing install is not overwritten without --force', () => {
  const a = sandbox();
  const b = sandbox();
  assert.equal(run(a, ['--yes', '--no-start']).status, 0);
  assert.equal(run(b, ['--yes', '--no-start']).status, 0);
  assert.notEqual(envOf(a.env).SR_SECRET_KEY, envOf(b.env).SR_SECRET_KEY);

  const before = readFileSync(a.env, 'utf8');
  const again = run(a, ['--yes', '--no-start']);
  assert.notEqual(again.status, 0);
  assert.match(again.out, /already installed/);
  assert.equal(readFileSync(a.env, 'utf8'), before, 'the refused run changed nothing');

  const forced = run(a, ['--yes', '--no-start', '--force']);
  assert.equal(forced.status, 0, forced.out);
  assert.equal(readFileSync(`${a.env}.bak`, 'utf8'), before, 'the old file is kept as .bak');
  assert.notEqual(readFileSync(a.env, 'utf8'), before);
});

test('awkward SMTP values survive a round trip through the env file', () => {
  const sb = sandbox();
  const password = 'p#a$s\\w"d&x y';
  const r = run(sb, ['--yes', '--no-start'], {
    SMTP_PASSWORD: password,
    SMTP_FROM: 'SIDE ROOM <no-reply@example.org>',
  });
  assert.equal(r.status, 0, r.out);
  // The file is a valid shell/compose env file: sourcing it gives back exactly what was typed.
  const read = (name) =>
    spawnSync('sh', ['-c', `. '${posix(sb.env)}'; printf %s "$${name}"`], { encoding: 'utf8' })
      .stdout;
  assert.equal(read('SMTP_PASSWORD'), password);
  assert.equal(read('SMTP_FROM'), 'SIDE ROOM <no-reply@example.org>');
});

test('bad answers are refused before anything is written', () => {
  for (const [patch, message] of [
    [{ SR_DOMAIN: 'not a host' }, /SR_DOMAIN/],
    [{ SR_DOMAIN: 'localhost' }, /SR_DOMAIN/],
    [{ SMTP_HOST: '' }, /SMTP_HOST/],
    [{ SMTP_PORT: 'abc' }, /SMTP_PORT/],
    [{ SMTP_TLS: 'maybe' }, /SMTP_TLS/],
    [{ SR_OWNER_EMAIL: 'nope' }, /SR_OWNER_EMAIL/],
    [{ SR_GARAGE_CAPACITY: 'lots' }, /SR_GARAGE_CAPACITY/],
    [{ SMTP_PASSWORD: "it's" }, /single quote/],
  ]) {
    const sb = sandbox();
    const r = run(sb, ['--yes', '--no-start'], patch);
    assert.notEqual(r.status, 0, JSON.stringify(patch));
    assert.match(r.out, message, JSON.stringify(patch));
    assert.ok(!existsSync(sb.env), `${JSON.stringify(patch)} left an env file behind`);
  }
});

test('the full run brings the stack up in dependency order and keeps the Owner password off the command line', () => {
  const sb = sandbox();
  const r = run(sb, ['--yes']);
  assert.equal(r.status, 0, r.out);
  const calls = readFileSync(sb.log, 'utf8').trim().split('\n');
  const idx = (needle) => {
    const i = calls.findIndex((c) => c.includes(needle));
    assert.notEqual(i, -1, `no docker call containing "${needle}"\n${calls.join('\n')}`);
    return i;
  };
  const order = [
    idx(' pull'),
    idx('up -d --wait postgres'),
    idx('up -d garage'),
    idx('/garage status'), // bootstrap.sh talking to Garage
    idx('sr-cli sr-server migrate'),
    idx('storage ensure-cors --origin https://chat.example.org'),
    idx('sr-cli sr-server bootstrap --owner-username boss --owner-email boss@example.org'),
    calls.length - 1, // the final `up -d`
  ];
  assert.deepEqual(
    [...order].sort((a, b) => a - b),
    order,
    'steps ran out of order',
  );
  assert.match(calls.at(-1), / up -d$/);
  assert.ok(
    calls.every((c) => !c.includes(answers.SR_OWNER_PASSWORD)),
    'the Owner password reached a command line',
  );
  assert.ok(
    calls.every(
      (c) => c === 'compose version' || !c.startsWith('compose') || c.includes('--env-file'),
    ),
    'every compose call reads .env.prod',
  );
  // bootstrap.sh was pointed at the production stack and capacity, not the dev defaults.
  assert.ok(
    calls.some(
      (c) =>
        c.includes('prod.yml') &&
        c.includes('layout assign') === false &&
        c.includes('exec -T garage'),
    ),
  );
  assert.match(r.out, /SIDE ROOM is starting at https:\/\/chat\.example\.org/);
});

test('a missing docker or compose plugin is a clear error', () => {
  const sb = sandbox();
  writeFileSync(join(sb.bin, 'docker'), '#!/bin/sh\nexit 1\n');
  const r = run(sb, ['--yes', '--no-start']);
  assert.notEqual(r.status, 0);
  assert.match(r.out, /compose plugin is missing/);
});

test('the ingest folder is asked for, created, and wired as a read-only root (H-48)', () => {
  const sb = sandbox();
  const dir = shPath(join(sb.dir, 'ingest'));
  const r = run(sb, ['--yes', '--no-start']);
  assert.equal(r.status, 0, r.out);
  const env = envOf(sb.env);
  assert.equal(env.SR_INGEST_DIR, dir);
  assert.equal(env.SR_INGEST_ROOTS, '/srv/sideroom/ingest');
  assert.ok(existsSync(join(sb.dir, 'ingest')), 'the missing folder was created');

  // "none" turns local folders off: no root, the mount source stays the default folder.
  const off = sandbox();
  const r2 = run(off, ['--yes', '--no-start'], { SR_INGEST_DIR: 'none' });
  assert.equal(r2.status, 0, r2.out);
  assert.equal(envOf(off.env).SR_INGEST_ROOTS, '');
  assert.equal(envOf(off.env).SR_INGEST_DIR, '/srv/sideroom/ingest');

  for (const bad of ['relative/dir', '/srv/in gest', '/srv/../etc']) {
    const sb3 = sandbox();
    const r3 = run(sb3, ['--yes', '--no-start'], { SR_INGEST_DIR: bad });
    assert.notEqual(r3.status, 0, bad);
    assert.match(r3.out, /SR_INGEST_DIR/, bad);
    assert.ok(!existsSync(sb3.env), `${bad} left an env file behind`);
  }
});

test('starter packs under packages/starter-packs are seeded after the Owner exists (M-03)', () => {
  const sb = sandbox();
  const packs = join(sb.dir, 'packages', 'starter-packs', 'side-room-words');
  mkdirSync(packs, { recursive: true });
  writeFileSync(join(packs, 'pack.json'), '{}');
  const r = run(sb, ['--yes']);
  assert.equal(r.status, 0, r.out);
  const calls = readFileSync(sb.log, 'utf8').trim().split('\n');
  const seed = calls.findIndex((c) =>
    c.includes('sr-cli sr-server package seed /starter-packs/side-room-words'),
  );
  const owner = calls.findIndex((c) => c.includes('sr-cli sr-server bootstrap'));
  assert.ok(seed > owner && owner !== -1, calls.join('\n'));
  assert.ok(calls[seed].includes(':/starter-packs:ro'), 'the folder is mounted read-only');
  assert.match(calls.at(-1), / up -d$/);
});

test('the electrs payments profile is off unless asked for (M-02, D-288)', () => {
  const off = sandbox();
  assert.equal(run(off, ['--no-start']).status, 0);
  const envOff = readFileSync(off.env, 'utf8');
  assert.match(envOff, /^SR_PAYMENTS_ESPLORA_URL=$/m);
  assert.doesNotMatch(envOff, /COMPOSE_PROFILES/);
  assert.match(envOff, /api\.coinbase\.com.*api\.kraken\.com.*api\.coingecko\.com/);

  const on = sandbox();
  assert.equal(run(on, ['--no-start'], { SR_PAYMENTS_ELECTRS: 'yes' }).status, 0);
  const envOn = readFileSync(on.env, 'utf8');
  assert.match(envOn, /^SR_PAYMENTS_ESPLORA_URL=http:\/\/electrs:3000$/m);
  assert.match(envOn, /^SR_PAYMENTS_ALLOW_PRIVATE_HOSTS=1$/m);
  assert.match(envOn, /^COMPOSE_PROFILES=payments$/m);
});

test('SR_INSTALL_SKIP_PULL=1 skips the image pull and nothing else (RDY-OPS-001)', () => {
  const sb = sandbox();
  const r = run(sb, ['--yes'], { SR_INSTALL_SKIP_PULL: '1' });
  assert.equal(r.status, 0, r.out);
  const calls = readFileSync(sb.log, 'utf8').trim().split('\n');
  assert.ok(!calls.some((c) => / pull/.test(c)), `a pull ran:\n${calls.join('\n')}`);
  assert.ok(calls.some((c) => c.includes('sr-cli sr-server migrate')));
  assert.ok(calls.at(-1).endsWith(' up -d'));
  assert.match(r.out, /skipping the image pull/);
});
