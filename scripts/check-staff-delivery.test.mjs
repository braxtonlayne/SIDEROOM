// SEC-AUD-170: the staff delivery check, run against small fixture dists: one that passes and one failing fixture per
// rule (a..e).
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { after, describe, it } from 'node:test';
import { checkStaffDelivery, staffCatalogNeedles } from './check-staff-delivery.mjs';

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
    // The build report sits beside dist/, not in it (SEC-AUD-271).
    '../staff-chunks.json': JSON.stringify({
      version: 1,
      memberModulesStaffLooking: ['src/lounge/shell/platformStaff.ts'],
      memberModulesUnderAdmin: ['src/admin/reauth.ts'],
    }),
  };
  edit(files);
  const root = mkdtempSync(join(tmpdir(), 'sr-staff-'));
  roots.push(root);
  const out = join(root, 'dist');
  for (const [f, text] of Object.entries(files)) {
    if (text === null) continue;
    mkdirSync(dirname(join(out, f)), { recursive: true });
    writeFileSync(join(out, f), text);
  }
  return out;
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
  it('h: fails when the build report is inside dist/app/, where every signed-in member can fetch it', () => {
    // Before SEC-AUD-271 the build emitted it there: the names and module paths of every staff chunk.
    const r = run((f) => {
      f['app/staff-chunks.json'] = f['../staff-chunks.json'];
    });
    assert.deepEqual(rules(r), ['h']);
    assert.match(r.failures[0], /app\/staff-chunks\.json/);
    // Nested or renamed with a suffix: still the report.
    const nested = run((f) => {
      f['app/x/staff-chunks.report.json'] = '{}';
    });
    assert.deepEqual(rules(nested), ['h']);
  });
  it('the build report is read from beside dist/, and a missing one is only said, not a failure', () => {
    const r = run((f) => {
      delete f['../staff-chunks.json'];
    });
    assert.deepEqual(r.failures, []);
    assert.match(r.report.join('\n'), /build report .* is missing/);
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

/** Fragments: a staff text (long, with an apostrophe), a short one, one a member fragment also holds. */
function messages() {
  const root = mkdtempSync(join(tmpdir(), 'sr-staffmsg-'));
  roots.push(root);
  const frag = (o) =>
    `export default {\n${Object.entries(o)
      .map(([k, v]) => `  '${k}': ${JSON.stringify(v)},\n`)
      .join('')}} satisfies Record<string, string>;\n`;
  mkdirSync(join(root, 'en'), { recursive: true });
  mkdirSync(join(root, 'en-staff'), { recursive: true });
  writeFileSync(join(root, 'en', 'a.ts'), frag({ 'member.shared': 'Save your changes now' }));
  writeFileSync(
    join(root, 'en-staff', 'b.ts'),
    frag({
      'admin.queue.title': "Review the reporter's queue before closing",
      'admin.queue.short': 'Close it',
      'admin.queue.shared': 'Save your changes now',
    }),
  );
  return root;
}
const catalogRun = (edit) => {
  const dir = messages();
  const dst = dist((f) => {
    f['app/staff/Zk3_-9aB.js'] =
      'export const a = "m-one"; export const q = "Review the reporter\\"s queue before closing";';
    edit?.(f);
  });
  return checkStaffDelivery(dst, MARK, staffCatalogNeedles(dir));
};

describe('check-staff-delivery: the staff message catalog (SEC-AUD-187, D-663)', () => {
  it('needles: long staff texts only, and not a text a member fragment also holds', () => {
    const { needles } = staffCatalogNeedles(messages());
    assert.deepEqual(
      needles.map(([k]) => k),
      ['admin.queue.title'],
    );
  });
  it('needles skip a text that member code or the changelog also holds as prose', () => {
    const dir = messages();
    const prose = mkdtempSync(join(tmpdir(), 'sr-staffprose-'));
    roots.push(prose);
    mkdirSync(join(prose, 'src', 'admin'), { recursive: true });
    mkdirSync(join(prose, 'src', 'docs'), { recursive: true });
    // admin/ is staff code: its literals do not count as member prose
    writeFileSync(join(prose, 'src', 'admin', 'A.tsx'), 'const a = "Review the reporter";');
    const withAdmin = staffCatalogNeedles(dir, {
      src: join(prose, 'src'),
      changelog: join(prose, 'CHANGELOG.md'),
    });
    assert.equal(withAdmin.needles.length, 1);
    writeFileSync(join(prose, 'CHANGELOG.md'), "- The reporter's queue before closing is clearer.");
    writeFileSync(join(prose, 'src', 'docs', 'D.ts'), 'export const d = "Review the reporter";');
    const changelog = join(prose, 'CHANGELOG.md');
    // the needle is the longest plain run: "s queue before closing", found in the changelog line
    assert.equal(
      staffCatalogNeedles(dir, { src: join(prose, 'src'), changelog }).needles.length,
      0,
    );
  });
  it('passes when the staff text is only inside staff/', () => {
    assert.deepEqual(catalogRun().failures, []);
  });
  it('f: fails when a member chunk holds a staff message, naming the key and not the text', () => {
    const r = catalogRun((f) => {
      f['app/vendor-BBBB2222.js'] =
        'export const v = "Review the reporter\'s queue before closing";';
    });
    assert.deepEqual(rules(r), ['f']);
    assert.match(r.failures[0], /admin\.queue\.title/);
    assert.doesNotMatch(r.failures[0], /Review the reporter/);
  });
  it('f: fails when app.html holds it', () => {
    const r = catalogRun((f) => {
      f['app.html'] += "<!-- Review the reporter's queue before closing -->";
    });
    assert.deepEqual(rules(r), ['f']);
  });
  it('g: fails when staff/ holds no catalog (the staff messages are missing from it)', () => {
    const r = catalogRun((f) => {
      f['app/staff/Zk3_-9aB.js'] = 'export const a = "m-one";';
    });
    assert.deepEqual(rules(r), ['g']);
  });
});
