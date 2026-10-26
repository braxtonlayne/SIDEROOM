// RDY-SEC-004: docs/15-threat-model.md names a file and a test for each control. This checks the references
// against the tree, so the document cannot drift into claims nobody can find.
import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const doc = readFileSync(join(ROOT, 'docs/15-threat-model.md'), 'utf8');
// Every roadmap from phase 20 on defines RDY items (roadmap-p20.md, roadmap-p25.md, ...).
const roadmap = readdirSync(join(ROOT, 'docs/dev'))
  .filter((f) => /^roadmap-p\d+\.md$/.test(f) && Number(f.match(/\d+/)[0]) >= 20)
  .map((f) => readFileSync(join(ROOT, 'docs/dev', f), 'utf8'))
  .join('\n');

const ROOTS = /^(crates|apps|deploy|scripts|docs|\.github|e2e|packages)\//;
const tokens = [...doc.matchAll(/`([^`\n]+)`/g)].map((m) => m[1]);
const refs = tokens.filter((t) => /^[\w./@:-]+$/.test(t) && (ROOTS.test(t) || t === 'AGENTS.md'));

const threatRows = doc
  .split('\n')
  .filter((l) => /^\| T\d+ \|/.test(l))
  .map((l) => l.split('|').map((c) => c.trim()));

test('the document has rows T1 to T27, in order, each with threat, controls and residual', () => {
  assert.equal(threatRows.length, 27);
  threatRows.forEach((r, i) => {
    assert.equal(r[1], `T${i + 1}`);
    assert.ok(r[2] && r[3] && r[4], r[1]);
  });
});

test('every named file exists and every file::name pair finds its name', () => {
  assert.ok(refs.length > 150, `only ${refs.length} references`);
  const problems = [];
  for (const ref of refs) {
    const [path, name] = ref.split('::');
    const abs = join(ROOT, path);
    if (!existsSync(abs)) {
      problems.push(`${ref}: no such file`);
      continue;
    }
    if (name === undefined) continue;
    const text = readFileSync(abs, 'utf8');
    const found = path.endsWith('.rs') ? text.includes(`fn ${name}(`) : text.includes(name);
    if (!found) problems.push(`${ref}: "${name}" not found in the file`);
  }
  assert.deepEqual(problems, []);
});

test('every control row names a test or a CI job, and "open" always names an RDY item', () => {
  for (const r of threatRows) {
    assert.ok(/::|\.github\/workflows\/|\.test\./.test(r[3]), `${r[1]} names no test or CI job`);
    const residual = r[4].replace(/`[^`]*`/g, '');
    if (/\bopen\b/i.test(residual)) {
      assert.match(residual, /RDY-[A-Z]+-\d{3}/, `${r[1]} says open without an RDY item`);
    }
  }
});

test('every RDY item the document names exists in the roadmap', () => {
  const ids = new Set(doc.match(/RDY-[A-Z]+-\d{3}/g));
  assert.ok(ids.size >= 8);
  for (const id of ids) assert.ok(roadmap.includes(`**${id} `) || roadmap.includes(id), id);
});

test('the sections the roadmap asked for are there', () => {
  for (const s of [
    'Person graph',
    'moderation register',
    'Data-subject exports',
    'suspicion signals',
    'takedown notifier',
    'data subject',
  ]) {
    assert.ok(doc.toLowerCase().includes(s.toLowerCase()), s);
  }
  for (const s of [
    'Payments',
    'Bot gateway and OAuth2',
    'Hostile Activity',
    'Webhook execute',
    'Web Push',
    'SDK voice bot',
  ]) {
    assert.ok(
      threatRows.some((r) => r[2].includes(s)),
      s,
    );
  }
});
