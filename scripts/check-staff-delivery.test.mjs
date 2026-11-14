// SEC-AUD-170: the staff delivery check, run against small fixture dists: one that passes and one failing fixture per
// rule (a..e).
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { after, describe, it } from 'node:test';
import { checkStaffDelivery } from './check-staff-delivery.mjs';

const MARK = ['m-one', 'm-two'];
const roots = [];

/** A dist that passes; `edit(files)` changes the file map (path under dist -> text, or null to remove). */
function dist(edit = () => {}) {
  const files = {
    'app.html': '<script type="module" src="/app/app-AAAA1111.js"></script>',
    'app/app-AAAA1111.js':
      'import "./vendor-BBBB2222.js"; const x = import("./staff/Zk3_-9aB.js");',
    'app/vendor-BBBB2222.js': 'export const v = 1;',
    'app/staff/Zk3_-9aB.js': 'export const a = "m-one";',
    'app/staff/Qq1WwEe2.css': '.x{}',
    'app/staff-chunks.json': JSON.stringify({
      version: 1,
      memberModulesStaffLooking: ['src/lounge/shell/platformStaff.ts'],
      memberModulesUnderAdmin: ['src/admin/reauth.ts'],
    }),
  };
  edit(files);
  const root = mkdtempSync(join(tmpdir(), 'sr-staff-'));
  roots.push(root);
  for (const [f, text] of Object.entries(files)) {
    if (text === null) continue;
    mkdirSync(dirname(join(root, f)), { recursive: true });
    writeFileSync(join(root, f), text);
  }
  return root;
}
const run = (edit) => checkStaffDelivery(dist(edit), MARK);
const rules = (r) => r.failures.map((f) => f[0]);

after(() => roots.forEach((r) => rmSync(r, { recursive: true, force: true })));

describe('check-staff-delivery', () => {
  it('passes a clean dist and lists the staff-looking member modules as a report', () => {
    const r = run();
    assert.deepEqual(r.failures, []);
    assert.match(r.report.join('\n'), /platformStaff/);
  });
  it('a: fails when staff/ is empty or missing', () => {
    const r = run((f) => {
      delete f['app/staff/Zk3_-9aB.js'];
      delete f['app/staff/Qq1WwEe2.css'];
    });
    assert.ok(rules(r).includes('a'));
  });
  it('b: fails when app.html references /app/staff/', () => {
    const r = run((f) => {
      f['app.html'] += '<link rel="modulepreload" href="/app/staff/Zk3_-9aB.js">';
    });
    assert.deepEqual(rules(r), ['b']);
  });
  it('c: fails when a member chunk statically imports a staff file, but a dynamic import is fine', () => {
    const r = run((f) => {
      f['app/vendor-BBBB2222.js'] = 'import { a } from "./staff/Zk3_-9aB.js"; export const v = a;';
    });
    assert.deepEqual(rules(r), ['c']);
  });
  it('d: fails when a file outside staff/ holds a staff marker', () => {
    const r = run((f) => {
      f['app/vendor-BBBB2222.js'] = 'export const v = "m-two";';
    });
    assert.deepEqual(rules(r), ['d']);
  });
  it('d: fails when no marker is inside staff/ (the markers are stale)', () => {
    const r = run((f) => {
      f['app/staff/Zk3_-9aB.js'] = 'export const a = 1;';
    });
    assert.deepEqual(rules(r), ['d']);
  });
  it('e: fails when a staff file has a name', () => {
    const r = run((f) => {
      f['app/staff/AdminApp-Zk3_-9aB.js'] = 'export {}';
    });
    assert.deepEqual(rules(r), ['e']);
  });
  it('e: fails on a source map or a nested file under staff/', () => {
    const r = run((f) => {
      f['app/staff/Zk3_-9aB.js.map'] = '{}';
      f['app/staff/sub/Qq1WwEe2.js'] = 'x';
    });
    assert.deepEqual(rules(r), ['e', 'e']);
  });
});
