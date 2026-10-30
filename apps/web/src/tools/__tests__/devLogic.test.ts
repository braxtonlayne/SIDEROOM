import { describe, expect, it } from 'vitest';
import {
  base64Decode,
  base64Encode,
  canonicalJson,
  countText,
  formatJson,
  jwtDecode,
  markdownTable,
  parseDelimited,
  parseJson,
  runRegex,
  urlDecode,
  urlEncode,
  urlParams,
} from '../dev/logic';

describe('JSON', () => {
  it('formats, minifies and sorts', () => {
    expect(formatJson('{"b":1,"a":[1,2]}', 2)).toEqual({
      ok: true,
      text: '{\n  "b": 1,\n  "a": [\n    1,\n    2\n  ]\n}',
    });
    expect(formatJson('{ "a" : 1 }', 0)).toEqual({ ok: true, text: '{"a":1}' });
    expect(formatJson('{"b":1,"a":{"d":1,"c":2}}', 0, true)).toEqual({
      ok: true,
      text: '{"a":{"c":2,"d":1},"b":1}',
    });
    expect(formatJson('[1]', 'tab')).toEqual({ ok: true, text: '[\n\t1\n]' });
  });

  it('says where a document is wrong', () => {
    const r = parseJson('{\n  "a": 1,\n  "b": }');
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.problem.message.length).toBeGreaterThan(0);
      // The engine reports a position or a line; either way it is a real place when given.
      if (r.problem.line !== null) expect(r.problem.line).toBeGreaterThanOrEqual(1);
    }
    expect(parseJson('').ok).toBe(false);
    expect(parseJson('{"a":1,}').ok).toBe(false);
    expect(parseJson("{'a':1}").ok).toBe(false);
  });

  it('canonical form ignores key order and spacing', () => {
    const a = canonicalJson('{"x":1,"y":[{"b":1,"a":2}]}');
    const b = canonicalJson('{ "y": [ {"a":2, "b":1} ], "x": 1 }');
    expect(a).toEqual(b);
  });
});

describe('Base64 and URL', () => {
  it('round-trips UTF-8, standard and URL-safe', () => {
    const text = 'héllo wörld \u{1F600} ??>>';
    expect(base64Decode(base64Encode(text))).toBe(text);
    const safe = base64Encode(text, true);
    expect(safe).not.toMatch(/[+/=]/);
    expect(base64Decode(safe)).toBe(text);
    expect(base64Encode('hello')).toBe('aGVsbG8=');
    expect(base64Decode('aGVsbG8')).toBe('hello');
    expect(base64Encode('')).toBe('');
  });

  it('refuses what is not Base64 or not UTF-8', () => {
    expect(base64Decode('not base64!!')).toBeNull();
    expect(base64Decode('/w==')).toBeNull(); // 0xFF is not UTF-8
    expect(base64Decode('a')).toBeNull();
  });

  it('percent-encodes parts and whole addresses', () => {
    expect(urlEncode('a b&c=d/é')).toBe('a%20b%26c%3Dd%2F%C3%A9');
    expect(urlEncode('https://x.test/a b?q=1', 'full')).toBe('https://x.test/a%20b?q=1');
    expect(urlDecode('a%20b%26')).toBe('a b&');
    expect(urlDecode('%E0%A4%A')).toBeNull();
    expect(urlParams('https://x.test/p?a=1&b=two%20words&a=3#frag')).toEqual([
      ['a', '1'],
      ['b', 'two words'],
      ['a', '3'],
    ]);
    expect(urlParams('no query')).toEqual([['no query', '']]);
    expect(urlParams('')).toEqual([]);
  });
});

describe('JWT', () => {
  const seg = (o: unknown) => base64Encode(JSON.stringify(o), true);
  const token = `${seg({ alg: 'HS256', typ: 'JWT' })}.${seg({ sub: '1', exp: 1_700_000_000, iat: 1_600_000_000 })}.c2ln`;

  it('reads the header, the payload and the time claims, and never the signature', () => {
    const d = jwtDecode(token)!;
    expect(d.header).toEqual({ alg: 'HS256', typ: 'JWT' });
    expect(d.payload).toMatchObject({ sub: '1' });
    expect(d.signature).toBe('c2ln');
    expect(d.times.map((t) => t.claim)).toEqual(['exp', 'iat']);
    expect(d.times[0]!.iso).toBe('2023-11-14T22:13:20.000Z');
    expect(jwtDecode(`Bearer ${token}`)).not.toBeNull();
  });

  it('refuses things that are not tokens', () => {
    for (const bad of [
      '',
      'a.b',
      'a.b.c',
      `${seg('x')}.${seg({})}.s`,
      `${seg({})}.not-json.s`,
      'a.b.c.d',
    ]) {
      expect(jwtDecode(bad)).toBeNull();
    }
  });
});

describe('Markdown tables', () => {
  it('parses CSV, TSV and semicolons, with quotes', () => {
    expect(parseDelimited('a,b\n1,2')).toEqual([
      ['a', 'b'],
      ['1', '2'],
    ]);
    expect(parseDelimited('a\tb\n1\t2\n')).toEqual([
      ['a', 'b'],
      ['1', '2'],
    ]);
    expect(parseDelimited('a;b\n1;2')).toEqual([
      ['a', 'b'],
      ['1', '2'],
    ]);
    expect(parseDelimited('"a,1","say ""hi"""\nx,y')).toEqual([
      ['a,1', 'say "hi"'],
      ['x', 'y'],
    ]);
    expect(parseDelimited('a,b\r\n\r\n1,2')).toEqual([
      ['a', 'b'],
      ['1', '2'],
    ]);
    expect(parseDelimited('')).toEqual([]);
  });

  it('builds a padded table with alignment and escapes pipes', () => {
    const t = markdownTable(
      [
        ['Name', 'Qty'],
        ['Pen|cil', '12'],
      ],
      ['left', 'right'],
    );
    const lines = t.split('\n');
    expect(lines).toHaveLength(3);
    expect(lines[1]).toMatch(/^\| :-+ \| -+: \|$/);
    expect(lines[2]).toContain('Pen\\|cil');
    // All rows are as wide as each other.
    expect(new Set(lines.map((l) => l.length)).size).toBe(1);
    expect(markdownTable([['a', 'b'], ['1']], ['center'], false).split('\n')[1]).toBe(
      '| :-: | --- |',
    );
    expect(markdownTable([])).toBe('');
  });
});

describe('counts', () => {
  it('counts what a person would', () => {
    const c = countText('Hello there. How are you?\n\nFine, thanks!');
    expect(c.words).toBe(7);
    expect(c.sentences).toBe(3);
    expect(c.paragraphs).toBe(2);
    expect(c.lines).toBe(3);
    expect(c.readingMinutes).toBe(1);
    expect(countText('')).toMatchObject({
      characters: 0,
      words: 0,
      lines: 0,
      bytes: 0,
      readingMinutes: 0,
    });
    expect(countText('é\u{1F600}')).toMatchObject({ characters: 2, bytes: 6 });
    expect(countText('a b').charactersNoSpaces).toBe(2);
  });
});

describe('regex', () => {
  it('lists matches, groups and named groups', () => {
    const r = runRegex('(?<k>\\w)=(\\d+)', 'g', 'a=1 b=22');
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.matches).toHaveLength(2);
      expect(r.matches[1]).toMatchObject({
        index: 4,
        text: 'b=22',
        groups: ['b', '22'],
        named: { k: 'b' },
      });
    }
  });

  it('adds the g flag, rejects bad flags and bad patterns', () => {
    const r = runRegex('a', 'i', 'aAa');
    expect(r.ok && r.matches.length).toBe(3);
    expect(runRegex('a', 'gg', 'a')).toEqual({ ok: false, message: 'bad-flags' });
    expect(runRegex('a', 'x', 'a')).toEqual({ ok: false, message: 'bad-flags' });
    const bad = runRegex('(', 'g', 'a');
    expect(bad.ok).toBe(false);
  });

  it('survives empty matches and caps the output and the input', () => {
    const e = runRegex('x*', 'g', 'ab');
    expect(e.ok && e.matches.length).toBe(3);
    const many = runRegex('a', 'g', 'a'.repeat(2000));
    expect(many.ok && many.matches.length).toBe(500);
    expect(many.ok && many.truncated).toBe(true);
    expect(runRegex('a', 'g', 'a'.repeat(100_001))).toEqual({
      ok: false,
      message: 'input-too-long',
    });
  });
});
