// @ts-check
// Guess matching and Guild Prompt Pack validation (shared/match.js, shared/pack.js).
import assert from 'node:assert/strict';
import { distance, judge, maskPrompt, normalize } from '../shared/match.js';
import { PACK_MAX_BYTES, parsePack, parseShelfRef } from '../shared/pack.js';
import { BUILT_IN_PROMPTS } from '../shared/prompts.js';

test('normalize drops case, accents, punctuation, articles and extra spaces', () => {
  assert.equal(normalize('  The Haunted  Toaster!! '), 'haunted toaster');
  assert.equal(normalize('Café au lait'), 'cafe au lait');
  assert.equal(distance('kitten', 'sitting'), 3);
  assert.equal(distance('abc', 'abcdefgh', 2), 3, 'stops early past the limit');
});

test('judge: hits with a typo allowance that grows with length, close calls, misses', () => {
  assert.equal(judge('haunted toaster', 'haunted toaster'), 'hit');
  assert.equal(judge('a haunted tosater', 'haunted toaster'), 'hit', 'two edits on a long prompt');
  assert.equal(judge('hauntedtoaster', 'haunted toaster'), 'hit', 'spaces do not matter');
  assert.equal(judge('haunted roast', 'haunted toaster'), 'close');
  assert.equal(judge('microwave', 'haunted toaster'), 'miss');
  assert.equal(judge('cat', 'car'), 'miss', 'no allowance on short prompts');
  assert.equal(judge('', 'car'), 'miss');
});

test('the mask hides letters and digits only, and reveals the shown positions', () => {
  assert.equal(maskPrompt("cat's 2 hats"), "___'_ _ ____");
  assert.equal(maskPrompt('cat', [1]), '_a_');
});

test('the built-in set is clean: unique, short, printable, no angle brackets', () => {
  const seen = new Set();
  for (const p of BUILT_IN_PROMPTS) {
    assert.ok(p.length <= 48 && p.length >= 3, p);
    assert.ok(!/[<>\p{C}]/u.test(p), p);
    assert.ok(!seen.has(p.toLowerCase()), `duplicate ${p}`);
    seen.add(p.toLowerCase());
  }
});

test('packs: JSON with a name, a bare array, and text with comments', () => {
  const json = parsePack(JSON.stringify({ name: ' Office\u0000 chaos ', prompts: ['stapler duel', 'reply all storm', 'printer jam', 'coffee queue', 'desk plant', 'stapler duel', 7, 'x'.repeat(60)] }));
  assert.equal(json.ok, true);
  if (!json.ok) return;
  assert.equal(json.name, 'Office chaos');
  assert.equal(json.prompts.length, 5);
  assert.equal(json.skipped, 3, 'a duplicate, a number and a long prompt');
  const arr = parsePack('["one thing", "two things", "three things", "four things", "five things"]', 'Mine');
  assert.ok(arr.ok && arr.name === 'Mine');
  const text = parsePack(new TextEncoder().encode('﻿# my pack\nalpha dog\n\nbeta fish\r\ngamma ray\ndelta wing\nepsilon <script>\n'));
  assert.ok(text.ok);
  if (text.ok) assert.deepEqual(text.prompts, ['alpha dog', 'beta fish', 'gamma ray', 'delta wing', 'epsilon script']);
});

test('packs: refused when empty, too big, not UTF-8, bad JSON, too few or too many', () => {
  assert.deepEqual(parsePack(''), { ok: false, error: 'empty' });
  assert.deepEqual(parsePack('a\n'.repeat(PACK_MAX_BYTES)), { ok: false, error: 'too-big' });
  assert.deepEqual(parsePack(new Uint8Array([0xff, 0xfe, 0x00])), { ok: false, error: 'not-text' });
  assert.deepEqual(parsePack('{"prompts": [}'), { ok: false, error: 'bad-json' });
  assert.deepEqual(parsePack('{"name": "x"}'), { ok: false, error: 'no-prompts' });
  assert.deepEqual(parsePack('one\ntwo'), { ok: false, error: 'too-few' });
  assert.deepEqual(parsePack(Array.from({ length: 1200 }, (_, i) => `prompt ${i}`).join('\n')), { ok: false, error: 'too-many' });
});

test('shelf references: "<id>/<hash>" or any text holding both', () => {
  const h = 'ab'.repeat(32);
  assert.deepEqual(parseShelfRef(`123/${h}`), { shelf_id: '123', object_hash: h });
  assert.deepEqual(parseShelfRef(`https://x.example/shelves/987/objects/${h.toUpperCase()}`), { shelf_id: '987', object_hash: h });
  assert.equal(parseShelfRef('123/abc'), null);
  assert.equal(parseShelfRef(h), null);
});
