// UTL-SRC-001: check-tools-corpus.mjs on the real corpus and on broken ones.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { corpusProblems, registryIds, registryMismatch } from './check-tools-corpus.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const phrases = (prefix, n = 10) =>
  Array.from({ length: n }, (_, i) => `${prefix} phrase ${'x'.repeat(i + 1)}`);
const tool = (id, extra = {}) => ({
  id,
  name: `Tool ${id}`,
  aliases: [],
  phrases: phrases(id),
  ...extra,
});

test('the real corpus is clean', () => {
  const doc = JSON.parse(readFileSync(join(ROOT, 'apps/web/src/tools/corpus.json'), 'utf8'));
  assert.deepEqual(corpusProblems(doc), []);
});

test('a good small corpus is clean', () => {
  assert.deepEqual(corpusProblems({ tools: [tool('alpha'), tool('beta')] }), []);
});

test('empty tools, too few and too many phrases are reported', () => {
  assert.match(corpusProblems({ tools: [tool('a', { phrases: [] })] })[0], /no phrases/);
  assert.match(
    corpusProblems({ tools: [tool('a', { phrases: phrases('a', 3) })] })[0],
    /3 phrases/,
  );
  assert.match(
    corpusProblems({ tools: [tool('a', { phrases: phrases('a', 31) })] })[0],
    /31 phrases/,
  );
  assert.match(corpusProblems({ tools: [] })[0], /no "tools"/);
});

test('duplicates are reported within a tool and across tools', () => {
  const dup = phrases('a');
  dup[1] = dup[0].toUpperCase() + '!';
  assert.ok(
    corpusProblems({ tools: [tool('a', { phrases: dup })] }).some((p) =>
      /duplicate phrase/.test(p),
    ),
  );
  const shared = tool('b', { phrases: [...phrases('b', 9), phrases('a')[0]] });
  assert.ok(
    corpusProblems({ tools: [tool('a'), shared] }).some((p) => /also a phrase of a/.test(p)),
  );
  assert.ok(corpusProblems({ tools: [tool('a'), tool('a')] }).some((p) => /duplicate id/.test(p)));
});

test('an alias that shadows another tool, empty phrases and bad ids are reported', () => {
  const a = tool('a', { aliases: ['Zed'] });
  const b = tool('b', { aliases: ['zed'] });
  assert.ok(corpusProblems({ tools: [a, b] }).some((p) => /also a name or alias of a/.test(p)));
  assert.ok(
    corpusProblems({ tools: [tool('a', { phrases: [...phrases('a', 9), '  ?! '] })] }).some((p) =>
      /empty or non-string/.test(p),
    ),
  );
  assert.ok(corpusProblems({ tools: [tool('Bad_Id')] }).some((p) => /kebab-case/.test(p)));
});

test('the corpus ids equal the tool registry ids (both ways)', () => {
  const src = "const a = { id: 'calc' };\nconst b = devTool(\n    'md-table',\n    'grid',\n  );";
  assert.deepEqual(registryIds(src), ['calc', 'md-table']);
  assert.deepEqual(
    registryMismatch({ tools: [{ id: 'calc' }, { id: 'md-table' }] }, ['calc', 'md-table']),
    [],
  );
  const out = registryMismatch({ tools: [{ id: 'calc' }, { id: 'tier-list' }] }, [
    'calc',
    'tierlist',
  ]);
  assert.ok(out.some((p) => /registry tool tierlist has no corpus entry/.test(p)));
  assert.ok(out.some((p) => /corpus entry tier-list is not a registry tool/.test(p)));
  const real = readFileSync(new URL('../apps/web/src/tools/registry.ts', import.meta.url), 'utf8');
  const doc = JSON.parse(
    readFileSync(new URL('../apps/web/src/tools/corpus.json', import.meta.url), 'utf8'),
  );
  assert.deepEqual(registryMismatch(doc, registryIds(real)), []);
});
