// SEC-AUD-211: the one packing script of the release and the dry run, run against small fake dists built in a
// temporary folder inside the worktree.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, describe, it } from 'node:test';
import { checkDist, packWebRelease } from './pack-web-release.mjs';

const repo = join(dirname(fileURLToPath(import.meta.url)), '..');
const scratch = mkdtempSync(join(repo, '.pack-web-test-'));
after(() => rmSync(scratch, { recursive: true, force: true }));

/** A dist that passes; `edit(files)` changes the file map (path under dist -> text, or null to remove). */
function dist(edit = () => {}) {
  const files = {
    'app.html': '<html>app</html>',
    'app/csp.txt': "default-src 'self'",
    'app/assets/app-1a2b3c.js': 'console.log(1)',
    'index.html': '<html>landing</html>',
    'sw.js': "const CACHE = 'sr-static-abc123';",
    'manifest.webmanifest': '{}',
    'offline.html': '<html>offline</html>',
  };
  edit(files);
  const root = mkdtempSync(join(scratch, 'dist-'));
  for (const [f, text] of Object.entries(files)) {
    if (text === null) continue;
    mkdirSync(dirname(join(root, f)), { recursive: true });
    writeFileSync(join(root, f), text);
  }
  return root;
}

const out = () => join(scratch, `out-${Math.random().toString(36).slice(2)}`, 'web.tar.gz');

describe('pack-web-release', () => {
  it('packs a good dist as web/ and the archive unpacks to <dir>/web/app.html', () => {
    const o = out();
    const r = packWebRelease(dist(), o, {});
    assert.equal(r.out, o);
    assert.ok(existsSync(o));
    const unpack = mkdtempSync(join(scratch, 'unpack-'));
    execFileSync('tar', ['-xzf', 'web.tar.gz', '-C', unpack.replace(/\\/g, '/')], {
      cwd: dirname(o),
    });
    assert.equal(readFileSync(join(unpack, 'web', 'app.html'), 'utf8'), '<html>app</html>');
    assert.equal(readFileSync(join(unpack, 'web', 'app', 'csp.txt'), 'utf8'), "default-src 'self'");
    assert.ok(existsSync(join(unpack, 'web', 'app', 'assets', 'app-1a2b3c.js')));
    // Nothing of the staging is left next to the archive.
    assert.deepEqual(
      execFileSync('tar', ['-tzf', 'web.tar.gz'], { cwd: dirname(o), encoding: 'utf8' })
        .split(/\r?\n/)
        .filter((l) => l && !l.startsWith('web')),
      [],
    );
  });

  it('refuses when VITE_SR_E2E is set, and writes nothing', () => {
    const o = out();
    assert.throws(() => packWebRelease(dist(), o, { VITE_SR_E2E: '1' }), /VITE_SR_E2E is set/);
    assert.ok(!existsSync(o));
    assert.deepEqual(checkDist(dist(), {}), []);
  });

  it('refuses a dist that holds the e2e voice opt-out marker', () => {
    const d = dist((f) => {
      f['app/assets/voice-9f.js'] = 'localStorage.getItem("sr.e2e.noVoiceE2ee")';
    });
    assert.throws(() => packWebRelease(d, out(), {}), /e2e voice opt-out.*voice-9f\.js/);
  });

  it('refuses an unstamped service worker, or one without its cache name', () => {
    const raw = dist((f) => {
      f['sw.js'] = "const CACHE = '__SR_BUILD__';";
    });
    assert.throws(() => packWebRelease(raw, out(), {}), /sw\.js still holds __SR_BUILD__/);
    const bare = dist((f) => {
      f['sw.js'] = 'self.addEventListener("fetch", () => {});';
    });
    assert.throws(() => packWebRelease(bare, out(), {}), /no sr-static- cache name/);
  });

  it('refuses a dist that lacks one of the five files, naming it', () => {
    for (const f of ['app.html', 'app/csp.txt', 'sw.js', 'manifest.webmanifest', 'offline.html']) {
      const d = dist((files) => {
        files[f] = null;
      });
      assert.throws(
        () => packWebRelease(d, out(), {}),
        (e) => e.message.includes(`${f} is missing`),
        f,
      );
    }
  });

  it('refuses a folder that is not there, and says every reason at once', () => {
    assert.throws(() => packWebRelease(join(scratch, 'nope'), out(), {}), /is not a directory/);
    const d = dist((f) => {
      f['app.html'] = null;
      f['offline.html'] = null;
    });
    assert.throws(
      () => packWebRelease(d, out(), {}),
      (e) => /app\.html is missing/.test(e.message) && /offline\.html is missing/.test(e.message),
    );
  });

  it('the command line exits 0 on a good dist and 1 with the reason on a bad one', () => {
    const script = join(repo, 'scripts', 'pack-web-release.mjs');
    const o = out();
    execFileSync('node', [script, dist(), o], { env: { ...process.env, VITE_SR_E2E: '' } });
    assert.ok(existsSync(o));
    const bad = dist((f) => {
      f['sw.js'] = null;
    });
    assert.throws(
      () =>
        execFileSync('node', [script, bad, out()], {
          env: { ...process.env, VITE_SR_E2E: '' },
          stdio: 'pipe',
        }),
      (e) => e.status === 1 && /sw\.js is missing/.test(String(e.stderr)),
    );
    assert.throws(
      () => execFileSync('node', [script], { stdio: 'pipe' }),
      (e) => e.status === 2,
    );
  });
});
