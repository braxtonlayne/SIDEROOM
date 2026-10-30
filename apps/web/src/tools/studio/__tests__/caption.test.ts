import { describe, expect, it } from 'vitest';
import {
  DEFAULT_CAPTION,
  drawCaptions,
  layoutCaption,
  resolveFont,
  wrapLines,
  type TextContext,
} from '../caption';

const measure = (s: string) => s.length * 10;

function fakeContext(calls: { kind: string; text: string; y: number }[] = []): TextContext {
  return {
    font: '',
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 0,
    lineJoin: '',
    textAlign: '',
    textBaseline: '',
    measureText: (s) => ({ width: s.length * 8 }),
    fillText: (text, _x, y) => void calls.push({ kind: 'fill', text, y }),
    strokeText: (text, _x, y) => void calls.push({ kind: 'stroke', text, y }),
  };
}

describe('captions', () => {
  it('wraps on words, breaks a word that is too wide, and keeps blank lines', () => {
    expect(wrapLines('one two three four', 90, measure)).toEqual(['one two', 'three', 'four']);
    expect(wrapLines('abcdefghijkl', 50, measure)).toEqual(['abcde', 'fghij', 'kl']);
    expect(wrapLines('a\n\nb', 100, measure)).toEqual(['a', '', 'b']);
    expect(wrapLines('', 100, measure)).toEqual(['']);
  });

  it('shrinks the font until the caption takes at most 40% of the height', () => {
    const at = (px: number) => (s: string) => s.length * px * 0.6;
    const long = 'this is a very long caption that needs several lines at a big size';
    const big = layoutCaption(long, 400, 400, 1, at)!;
    expect(big.lines.length * big.lineHeight).toBeLessThanOrEqual(400 * 0.4);
    expect(layoutCaption('   ', 400, 400, 1, at)).toBeNull();
    expect(layoutCaption('hi', 400, 400, 1, at)!.fontPx).toBe(40);
  });

  it('draws the stroke before the fill, top near the top and bottom near the bottom', () => {
    const calls: { kind: string; text: string; y: number }[] = [];
    const ctx = fakeContext(calls);
    const out = drawCaptions(
      ctx,
      300,
      200,
      { ...DEFAULT_CAPTION, top: 'hello', bottom: 'world' },
      'Test',
    );
    expect(out).toEqual({ top: ['HELLO'], bottom: ['WORLD'] });
    expect(calls.map((c) => c.kind)).toEqual(['stroke', 'fill', 'stroke', 'fill']);
    expect(calls[0]!.y).toBeLessThan(20);
    expect(calls[2]!.y).toBeGreaterThan(130);
    expect(ctx.font).toContain('Test');
    expect(ctx.lineJoin).toBe('round');
  });

  it('keeps case when uppercase is off and falls back to a font stack outside a theme', () => {
    const out = drawCaptions(
      fakeContext(),
      200,
      200,
      { ...DEFAULT_CAPTION, top: 'Mixed Case', uppercase: false },
      'X',
    );
    expect(out.top).toEqual(['Mixed Case']);
    expect(resolveFont('mono')).toMatch(/mono/);
  });
});
