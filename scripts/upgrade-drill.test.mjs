// RDY-OPS-007: the stages of the upgrade drill in deploy/scripts/prod-smoke.sh, run against a throwaway git repository
// holding a "previous" and a "new" copy of deploy/, with the recording fake `docker` of install.test.mjs on PATH.
// Nothing here needs Docker, a server or a network, so this proves the ORDER and the ARGUMENTS of the stages (which
// commit is the previous release, which file is installed, that the backup precedes the image switch, that migrations
// run before the restart, that a rollback restores into a fresh database before the old image starts). The proof that
// the stack really upgrades is the `upgrade` job of .github/workflows/prod-smoke.yml on the pull request.
// Run with: node --test "scripts/*.test.mjs"
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  chmodSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const posix = (p) => p.replaceAll('\\', '/');
const shPath = (p) => posix(p).replace(/^([A-Za-z]):/, (_, d) => `/${d.toLowerCase()}`);

const git = (cwd, ...args) => {
  const r = spawnSync(
    'git',
    ['-c', 'user.name=t', '-c', 'user.email=t@example.test', '-c', 'commit.gpgsign=false', ...args],
    { cwd, encoding: 'utf8' },
  );
  assert.equal(r.status, 0, `git ${args.join(' ')}: ${r.stderr}`);
  return r.stdout.trim();
};

/** A repository with the real deploy/ files committed as "previous" and a changed copy as "head". */
function repoWithTwoReleases() {
  const dir = mkdtempSync(join(tmpdir(), 'sr-upgrade-'));
  git(dir, 'init', '-q', '-b', 'main');
  cpSync(join(root, 'deploy'), join(dir, 'deploy'), { recursive: true });
  mkdirSync(join(dir, 'packages', 'starter-packs'), { recursive: true });
  writeFileSync(join(dir, 'packages', 'starter-packs', 'README'), 'packs\n');
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', 'previous release');
  const previous = git(dir, 'rev-parse', 'HEAD');
  // The "new release" adds an env key (with a usable default) and edits a compose comment.
  const example = join(dir, 'deploy', 'compose', '.env.prod.example');
  writeFileSync(example, `${readFileSync(example, 'utf8')}SR_NEW_OPTION=on\n`);
  const prod = join(dir, 'deploy', 'compose', 'prod.yml');
  writeFileSync(prod, `${readFileSync(prod, 'utf8')}# the new release\n`);
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', 'new release');
  const head = git(dir, 'rev-parse', 'HEAD');
  return { dir, previous, head };
}

function fakeDocker(dir) {
  const bin = join(dir, 'bin');
  mkdirSync(bin, { recursive: true });
  const log = join(dir, 'docker.log');
  writeFileSync(
    join(bin, 'docker'),
    `#!/bin/sh
echo "$*" >> "${posix(log)}"
case "$*" in
  *"node id"*) echo "abc123def@127.0.0.1:3901" ;;
  *"/garage status"*) echo "abc123def  host  dc1  1G" ;;
  *"ps -q sr-server"*) echo "c0ffee" ;;
  "inspect -f"*) echo "healthy" ;;
  *"ls -1 /var/backups/sideroom/db"*) echo "/var/backups/sideroom/db/sideroom-20261025T000000Z.dump" ;;
  *"_sqlx_migrations"*) echo "7" ;;
esac
exit 0
`,
  );
  chmodSync(join(bin, 'docker'), 0o755);
  return { bin, log };
}

function smoke(dir, fake, stage, extra = {}, args = []) {
  const work = join(dir, 'work');
  mkdirSync(work, { recursive: true });
  const web = join(dir, 'web');
  mkdirSync(web, { recursive: true });
  writeFileSync(join(web, 'index.html'), '<div id="root"></div>');
  const r = spawnSync(
    'sh',
    [shPath(join(dir, 'deploy', 'scripts', 'prod-smoke.sh')), stage, ...args],
    {
      cwd: dir,
      env: {
        ...process.env,
        PATH: `${shPath(fake.bin)}:${process.env.PATH}`,
        SMOKE_WORK: shPath(work),
        SMOKE_WEB_DIR: shPath(web),
        SMOKE_SKIP_HOSTS: '1',
        SMOKE_PREV_IMAGE: 'img:prev',
        SMOKE_IMAGE: 'img:head',
        ...extra,
      },
      encoding: 'utf8',
      input: '',
    },
  );
  return { ...r, out: `${r.stdout}${r.stderr}`, work };
}

const calls = (fake) => readFileSync(fake.log, 'utf8').trim().split('\n');
const indexOf = (list, needle, from = 0) => {
  const i = list.findIndex((c, n) => n >= from && c.includes(needle));
  assert.notEqual(
    i,
    -1,
    `no docker call containing "${needle}" after #${from}\n${list.join('\n')}`,
  );
  return i;
};
const envValue = (path, key) =>
  new RegExp(`^${key}=(.*)$`, 'm').exec(readFileSync(path, 'utf8'))?.[1];

test('base-sha: the merge base on a branch, the commit before on the target, the merge parent on a pull request', () => {
  const dir = mkdtempSync(join(tmpdir(), 'sr-basesha-'));
  git(dir, 'init', '-q', '-b', 'main');
  mkdirSync(join(dir, 'deploy', 'scripts'), { recursive: true });
  for (const f of ['prod-smoke.sh', 'lib.sh']) {
    cpSync(join(root, 'deploy', 'scripts', f), join(dir, 'deploy', 'scripts', f));
  }
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', 'c1');
  const c1 = git(dir, 'rev-parse', 'HEAD');
  writeFileSync(join(dir, 'a.txt'), 'a');
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', 'c2');
  const c2 = git(dir, 'rev-parse', 'HEAD');
  git(dir, 'checkout', '-q', '-b', 'feature', c1);
  writeFileSync(join(dir, 'f.txt'), 'f');
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', 'f1');
  const f1 = git(dir, 'rev-parse', 'HEAD');
  git(dir, 'update-ref', 'refs/remotes/origin/main', c2);
  const base = (event = 'workflow_dispatch', baseRef = '') => {
    const r = spawnSync(
      'sh',
      [shPath(join(dir, 'deploy', 'scripts', 'prod-smoke.sh')), 'base-sha'],
      {
        cwd: dir,
        env: { ...process.env, GITHUB_EVENT_NAME: event, GITHUB_BASE_REF: baseRef },
        encoding: 'utf8',
      },
    );
    return { ...r, sha: r.stdout.trim() };
  };

  // A branch that forked from c1 while main moved on to c2: the previous release is the fork point.
  let r = base();
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.sha, c1);

  // A pull request run checks out a merge commit; its second parent is the branch head.
  git(dir, 'checkout', '-q', '--detach', c2);
  git(dir, 'merge', '-q', '--no-ff', '-m', 'merge', f1);
  r = base('pull_request', 'main');
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.sha, c1, 'the merge base of the branch head with main, not main itself');

  // A push to main (a merge commit whose second parent is already part of main): the commit before it.
  git(dir, 'update-ref', 'refs/remotes/origin/main', 'HEAD');
  const merge = git(dir, 'rev-parse', 'HEAD');
  r = base('push');
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.sha, c2, `first parent of ${merge}`);

  // No parent at all: a clear error, not an empty sha.
  git(dir, 'checkout', '-q', '--orphan', 'orphan');
  git(dir, 'commit', '-q', '--allow-empty', '-m', 'root');
  git(dir, 'update-ref', 'refs/remotes/origin/main', 'HEAD');
  r = base('push');
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /no previous commit/);
  assert.equal(r.sha, '');
});

test('the drill installs the previous release, backs up, upgrades to the new files and image, then rolls back (RDY-OPS-007)', () => {
  const { dir, previous, head } = repoWithTwoReleases();
  const fake = fakeDocker(dir);
  const env = { SMOKE_BASE_SHA: previous };
  const live = join(dir, 'work', 'live', 'deploy', 'compose');
  const envFile = join(live, '.env.prod');

  // upgrade-prepare unpacks the PREVIOUS release's deploy files (no new key yet).
  let r = smoke(dir, fake, 'upgrade-prepare', env);
  assert.equal(r.status, 0, r.out);
  assert.ok(existsSync(join(live, 'prod.yml')));
  assert.doesNotMatch(readFileSync(join(live, '.env.prod.example'), 'utf8'), /SR_NEW_OPTION/);
  assert.doesNotMatch(readFileSync(join(live, 'prod.yml'), 'utf8'), /the new release/);

  // upgrade-install: install.sh --yes of that copy with the previous image. (The script checks the file mode of
  // .env.prod with GNU stat, which a Windows checkout cannot satisfy, so there install.sh is run the way the stage
  // runs it and only the rest is checked.)
  if (process.platform === 'win32') {
    const answers = {
      SR_DOMAIN: 'smoke.localhost',
      SR_IMAGE: 'img:prev',
      SR_WEB_DIR: shPath(join(dir, 'web')),
      SMTP_HOST: 'mail.example.org',
      SR_OWNER_EMAIL: 'owner@example.test',
      SR_OWNER_PASSWORD: 'correct horse battery staple',
      SR_INGEST_DIR: shPath(join(dir, 'ingest')),
      SR_GARAGE_CAPACITY: '1G',
      SR_INSTALL_SKIP_PULL: '1',
      SR_INSTALL_PREFLIGHT: 'skip',
    };
    r = spawnSync('sh', [shPath(join(live, '..', 'install.sh')), '--yes'], {
      env: { ...process.env, PATH: `${shPath(fake.bin)}:${process.env.PATH}`, ...answers },
      encoding: 'utf8',
      input: '',
    });
    assert.equal(r.status, 0, `${r.stdout}${r.stderr}`);
  } else {
    r = smoke(dir, fake, 'upgrade-install', env);
    assert.equal(r.status, 0, r.out);
    assert.match(r.out, /PREVIOUS release with image img:prev/);
  }
  assert.equal(envValue(envFile, 'SR_IMAGE'), 'img:prev');
  const afterInstall = calls(fake);
  assert.ok(afterInstall.some((c) => c.includes('sr-cli sr-server migrate')));
  assert.ok(afterInstall.some((c) => c.includes('sr-cli sr-server bootstrap')));

  // upgrade-backup: the command comes out of UPGRADE.md, runs in the postgres container, and is followed by the drill.
  const before = calls(fake).length;
  r = smoke(dir, fake, 'upgrade-backup', env);
  assert.equal(r.status, 0, r.out);
  const backup = calls(fake).slice(before);
  const b = indexOf(
    backup,
    'exec -T -u postgres postgres sh -c SR_DATABASE_URL=postgres://sideroom@/sideroom SR_BACKUP_DIR=/var/backups/sideroom SR_BACKUP_DIR_MODE=750 sh /opt/sideroom/scripts/backup.sh',
  );
  assert.ok(indexOf(backup, 'restore-drill.sh') > b, 'restore-drill.sh follows the backup');
  assert.equal(
    readFileSync(join(dir, 'work', 'upgrade-dump'), 'utf8').trim(),
    '/var/backups/sideroom/db/sideroom-20261025T000000Z.dump',
  );
  assert.equal(readFileSync(join(dir, 'work', 'upgrade-migrations'), 'utf8').trim(), '7');

  // upgrade-apply: new files first, the image switch, migrations on the new image BEFORE the restart, nothing pulled.
  const beforeApply = calls(fake).length;
  r = smoke(dir, fake, 'upgrade-apply', env);
  assert.equal(r.status, 0, r.out);
  assert.match(
    readFileSync(join(live, 'prod.yml'), 'utf8'),
    /the new release/,
    'the new deploy files are in place',
  );
  assert.equal(
    envValue(envFile, 'SR_NEW_OPTION'),
    'on',
    'a key the release adds is taken from the example',
  );
  assert.equal(envValue(envFile, 'SR_IMAGE'), 'img:head');
  assert.ok(
    readFileSync(envFile, 'utf8').includes('POSTGRES_PASSWORD='),
    '.env.prod kept its secrets',
  );
  const apply = calls(fake).slice(beforeApply);
  const dry = indexOf(apply, 'sr-cli sr-server upgrade --dry-run');
  const migrate = indexOf(apply, 'sr-cli sr-server upgrade --no-backup');
  const restart = indexOf(apply, 'up -d sr-server');
  assert.ok(dry < migrate && migrate < restart, apply.join('\n'));
  assert.ok(
    apply.every((c) => !/ pull/.test(c)),
    'the image is loaded, not pulled',
  );

  // upgrade-rollback: stop, fresh database, restore, previous files and image, start.
  const beforeRollback = calls(fake).length;
  r = smoke(dir, fake, 'upgrade-rollback', env);
  assert.equal(r.status, 0, r.out);
  const back = calls(fake).slice(beforeRollback);
  const stop = indexOf(back, 'stop sr-server');
  const drop = indexOf(back, 'drop database sideroom with (force)', stop);
  const create = indexOf(back, 'create database sideroom', drop + 1);
  const restore = indexOf(
    back,
    "restore.sh '/var/backups/sideroom/db/sideroom-20261025T000000Z.dump' postgres://sideroom@/sideroom",
    create,
  );
  const start = indexOf(back, 'up -d sr-server', restore);
  assert.ok(stop < drop && drop < create && create < restore && restore < start, back.join('\n'));
  assert.ok(
    back.every((c) => !c.includes('SR_RESTORE_FORCE')),
    'a fresh database, not --force over the upgraded one',
  );
  assert.equal(envValue(envFile, 'SR_IMAGE'), 'img:prev');
  assert.doesNotMatch(
    readFileSync(join(live, 'prod.yml'), 'utf8'),
    /the new release/,
    'the previous deploy files are back',
  );
  assert.ok(head !== previous);
});

test('a rollback that restores a different schema than the backup had is refused (RDY-OPS-007)', () => {
  const { dir, previous } = repoWithTwoReleases();
  const fake = fakeDocker(dir);
  const env = { SMOKE_BASE_SHA: previous };
  assert.equal(smoke(dir, fake, 'upgrade-prepare', env).status, 0);
  mkdirSync(join(dir, 'work'), { recursive: true });
  writeFileSync(join(dir, 'work', 'upgrade-dump'), '/var/backups/sideroom/db/x.dump\n');
  writeFileSync(join(dir, 'work', 'upgrade-migrations'), '6\n'); // the fake database reports 7
  writeFileSync(join(dir, 'work', 'live', 'deploy', 'compose', '.env.prod'), 'SR_IMAGE=img:head\n');
  const r = smoke(dir, fake, 'upgrade-rollback', env);
  assert.notEqual(r.status, 0);
  assert.match(r.out, /restored database has 7 migrations, the backup had 6/);
  assert.ok(
    calls(fake).every((c) => !c.includes('up -d')),
    'the old image was not started on a wrong database',
  );
});

test('a release that needs an operator secret in .env.prod stops the upgrade with a clear message', () => {
  const { dir, previous } = repoWithTwoReleases();
  const fake = fakeDocker(dir);
  const example = join(dir, 'deploy', 'compose', '.env.prod.example');
  writeFileSync(example, `${readFileSync(example, 'utf8')}SR_NEW_SECRET=CHANGE_ME\n`);
  git(dir, 'add', '-A');
  git(dir, 'commit', '-q', '-m', 'needs a secret');
  const env = { SMOKE_BASE_SHA: previous };
  assert.equal(smoke(dir, fake, 'upgrade-prepare', env).status, 0);
  const live = join(dir, 'work', 'live', 'deploy', 'compose');
  writeFileSync(
    join(live, '.env.prod'),
    readFileSync(join(live, '.env.prod.example'), 'utf8').replaceAll('CHANGE_ME', 'x'),
  );
  const r = smoke(dir, fake, 'upgrade-apply', env);
  assert.notEqual(r.status, 0);
  assert.match(r.out, /SR_NEW_SECRET/);
  assert.ok(
    !existsSync(fake.log) || calls(fake).every((c) => !c.includes('upgrade')),
    'no migration ran',
  );
});
