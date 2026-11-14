// SEC-AUD-187 (D-663): the staff key audit, run on a fixture tree.
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { after, describe, it } from 'node:test';
import {
  apply,
  audit,
  namesIn,
  parseFragment,
  plan,
  renderFragment,
  summarize,
  violations,
} from './staff-keys.mjs';

const roots = [];
after(() => roots.forEach((r) => rmSync(r, { recursive: true, force: true })));

const frag = (entries, foot = '} satisfies Record<string, string>;\n') =>
  `export default {\n${Object.entries(entries)
    .map(([k, v]) => `  '${k}': '${v}',\n`)
    .join('')}${foot}`;

/** A small web app: member code, a lazy staff door, the Admin entry, and fragments. */
function fixture(extra = {}) {
  const files = {
    'app.html': '<script type="module" src="/src/app/main.tsx"></script>',
    'src/app/main.tsx': `import { shared } from '../shared';\nimport('../admin/Door');\nt('member.title'); t('shared.both');`,
    'src/shared.ts': `export const shared = 'shared.both';`,
    'src/admin/Door.tsx': `import('./AdminApp');`,
    'src/admin/AdminApp.tsx': `import './Section';\nt('admin.only.title'); t('shared.both');\nt(\`admin.nav.\${id}\`);`,
    'src/admin/Section.tsx': `const keys = { a: 'admin.section.heading' };\nconst k = 'admin.limits.' + x;\nt('admin.section.mixedStaff');`,
    // a test file names a member key as if it were used: it must not count
    'src/admin/Section.test.tsx': `t('member.only.fromTest'); render('admin.only.title');`,
    'src/app/__tests__/x.test.tsx': `t('admin.only.title');`,
    'src/i18n/messages/en.ts': `const core = import.meta.glob('./en/{entrance,a11y}.ts', { eager: true });\n`,
    'src/i18n/messages/en.rest.ts': `const rest = import.meta.glob(['./en/*.ts', '!./en/entrance.ts', '!./en/a11y.ts'], {});\n`,
    // all staff: moves whole
    'src/i18n/messages/en/admin-1.ts': frag({
      'admin.only.title': 'Admin title',
      'admin.nav.users': 'Users',
      'admin.limits.maxUpload': 'Upload',
    }),
    // mixed: member + staff
    'src/i18n/messages/en/mixed-1.ts': frag({
      'member.title': 'Member',
      'admin.section.heading': 'Heading',
      'shared.both': 'Both',
    }),
    // a staff key in a first-paint fragment never moves
    'src/i18n/messages/en/entrance.ts': frag({ 'admin.section.mixedStaff': 'Entrance copy' }),
    // nobody names these
    'src/i18n/messages/en/dead-1.ts': frag({ 'dead.key.one': 'Dead', 'admin.only.second': 'x' }),
    ...extra,
  };
  const root = mkdtempSync(join(tmpdir(), 'sr-staffkeys-'));
  roots.push(root);
  for (const [f, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, f)), { recursive: true });
    writeFileSync(join(root, f), text);
  }
  return { root, src: join(root, 'src'), messages: join(root, 'src/i18n/messages') };
}

const kinds = (a) => Object.fromEntries(a.kind);

describe('naming and classification', () => {
  const fx = fixture();
  const a = audit({ srcDir: fx.src });
  const k = kinds(a);

  it('a key only staff-only modules name is staff', () => {
    assert.equal(k['admin.only.title'], 'staff');
    assert.equal(k['admin.section.heading'], 'staff');
  });
  it('a key a member module also names stays member', () => {
    assert.equal(k['shared.both'], 'member');
    assert.equal(k['member.title'], 'member');
  });
  it('a dynamic key is found by its literal prefix, as template and as concatenation', () => {
    assert.equal(k['admin.nav.users'], 'staff');
    assert.equal(k['admin.limits.maxUpload'], 'staff');
  });
  it('a key nobody names is reported, never staff', () => {
    assert.equal(k['dead.key.one'], 'unnamed');
    assert.equal(k['admin.only.second'], 'unnamed');
  });
  it('a test file is not a naming module', () => {
    assert.ok(![...a.named.get('admin.only.title')].some((m) => /test/.test(m)));
  });
  it('summarizes per fragment', () => {
    const { rows, total } = summarize(a);
    assert.deepEqual(
      rows.find((r) => r.fragment === 'en/mixed-1.ts'),
      { fragment: 'en/mixed-1.ts', firstPaint: false, staff: 1, member: 2, unnamed: 0 },
    );
    assert.equal(total.unnamed, 2);
  });
  it('--check fails while a staff key is in en/, and names the key', () => {
    const bad = violations(a);
    assert.ok(bad.some((b) => b.startsWith('admin.only.title ')));
    // first-paint fragments are exempt
    assert.ok(!bad.some((b) => b.includes('entrance.ts')));
  });
  it('patterns: exact tokens, prefixes, templates', () => {
    const n = namesIn("x('a.b.c'); y('pre.fix.' + z); w(`tpl.${q}.end`); `${a}.noLiteralStart`");
    assert.ok(n.exact.has('a.b.c'));
    assert.ok(n.prefixes.includes('pre.fix.'));
    assert.ok(!n.prefixes.includes('tpl.'));
    assert.ok(n.templates.some((r) => r.test('tpl.anything.end')));
    assert.ok(!n.templates.some((r) => r.test('other.anything.end')));
  });
});

describe('--write', () => {
  it('moves a whole staff fragment, splits a mixed one, leaves first-paint and unnamed alone, and is idempotent', () => {
    const fx = fixture();
    let a = audit({ srcDir: fx.src });
    const p = plan(a);
    assert.deepEqual(p.whole, ['admin-1.ts']);
    assert.deepEqual(
      p.split.map((s) => s.file),
      ['mixed-1.ts'],
    );
    const c = apply(a, fx.messages);
    assert.equal(c.whole, 1);
    assert.equal(c.split, 1);
    assert.ok(existsSync(join(fx.messages, 'en-staff/admin-1.ts')));
    assert.ok(!existsSync(join(fx.messages, 'en/admin-1.ts')));
    const mixedStaff = readFileSync(join(fx.messages, 'en-staff/mixed-1.ts'), 'utf8');
    assert.match(mixedStaff, /admin\.section\.heading/);
    assert.doesNotMatch(mixedStaff, /member\.title/);
    const mixedMember = readFileSync(join(fx.messages, 'en/mixed-1.ts'), 'utf8');
    assert.match(mixedMember, /member\.title/);
    assert.match(mixedMember, /shared\.both/);
    assert.doesNotMatch(mixedMember, /admin\.section\.heading/);
    assert.match(
      readFileSync(join(fx.messages, 'en/entrance.ts'), 'utf8'),
      /admin\.section\.mixedStaff/,
    );
    assert.match(readFileSync(join(fx.messages, 'en/dead-1.ts'), 'utf8'), /dead\.key\.one/);
    a = audit({ srcDir: fx.src });
    assert.deepEqual(violations(a), []);
    assert.deepEqual(plan(a), { whole: [], split: [], back: [] });
    const before = readFileSync(join(fx.messages, 'en-staff/mixed-1.ts'), 'utf8');
    apply(a, fx.messages);
    assert.equal(readFileSync(join(fx.messages, 'en-staff/mixed-1.ts'), 'utf8'), before);
  });

  it('a key in en-staff that a member module names goes back, and --check names it first', () => {
    const fx = fixture({
      'src/i18n/messages/en-staff/leak-1.ts': frag({ 'member.title': 'Leaked' }),
    });
    const a = audit({ srcDir: fx.src });
    assert.ok(violations(a).some((v) => v.startsWith('member.title is in en-staff/leak-1.ts')));
    const c = apply(a, fx.messages);
    assert.equal(c.back, 1);
    assert.match(readFileSync(join(fx.messages, 'en/leak-1.ts'), 'utf8'), /member\.title/);
    assert.ok(!existsSync(join(fx.messages, 'en-staff/leak-1.ts')));
  });
});

describe('fragments', () => {
  it('parse and render keep wrapped values and the comment above an entry', () => {
    const src =
      "export default {\n  // heading\n  'a.b.c': 'one',\n  'a.b.d':\n    'a long wrapped value',\n} satisfies Record<string, string>;\n";
    const p = parseFragment(src);
    assert.deepEqual(
      p.entries.map((e) => e.key),
      ['a.b.c', 'a.b.d'],
    );
    assert.match(p.entries[0].text, /heading/);
    assert.equal(renderFragment(p, p.entries), src);
  });
});
