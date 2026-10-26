// RDY-OPS-008: deploy/scripts/backup.sh removes a stale `*.dump.partial` before it starts and its retention never
// counts one. Runs the real script with fake `pg_dump` and `psql` on PATH. Nothing here needs Postgres or Docker.
// Run with: node --test "scripts/*.test.mjs"
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const posix = (p) => p.replaceAll('\\', '/');

function sandbox() {
  const dir = mkdtempSync(join(tmpdir(), 'sr-backup-'));
  const bin = join(dir, 'bin');
  mkdirSync(bin);
  // pg_dump writes the file named by --file=...; psql answers the migration count.
  writeFileSync(
    join(bin, 'pg_dump'),
    '#!/bin/sh\nfor a in "$@"; do case $a in --file=*) echo dump > "${a#--file=}" ;; esac; done\n',
  );
  writeFileSync(join(bin, 'psql'), '#!/bin/sh\necho 42\n');
  chmodSync(join(bin, 'pg_dump'), 0o755);
  chmodSync(join(bin, 'psql'), 0o755);
  const backups = join(dir, 'backups');
  mkdirSync(join(backups, 'db'), { recursive: true });
  return { dir, bin, backups, db: join(backups, 'db') };
}

function run(sb, extra = {}) {
  const r = spawnSync('sh', [posix(join(root, 'deploy', 'scripts', 'backup.sh'))], {
    env: {
      ...process.env,
      PATH: `${posix(sb.bin)}:${process.env.PATH}`,
      SR_DATABASE_URL: 'postgres://u:p@localhost:5432/sideroom',
      SR_BACKUP_DIR: posix(sb.backups),
      ...extra,
    },
    encoding: 'utf8',
  });
  return { ...r, out: `${r.stdout}${r.stderr}` };
}

const touchAgo = (file, minutes) => {
  writeFileSync(file, 'x');
  const t = new Date(Date.now() - minutes * 60_000);
  utimesSync(file, t, t);
};

test('a partial dump older than an hour is removed, a younger one is left alone (RDY-OPS-008)', () => {
  const sb = sandbox();
  const stale = join(sb.db, 'sideroom-20260101T000000Z.dump.partial');
  const fresh = join(sb.db, 'sideroom-20260101T010000Z.dump.partial');
  touchAgo(stale, 120);
  touchAgo(fresh, 10);
  const r = run(sb);
  assert.equal(r.status, 0, r.out);
  assert.ok(!existsSync(stale), 'the stale partial is still there');
  assert.ok(existsSync(fresh), 'a partial younger than an hour must stay (a dump may be running)');
  assert.match(r.out, /removing the stale partial dump sideroom-20260101T000000Z\.dump\.partial/);
  const names = readdirSync(sb.db);
  assert.equal(names.filter((n) => /^sideroom-.*\.dump$/.test(n)).length, 1, names.join());
});

test('retention never counts a partial dump and never deletes one it should keep (RDY-OPS-008)', () => {
  const sb = sandbox();
  // Three finished dumps on three days with a keep of 2 daily: the oldest dump goes, partials do not count as one.
  for (const [name, age] of [
    ['sideroom-20260101T030000Z.dump', 3 * 24 * 60],
    ['sideroom-20260102T030000Z.dump', 2 * 24 * 60],
    ['sideroom-20260103T030000Z.dump', 24 * 60],
  ]) {
    touchAgo(join(sb.db, name), age);
    touchAgo(join(sb.db, `${name}.json`), age);
  }
  const young = join(sb.db, 'sideroom-20260104T000000Z.dump.partial');
  touchAgo(young, 5);
  const r = run(sb, { SR_BACKUP_KEEP: '2,0,0' });
  assert.equal(r.status, 0, r.out);
  const names = readdirSync(sb.db);
  // The new dump and the newest old one are the 2 daily; the older two are pruned with their manifests.
  assert.ok(!names.includes('sideroom-20260101T030000Z.dump'), names.join());
  assert.ok(!names.includes('sideroom-20260102T030000Z.dump'), names.join());
  assert.ok(names.includes('sideroom-20260103T030000Z.dump'), names.join());
  assert.ok(names.includes('sideroom-20260104T000000Z.dump.partial'), 'the young partial stays');
  assert.doesNotMatch(r.out, /pruning .*partial/);
});
