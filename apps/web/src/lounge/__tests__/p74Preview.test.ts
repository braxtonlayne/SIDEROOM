// SEC-AUD-292 (wave 74, D-782): the client's twin of the server's preview drops heading and subtext markers at
// line starts, outside code fences, and nothing else.
import { describe, expect, it } from 'vitest';
import { excerpt } from '../msg/excerpt';

describe('excerpt drops heading and subtext markers (D-782)', () => {
  it('at the start of every line', () => {
    expect(excerpt('# One\n## Two\n### Three')).toBe('One Two Three');
    expect(excerpt('intro\n# Title\nmore')).toBe('intro Title more');
    expect(excerpt('-# small print\ntext')).toBe('small print text');
  });
  it('and leaves what the parser does not read as a marker', () => {
    for (const s of ['C# is fine', 'issue #3', '#tag', 'a # b', '#### four', '#', '-#nospace']) {
      expect(excerpt(s)).toBe(s);
    }
    expect(excerpt('one\n# \ntwo')).toBe('one # two');
  });
  it('and keeps a fenced block as typed', () => {
    expect(excerpt('```\n# not a heading\n```')).toBe('``` # not a heading ```');
  });
  it('and prints later quote and list markers as typed (D-783)', () => {
    expect(excerpt('a\n> q')).toBe('a > q');
    expect(excerpt('a\n>>> q')).toBe('a >>> q');
    expect(excerpt('a\n- i')).toBe('a - i');
    expect(excerpt('a\n1. i')).toBe('a 1. i');
  });
});
