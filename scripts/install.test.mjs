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
  readdirSync,
  rmSync,
  readFileSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, test } from 'node:test';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const posix = (p) => p.replaceAll('\\', '/');
// An absolute POSIX path for the install script (Git Bash sees C:/x as /c/x).
const shPath = (p) => posix(p).replace(/^([A-Za-z]):/, (_, d) => `/${d.toLowerCase()}`);

// Every sandbox lives inside the worktree (untracked, removed when the file is done), never in the system temp dir.
const scratch = join(root, 'scratch', 'install-tests');
mkdirSync(scratch, { recursive: true });
after(() => rmSync(scratch, { recursive: true, force: true }));

function sandbox() {
  const dir = mkdtempSync(join(scratch, 'sr-install-'));
  cpSync(join(root, 'deploy', 'install.sh'), join(dir, 'deploy', 'install.sh'));
  cpSync(join(root, 'deploy', 'compose'), join(dir, 'deploy', 'compose'), { recursive: true });
  const bin = join(dir, 'bin');
  mkdirSync(bin);
  const log = join(dir, 'docker.log');
  writeFileSync(
    join(bin, 'docker'),
    `#!/bin/sh
echo "$*" >> "${posix(log)}"
here=$(dirname "$0")
# Switches a test turns on by creating a file next to this stub.
case "$*" in
  *" pull") [ -f "$here/fail-pull" ] && { echo "pull failed" >&2; exit 1; } ;;
  *" bootstrap --owner-username"*)
    [ -f "$here/owner-done" ] && { echo "Error: this instance is already bootstrapped (accounts exist)" >&2; exit 1; }
    [ -f "$here/owner-half" ] && { echo "Error: this instance is already bootstrapped (accounts exist); its Staff Server is not finished, but only the same username, email and password as the first run can finish it" >&2; exit 1; }
    ;;
esac
case "$*" in
  # docker compose version (and --short): the text of the file compose-version next to this stub, else a current version.
  "compose version"*) if [ -f "$here/compose-version" ]; then cat "$here/compose-version"; else echo "2.29.1"; fi ;;
  *"node id"*) echo "abc123def@127.0.0.1:3901" ;;
  *"/garage status"*) echo "abc123def  host  dc1  1G" ;;
  *"ps -q sr-server"*) echo "c0ffee" ;;
  "inspect -f"*) echo "healthy" ;;
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
    // The built web app the installer insists on (apps/web/dist: it holds app/csp.txt).
    web: (() => {
      const web = join(dir, 'web');
      mkdirSync(join(web, 'app'), { recursive: true });
      writeFileSync(join(web, 'app', 'csp.txt'), "default-src 'self'\n");
      return web;
    })(),
    // A switch for the fake docker: the file's existence turns it on.
    stub: (name, on = true) => {
      if (on) writeFileSync(join(bin, name), '');
      else rmSync(join(bin, name), { force: true });
    },
  };
}

const answers = {
  SR_DOMAIN: 'chat.example.org',
  SR_IMAGE: 'ghcr.io/acme/sideroom-server:1.2.3',
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
      SR_WEB_DIR: shPath(sb.web),
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
    // Every value the installer writes is single-quoted (D-747): read it as compose does.
    if (m) out[m[1]] = m[2].replace(/^'(.*)'$/s, '$1');
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

test('secrets are fresh on every install, and a second run resumes: it never rewrites the files', () => {
  const a = sandbox();
  const b = sandbox();
  assert.equal(run(a, ['--yes', '--no-start']).status, 0);
  assert.equal(run(b, ['--yes', '--no-start']).status, 0);
  assert.notEqual(envOf(a.env).SR_SECRET_KEY, envOf(b.env).SR_SECRET_KEY);

  // D-747: before this, the second run died with "already installed" and --force was the only way on.
  const before = readFileSync(a.env, 'utf8');
  const garageBefore = readFileSync(a.garage, 'utf8');
  const again = run(a, ['--yes', '--no-start']);
  assert.equal(again.status, 0, again.out);
  assert.doesNotMatch(again.out, /already installed/);
  assert.match(again.out, /resuming/);
  assert.equal(
    readFileSync(a.env, 'utf8'),
    before,
    'a resumed run leaves the env file byte for byte',
  );
  assert.equal(readFileSync(a.garage, 'utf8'), garageBefore, 'and the Garage config');
});

/** The backups of a file a --force run kept: <file>.bak.<UTC yyyymmddThhmmssZ>[-n]. */
const backupsOf = (path) =>
  readdirSync(dirname(path))
    .filter((f) => f.startsWith(`${path.split(/[\\/]/).at(-1)}.bak.`))
    .sort();

test('--force keeps every earlier file as its own dated backup and says what it is for (D-747)', () => {
  const a = sandbox();
  assert.equal(run(a, ['--yes', '--no-start']).status, 0);
  const first = readFileSync(a.env, 'utf8');
  const firstGarage = readFileSync(a.garage, 'utf8');

  const one = run(a, ['--yes', '--no-start', '--force']);
  assert.equal(one.status, 0, one.out);
  assert.match(one.out, /only for a machine whose data volumes are gone/);
  const second = readFileSync(a.env, 'utf8');
  assert.notEqual(second, first);
  const two = run(a, ['--yes', '--no-start', '--force']);
  assert.equal(two.status, 0, two.out);
  assert.notEqual(readFileSync(a.env, 'utf8'), second);

  // Before this the second --force overwrote the single `.bak`: the first keys were gone.
  const envBaks = backupsOf(a.env);
  assert.equal(envBaks.length, 2, envBaks.join(', '));
  for (const f of envBaks) assert.match(f, /\.bak\.\d{8}T\d{6}Z(-\d+)?$/);
  const contents = envBaks.map((f) => readFileSync(join(dirname(a.env), f), 'utf8'));
  assert.ok(contents.includes(first), 'the first install is still kept');
  assert.ok(contents.includes(second), 'and the one --force wrote after it');
  const garageBaks = backupsOf(a.garage);
  assert.equal(garageBaks.length, 2);
  assert.ok(
    garageBaks.map((f) => readFileSync(join(dirname(a.garage), f), 'utf8')).includes(firstGarage),
  );
  if (process.platform !== 'win32') {
    for (const f of [...envBaks.map((n) => join(dirname(a.env), n)), a.env, a.garage]) {
      assert.equal((statSync(f).mode & 0o777).toString(8), '600', f);
    }
  }
});

test('a first run that fails after the files are written is finished by running it again (D-747)', () => {
  const sb = sandbox();
  sb.stub('fail-pull');
  const first = run(sb, ['--yes']);
  assert.notEqual(first.status, 0, 'the pull fails');
  assert.ok(existsSync(sb.env) && existsSync(sb.garage), 'the files were written before it failed');
  const env = readFileSync(sb.env, 'utf8');
  const garage = readFileSync(sb.garage, 'utf8');

  sb.stub('fail-pull', false);
  writeFileSync(sb.log, '');
  const second = run(sb, ['--yes']);
  assert.equal(second.status, 0, second.out);
  assert.doesNotMatch(second.out, /already installed/);
  assert.equal(readFileSync(sb.env, 'utf8'), env, 'no secret was generated or rewritten');
  assert.equal(readFileSync(sb.garage, 'utf8'), garage);
  assert.deepEqual(backupsOf(sb.env), [], 'a resume makes no backup: nothing is replaced');
  // It went on through every step, with the Owner created once.
  const calls = readFileSync(sb.log, 'utf8').trim().split('\n');
  assert.ok(calls.some((c) => c.includes('sr-cli sr-server migrate')));
  assert.equal(calls.filter((c) => c.includes('sr-server bootstrap --owner-username')).length, 1);
  assert.ok(calls.at(-1).endsWith(' sr-server preflight'));
  assert.match(second.out, /SIDE ROOM is starting at https:\/\/chat\.example\.org/);
});

test('a resumed run reads the files: the answers given now do not replace the installed ones', () => {
  const sb = sandbox();
  assert.equal(run(sb, ['--yes', '--no-start']).status, 0);
  const env = readFileSync(sb.env, 'utf8');
  // The mail answers are not asked again, so none is needed; a different domain is ignored, with a warning.
  const r = run(sb, ['--yes', '--no-start'], {
    SMTP_HOST: '',
    SR_DOMAIN: 'other.example.org',
    SR_IMAGE: '',
  });
  assert.equal(r.status, 0, r.out);
  assert.match(r.out, /SR_DOMAIN is already chat\.example\.org/);
  assert.match(r.out, /https:\/\/chat\.example\.org/);
  assert.equal(readFileSync(sb.env, 'utf8'), env);
  // The Garage config alone is made again when an earlier run stopped between the two files.
  rmSync(sb.garage);
  const again = run(sb, ['--yes', '--no-start']);
  assert.equal(again.status, 0, again.out);
  assert.ok(existsSync(sb.garage));
  assert.equal(readFileSync(sb.env, 'utf8'), env);
  // A Garage config with no env file is not guessed at.
  const lone = sandbox();
  assert.equal(run(lone, ['--yes', '--no-start']).status, 0);
  rmSync(lone.env);
  const refused = run(lone, ['--yes', '--no-start']);
  assert.notEqual(refused.status, 0);
  assert.match(refused.out, /garage\.prod\.toml exists but .*\.env\.prod does not/);
});

test('the Owner step: an existing Owner is read as done on a resume, a half-made one is not (D-747)', () => {
  const sb = sandbox();
  assert.equal(run(sb, ['--yes']).status, 0);
  // Resumed, sr-cli refuses a second Owner: that is "done", not a failure, and no password is made up and shown.
  sb.stub('owner-done');
  const r = run(sb, ['--yes'], { SR_OWNER_PASSWORD: '' });
  assert.equal(r.status, 0, r.out);
  assert.match(r.out, /Owner account already exists/);
  assert.doesNotMatch(r.out, /Owner password \(change it/);
  // A half-made Owner needs the first run's answers: the script says so and stops.
  sb.stub('owner-done', false);
  sb.stub('owner-half');
  const half = run(sb, ['--yes']);
  assert.notEqual(half.status, 0);
  assert.match(half.out, /half made/);
  assert.match(half.out, /SR_OWNER_PASSWORD/);
  // A first run is never told the Owner is "done": a database with accounts is the wrong database.
  const fresh = sandbox();
  fresh.stub('owner-done');
  const f = run(fresh, ['--yes']);
  assert.notEqual(f.status, 0);
  assert.match(f.out, /already has accounts/);
});

test('every value is single-quoted, and a value that is not data to a shell cannot be written (D-747)', () => {
  // Written bare before: ';' and '(' are a syntax error or a command when the file is sourced, as garage/bootstrap.sh did.
  for (const password of [
    'a;b',
    'a(b',
    'a`touch PWNED`b',
    'a&b',
    'a|b',
    'a<b>c',
    'a*b?c',
    '$(touch PWNED)',
  ]) {
    const sb = sandbox();
    const r = run(sb, ['--yes', '--no-start'], { SMTP_PASSWORD: password });
    assert.equal(r.status, 0, `${password}: ${r.out}`);
    const text = readFileSync(sb.env, 'utf8');
    // Every line the installer filled in (a line that is not the template's own) holds one single-quoted value.
    const template = new Set(
      readFileSync(join(root, 'deploy', 'compose', '.env.prod.example'), 'utf8').split('\n'),
    );
    let filled = 0;
    for (const line of text.split('\n')) {
      const m = /^([A-Z_]+)=(.*)$/.exec(line);
      if (!m || template.has(line)) continue;
      filled += 1;
      assert.match(m[2], /^'[^']*'$/, line);
    }
    assert.ok(filled >= 17, `only ${filled} filled lines found`);
    assert.equal(
      spawnSync('bash', ['-n', posix(sb.env)], { encoding: 'utf8' }).status,
      0,
      password,
    );
    const sourced = spawnSync(
      'sh',
      ['-c', `cd '${posix(sb.dir)}'; . '${posix(sb.env)}'; printf %s "$SMTP_PASSWORD"`],
      { encoding: 'utf8' },
    );
    assert.equal(sourced.stdout, password, password);
    assert.equal(sourced.stderr, '', password);
    assert.ok(!existsSync(join(sb.dir, 'PWNED')), `${password} ran a command`);
    assert.equal(envOf(sb.env).SMTP_PASSWORD, password);
  }
  // A single quote is refused, and the message names the answer.
  for (const [name, value] of [
    ['SMTP_PASSWORD', "it's"],
    ['SMTP_USERNAME', "o'brien"],
    ['SMTP_FROM', "SIDE ROOM <it's@example.org>"],
    ['SR_WEB_DIR', null],
    ['SR_IMAGE', "img'x"],
  ]) {
    const sb = sandbox();
    const given = name === 'SR_WEB_DIR' ? { SR_WEB_DIR: `${shPath(sb.web)}'s` } : { [name]: value };
    const r = run(sb, ['--yes', '--no-start'], given);
    assert.notEqual(r.status, 0, name);
    assert.match(r.out, new RegExp(`${name} may not contain a single quote`), name);
    assert.ok(!existsSync(sb.env), `${name} left an env file behind`);
  }
  const nl = sandbox();
  const rn = run(nl, ['--yes', '--no-start'], { SMTP_USERNAME: 'a\nb' });
  assert.notEqual(rn.status, 0);
  assert.match(rn.out, /SMTP_USERNAME must be a single line/);
});

test('garage/bootstrap.sh reads the three keys it needs and never runs the env file (D-747)', () => {
  const sb = sandbox();
  mkdirSync(join(sb.dir, 'run'));
  const envFile = join(sb.dir, 'run', 'env');
  // A hand-made file: keys as an operator might write them, and values that would be commands if the file were sourced.
  writeFileSync(
    envFile,
    [
      'SMTP_PASSWORD=a;touch PWNED',
      'SMTP_FROM=x`touch PWNED2`y',
      "GARAGE_ACCESS_KEY='GKquoted0123'",
      'GARAGE_SECRET_KEY="dq-secret"',
      'GARAGE_BUCKET=bare-bucket',
      '',
    ].join('\n'),
  );
  const script = posix(join(root, 'deploy', 'compose', 'garage', 'bootstrap.sh'));
  const base = { ...process.env, PATH: `${posix(sb.bin)}:${process.env.PATH}`, SR_SKIP_CORS: '1' };
  const r = spawnSync('bash', [script], {
    cwd: sb.dir,
    env: { ...base, SR_COMPOSE_FILE: 'prod.yml', SR_ENV_FILE: posix(envFile) },
    encoding: 'utf8',
  });
  assert.equal(r.status, 0, `${r.stdout}${r.stderr}`);
  assert.ok(
    !existsSync(join(sb.dir, 'PWNED')) && !existsSync(join(sb.dir, 'PWNED2')),
    'it sourced the file',
  );
  const calls = readFileSync(sb.log, 'utf8');
  assert.match(calls, /key info GKquoted0123/);
  assert.match(calls, /bucket allow --read --write --owner bare-bucket --key GKquoted0123/);
  assert.match(r.stdout, /bucket=bare-bucket/);
  // A file without the keys is a clear error, not an empty key.
  writeFileSync(envFile, 'SMTP_HOST=x\n');
  const bad = spawnSync('bash', [script], {
    cwd: sb.dir,
    env: { ...base, SR_ENV_FILE: posix(envFile) },
    encoding: 'utf8',
  });
  assert.notEqual(bad.status, 0);
  assert.match(bad.stderr, /GARAGE_ACCESS_KEY is not set/);
});

test('the Owner password rule is checked before a file is written, with the length sr-cli enforces (D-747)', () => {
  const cli = readFileSync(join(root, 'crates', 'sr-cli', 'src', 'commands.rs'), 'utf8');
  const rust = /const MIN_PASSWORD_LEN: usize = (\d+);/.exec(cli)?.[1];
  const installer = readFileSync(join(root, 'deploy', 'install.sh'), 'utf8');
  const script = /^MIN_OWNER_PASSWORD=(\d+)$/m.exec(installer)?.[1];
  assert.ok(rust && script, 'both constants found');
  assert.equal(script, rust, 'install.sh and sr-cli disagree on the shortest Owner password');

  const sb = sandbox();
  const short = 'x'.repeat(Number(rust) - 1);
  const r = run(sb, ['--yes'], { SR_OWNER_PASSWORD: short });
  assert.notEqual(r.status, 0);
  assert.match(r.out, new RegExp(`password must be at least ${rust} characters`));
  assert.ok(
    !existsSync(sb.env) && !existsSync(sb.garage),
    'a file was written before the password was checked',
  );
  assert.ok(
    !existsSync(sb.log) || !/ (pull|up|run)/.test(readFileSync(sb.log, 'utf8')),
    'something started',
  );
  // --no-start does not need a password at all; the exact length passes.
  const ok = run(sandbox(), ['--yes', '--no-start'], { SR_OWNER_PASSWORD: short });
  assert.equal(ok.status, 0, ok.out);
  const fine = run(sandbox(), ['--yes'], { SR_OWNER_PASSWORD: 'x'.repeat(Number(rust)) });
  assert.equal(fine.status, 0, fine.out);
});

test('each step of the env template is checked: a key missing from the template stops the run (D-747)', () => {
  // Before this the awks ran as one pipe and only the last one's status was read: a key missing from the middle of
  // the chain left its value out and the run went on.
  const sb = sandbox();
  const tpl = join(sb.dir, 'deploy', 'compose', '.env.prod.example');
  writeFileSync(
    tpl,
    readFileSync(tpl, 'utf8')
      .split('\n')
      .filter((l) => !l.startsWith('SMTP_PORT='))
      .join('\n'),
  );
  const r = run(sb, ['--yes', '--no-start']);
  assert.notEqual(r.status, 0, 'a missing SMTP_PORT line was not noticed');
  assert.match(r.out, /could not set SMTP_PORT/);
  assert.ok(!existsSync(sb.env), 'a half-filled env file was left behind');
  assert.ok(!existsSync(`${sb.env}.new`) && !existsSync(`${sb.env}.new.part`));
});

test('the web build is checked before anything is written or started, and the message says where it goes (D-747)', () => {
  // `docker compose up -d` used to run first; the server then exits at start (WebGate::from_config: a web root with
  // no app/csp.txt is an error that ends the process), so the install ended in a health wait that could never pass.
  const sb = sandbox();
  const empty = join(sb.dir, 'empty-web');
  mkdirSync(empty);
  const r = run(sb, ['--yes'], { SR_WEB_DIR: shPath(empty) });
  assert.notEqual(r.status, 0);
  assert.ok(r.out.includes(`the web build is not in ${shPath(empty)}`), r.out);
  assert.match(r.out, /app\/csp\.txt/);
  assert.match(r.out, /apps\/web\/dist/);
  assert.ok(!existsSync(sb.env) && !existsSync(sb.garage), 'a file was written');
  assert.ok(
    !existsSync(sb.log) || !/ (pull|up|run)/.test(readFileSync(sb.log, 'utf8')),
    'something started',
  );
  // --no-start prints commands and starts nothing: it does not need the build.
  const nb = run(sandbox(), ['--yes', '--no-start'], { SR_WEB_DIR: shPath(empty) });
  assert.equal(nb.status, 0, nb.out);
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
    calls.findLastIndex((c) => / up -d$/.test(c)), // the final `up -d`
    calls.length - 1, // and last of all the preflight (RDY-OPS-004)
  ];
  assert.deepEqual(
    [...order].sort((a, b) => a - b),
    order,
    'steps ran out of order',
  );
  assert.match(calls.at(-1), /--entrypoint \/usr\/local\/bin\/sr-cli sr-server preflight$/);
  assert.ok(
    calls.every((c) => !c.includes(answers.SR_OWNER_PASSWORD)),
    'the Owner password reached a command line',
  );
  assert.ok(
    calls.every(
      (c) =>
        c === 'compose version' ||
        c === 'compose version --short' ||
        c.startsWith('inspect') ||
        !c.startsWith('compose') ||
        c.includes('--env-file'),
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
  assert.ok(
    calls.findLastIndex((c) => / up -d$/.test(c)) > seed,
    'the stack starts after the packs',
  );
});

test('the electrs payments profile is off unless asked for (M-02, D-288)', () => {
  const off = sandbox();
  assert.equal(run(off, ['--no-start']).status, 0);
  const envOff = readFileSync(off.env, 'utf8');
  assert.match(envOff, /^SR_PAYMENTS_ESPLORA_URL=''$/m);
  assert.doesNotMatch(envOff, /COMPOSE_PROFILES/);
  assert.match(envOff, /api\.coinbase\.com.*api\.kraken\.com.*api\.coingecko\.com/);

  const on = sandbox();
  assert.equal(run(on, ['--no-start'], { SR_PAYMENTS_ELECTRS: 'yes' }).status, 0);
  const envOn = readFileSync(on.env, 'utf8');
  assert.match(envOn, /^SR_PAYMENTS_ESPLORA_URL='http:\/\/electrs:3000'$/m);
  assert.match(envOn, /^SR_PAYMENTS_ALLOW_PRIVATE_HOSTS='1'$/m);
  assert.match(envOn, /^COMPOSE_PROFILES=payments$/m);
});

test('SR_INSTALL_SKIP_PULL=1 skips the image pull and nothing else (RDY-OPS-001)', () => {
  const sb = sandbox();
  const r = run(sb, ['--yes'], { SR_INSTALL_SKIP_PULL: '1' });
  assert.equal(r.status, 0, r.out);
  const calls = readFileSync(sb.log, 'utf8').trim().split('\n');
  assert.ok(!calls.some((c) => / pull/.test(c)), `a pull ran:\n${calls.join('\n')}`);
  assert.ok(calls.some((c) => c.includes('sr-cli sr-server migrate')));
  assert.ok(calls.at(-1).endsWith(' sr-server preflight'));
  assert.match(r.out, /skipping the image pull/);
});

test('sr-cli preflight runs last, after sr-server is healthy, and only strict mode fails the install (RDY-OPS-004)', () => {
  const sb = sandbox();
  const r = run(sb, ['--yes'], { SR_INSTALL_PREFLIGHT_ARGS: '--insecure-tls' });
  assert.equal(r.status, 0, r.out);
  const calls = readFileSync(sb.log, 'utf8').trim().split('\n');
  const up = calls.findLastIndex((c) => / up -d$/.test(c));
  const health = calls.findIndex((c) => c.startsWith('inspect -f'));
  assert.ok(up !== -1 && health > up, calls.join('\n'));
  assert.match(calls.at(-1), /sr-server preflight --insecure-tls$/);
  assert.match(r.out, /==> sr-cli preflight/);

  // A failing preflight (the fake docker exits 1 for it) is reported; only strict mode fails the install.
  const failing = (box) =>
    writeFileSync(
      join(box.bin, 'docker'),
      readFileSync(join(box.bin, 'docker'), 'utf8').replace(
        'esac',
        '  *" preflight"*) echo "FAIL  mail" ; exit 1 ;;\nesac',
      ),
    );
  const warn = sandbox();
  failing(warn);
  const rw = run(warn, ['--yes']);
  assert.equal(rw.status, 0, rw.out);
  assert.match(rw.out, /preflight reported a FAIL row/);
  const strict = sandbox();
  failing(strict);
  const rs = run(strict, ['--yes'], { SR_INSTALL_PREFLIGHT: 'strict' });
  assert.equal(rs.status, 3, rs.out);

  const skip = sandbox();
  const rk = run(skip, ['--yes'], { SR_INSTALL_PREFLIGHT: 'skip' });
  assert.equal(rk.status, 0, rk.out);
  assert.ok(!readFileSync(skip.log, 'utf8').includes(' preflight'));

  const bad = sandbox();
  assert.notEqual(run(bad, ['--yes'], { SR_INSTALL_PREFLIGHT: 'maybe' }).status, 0);
});

test('the example image is refused before anything is written, with what to type (D-1041, SEC-AUD-568)', () => {
  const installer = readFileSync(join(root, 'deploy', 'install.sh'), 'utf8');
  const refused = /^PLACEHOLDER_IMAGE_OWNER=(\S+)$/m.exec(installer)?.[1];
  const example = envOf(join(root, 'deploy', 'compose', '.env.prod.example')).SR_IMAGE;
  assert.ok(refused && example, 'both found');
  // The example shows the refused owner and the real tag form, with its v.
  assert.ok(
    example.includes(refused),
    'the example no longer holds the placeholder the installer refuses',
  );
  assert.match(example, /:v\d+\.\d+\.\d+$/);
  for (const args of [['--yes'], ['--yes', '--no-start']]) {
    const sb = sandbox();
    const r = run(sb, args, { SR_IMAGE: example });
    assert.notEqual(r.status, 0, args.join(' '));
    assert.match(r.out, /SR_IMAGE is the example's placeholder/);
    assert.match(r.out, /ghcr\.io\/<owner>\/sideroom-server:<tag>/);
    assert.match(r.out, /leading v/);
    assert.ok(!existsSync(sb.env) && !existsSync(sb.garage), 'a secret file was written');
    assert.ok(!existsSync(sb.log) || !/ (pull|up|run)/.test(readFileSync(sb.log, 'utf8')));
  }
  // With no answer at all the default is the placeholder: refused too.
  const none = sandbox();
  const rn = run(none, ['--yes', '--no-start'], { SR_IMAGE: '' });
  assert.notEqual(rn.status, 0);
  assert.ok(!existsSync(none.env));
  // A real reference passes (the other cases use one).
  assert.equal(
    run(sandbox(), ['--yes', '--no-start'], { SR_IMAGE: 'ghcr.io/acme/sideroom-server:v1.2.3' })
      .status,
    0,
  );
});

test('on a rerun an SR_IMAGE given now is warned about, never applied or silent (D-1041)', () => {
  const sb = sandbox();
  assert.equal(run(sb, ['--yes', '--no-start']).status, 0);
  const env = readFileSync(sb.env, 'utf8');
  const other = 'ghcr.io/acme/sideroom-server:v9.9.9';
  const r = run(sb, ['--yes', '--no-start'], { SR_IMAGE: other });
  assert.equal(r.status, 0, r.out);
  assert.ok(r.out.includes('ghcr.io/acme/sideroom-server:1.2.3'), 'names the stored image');
  assert.ok(r.out.includes(other), 'names the one given now');
  assert.match(r.out, /edit SR_IMAGE in .*\.env\.prod and run this script again/);
  assert.equal(readFileSync(sb.env, 'utf8'), env, 'the stored file is not rewritten');
  // The same image, or none, says nothing.
  for (const same of [answers.SR_IMAGE, '']) {
    const q = run(sb, ['--yes', '--no-start'], { SR_IMAGE: same });
    assert.equal(q.status, 0, q.out);
    assert.doesNotMatch(q.out, /SR_IMAGE is/);
  }
  // A stored placeholder (a file from an older installer) stops the rerun with where to edit.
  writeFileSync(
    sb.env,
    env.replace(/^SR_IMAGE=.*$/m, "SR_IMAGE='ghcr.io/example-owner/sideroom-server:0.0.0'"),
  );
  const bad = run(sb, ['--yes', '--no-start'], { SR_IMAGE: '' });
  assert.notEqual(bad.status, 0);
  assert.match(bad.out, /placeholder/);
  assert.match(bad.out, /Edit SR_IMAGE in .*\.env\.prod/);
});

test('the generated Owner password and the link are shown as soon as the Owner exists, and again at the end (D-1041)', () => {
  const sb = sandbox();
  // The fake docker prints the link the way sr-cli does, and the starter-pack step fails after the Owner is made.
  // Both go into the second `case` of the stub (the first one only decides the Owner step's exit status).
  const dockerPath = join(sb.bin, 'docker');
  const link =
    'echo "One-time login link (valid 1h, single use):"; echo "https://chat.example.org/login/link?token=abc"';
  const withLines = (seed) =>
    readFileSync(dockerPath, 'utf8').replace(
      '  *"node id"*)',
      [
        `  *" bootstrap --owner-username"*) ${link} ;;`,
        `  *"package seed"*) ${seed} ;;`,
        '  *"node id"*)',
      ].join('\n'),
    );
  const original = readFileSync(dockerPath, 'utf8');
  writeFileSync(dockerPath, withLines('echo "seed failed" >&2; exit 1'));
  const packs = join(sb.dir, 'packages', 'starter-packs', 'side-room-words');
  mkdirSync(packs, { recursive: true });
  writeFileSync(join(packs, 'pack.json'), '{}');
  const first = run(sb, ['--yes'], { SR_OWNER_PASSWORD: '' });
  assert.notEqual(first.status, 0, 'the starter pack step fails after the Owner is made');
  const shown = /Owner password \(generated now[^)]*\): ([0-9a-f]{24})/.exec(first.out);
  assert.ok(shown, `the password was not printed at the Owner step:\n${first.out}`);
  assert.match(first.out, /login\/link\?token=abc/);
  assert.match(first.out, /good for one hour/);
  // Rerun: the Owner exists; neither is shown again, and the recovery is named.
  sb.stub('owner-done');
  writeFileSync(dockerPath, original);
  const again = run(sb, ['--yes'], { SR_OWNER_PASSWORD: '' });
  assert.equal(again.status, 0, again.out);
  assert.match(again.out, /cannot be shown again/);
  assert.match(again.out, /Forgot password/);
  assert.ok(!again.out.includes(shown[1]), 'the old password was printed again');
  // A password the operator gave is never echoed.
  const given = sandbox();
  const rg = run(given, ['--yes']);
  assert.equal(rg.status, 0, rg.out);
  assert.ok(!rg.out.includes(answers.SR_OWNER_PASSWORD));
  // The script keeps no log of its own: nothing is piped to tee or redirected to a file with the secret.
  const text = readFileSync(join(root, 'deploy', 'install.sh'), 'utf8');
  const code = text
    .split('\n')
    .filter((l) => !l.trim().startsWith('#'))
    .join('\n');
  assert.doesNotMatch(code, /\btee\b|\bexec\s+>/, 'install.sh must not tee or redirect its output');
});

// D-1046, SEC-AUD-576: the installer names the oldest Compose prod.yml works on and stops, before it writes anything,
// on an older one; a version string it cannot read is a warning, not a stop.
test('the Compose version is checked before anything is written (D-1046, SEC-AUD-576)', () => {
  const prod = readFileSync(join(root, 'deploy', 'compose', 'prod.yml'), 'utf8');
  const script = readFileSync(join(root, 'deploy', 'install.sh'), 'utf8');
  const major = /^MIN_COMPOSE_MAJOR=(\d+)$/m.exec(script)?.[1];
  const minor = /^MIN_COMPOSE_MINOR=(\d+)$/m.exec(script)?.[1];
  assert.equal(`${major}.${minor}`, '2.24', 'required: false on env_file needs 2.24');
  assert.match(prod, /required: false/, 'the key the minimum exists for');

  const old = ['2.23.3', 'v2.20.0', '1.29.2', '2.23.9-desktop.1', 'Docker Compose version v2.0.1'];
  for (const v of old) {
    const sb = sandbox();
    writeFileSync(join(sb.bin, 'compose-version'), `${v}\n`);
    const r = run(sb, ['--yes', '--no-start']);
    assert.notEqual(r.status, 0, v);
    assert.match(r.out, /Docker Compose \d+\.\d+ is too old: this stack needs 2\.24 or newer/, v);
    assert.match(r.out, /Nothing was written or started/, v);
    assert.ok(!existsSync(sb.env), `${v} left an env file behind`);
  }
  for (const v of [
    '2.24.0',
    'v2.24.5',
    '2.24.5-desktop.1',
    '2.29.1+azure',
    '3.0.0',
    'Docker Compose version v2.38.2',
  ]) {
    const sb = sandbox();
    writeFileSync(join(sb.bin, 'compose-version'), `${v}\n`);
    const r = run(sb, ['--yes', '--no-start'], {
      SR_SERVER_ENV_FILE: shPath(join(sb.dir, 'etc', 'server.env')),
      SR_SECRETS_DIR: shPath(join(sb.dir, 'etc', 'secrets')),
    });
    assert.equal(r.status, 0, `${v}: ${r.out}`);
    assert.doesNotMatch(r.out, /could not read the Docker Compose version/, v);
  }
});

test('a Compose version the installer cannot read is a warning, and the run goes on (D-1046)', () => {
  for (const v of ['dev', '', 'unknown-build']) {
    const sb = sandbox();
    writeFileSync(join(sb.bin, 'compose-version'), `${v}\n`);
    const r = run(sb, ['--yes', '--no-start'], {
      SR_SERVER_ENV_FILE: shPath(join(sb.dir, 'etc', 'server.env')),
      SR_SECRETS_DIR: shPath(join(sb.dir, 'etc', 'secrets')),
    });
    assert.equal(r.status, 0, `'${v}': ${r.out}`);
    assert.match(r.out, /warning: could not read the Docker Compose version/, v);
    assert.match(r.out, /needs Compose 2\.24 or newer/, v);
    assert.ok(existsSync(sb.env), `'${v}' wrote nothing`);
  }
});

test('an empty server env file (0600) and the secrets folder (0700) are created, and never touched when they exist (D-1046, SEC-AUD-576)', () => {
  const sb = sandbox();
  const serverEnv = join(sb.dir, 'etc', 'server.env');
  const secrets = join(sb.dir, 'etc', 'secrets');
  const paths = { SR_SERVER_ENV_FILE: shPath(serverEnv), SR_SECRETS_DIR: shPath(secrets) };
  const r = run(sb, ['--yes', '--no-start'], paths);
  assert.equal(r.status, 0, r.out);
  assert.ok(existsSync(serverEnv) && statSync(serverEnv).isFile(), 'the env file was created');
  assert.equal(readFileSync(serverEnv, 'utf8'), '', 'it is empty');
  assert.ok(statSync(secrets).isDirectory(), 'the folder was created');
  if (process.platform !== 'win32') {
    assert.equal(statSync(serverEnv).mode & 0o777, 0o600);
    assert.equal(statSync(secrets).mode & 0o777, 0o700);
  }
  assert.match(r.out, /created the empty server env file/);
  assert.match(r.out, /created the secrets folder/);

  // What an operator put there stays: a rerun neither empties the file nor says it created anything.
  writeFileSync(serverEnv, 'SR_LOG__FILTER=debug\n');
  writeFileSync(join(secrets, 'discord_client_secret'), 'typed-by-the-owner\n');
  const again = run(sb, ['--yes', '--no-start'], paths);
  assert.equal(again.status, 0, again.out);
  assert.equal(readFileSync(serverEnv, 'utf8'), 'SR_LOG__FILTER=debug\n');
  assert.equal(
    readFileSync(join(secrets, 'discord_client_secret'), 'utf8'),
    'typed-by-the-owner\n',
  );
  assert.doesNotMatch(again.out, /created the (empty server env file|secrets folder)/);

  // Run as someone who cannot create them (a path under a plain file): a warning that names the fix, not a stop.
  const blocked = sandbox();
  const plain = join(blocked.dir, 'plain-file');
  writeFileSync(plain, 'x');
  const b = run(blocked, ['--yes', '--no-start'], {
    SR_SERVER_ENV_FILE: shPath(join(plain, 'server.env')),
    SR_SECRETS_DIR: shPath(join(plain, 'secrets')),
  });
  assert.equal(b.status, 0, b.out);
  assert.match(b.out, /warning: could not create .*server\.env/);
  assert.match(b.out, /warning: could not create .*secrets/);
  assert.ok(existsSync(blocked.env), 'the install itself went on');

  // A relative path is refused, naming the variable.
  const rel = run(sandbox(), ['--yes', '--no-start'], { SR_SECRETS_DIR: 'secrets' });
  assert.notEqual(rel.status, 0);
  assert.match(rel.out, /SR_SECRETS_DIR must be an absolute path/);
});

test('on a rerun the paths stored in .env.prod count, and the secrets mount in prod.yml is read-only (D-1046)', () => {
  const sb = sandbox();
  const serverEnv = join(sb.dir, 'kept', 'server.env');
  const secrets = join(sb.dir, 'kept', 'secrets');
  assert.equal(
    run(sb, ['--yes', '--no-start'], { SR_SERVER_ENV_FILE: '', SR_SECRETS_DIR: '' }).status,
    0,
  );
  const stored = readFileSync(sb.env, 'utf8');
  writeFileSync(
    sb.env,
    `${stored}SR_SERVER_ENV_FILE='${shPath(serverEnv)}'\nSR_SECRETS_DIR='${shPath(secrets)}'\n`,
  );
  const r = run(sb, ['--yes', '--no-start']);
  assert.equal(r.status, 0, r.out);
  assert.ok(existsSync(serverEnv), 'the env file named in .env.prod was created');
  assert.ok(existsSync(secrets), 'the folder named in .env.prod was created');

  const prod = readFileSync(join(root, 'deploy', 'compose', 'prod.yml'), 'utf8');
  assert.match(
    prod,
    /^\s+- \$\{SR_SECRETS_DIR:-\/etc\/sideroom\/secrets\}:\/run\/secrets\/sideroom:ro$/m,
  );
});
