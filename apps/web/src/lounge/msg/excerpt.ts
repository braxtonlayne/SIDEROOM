/**
 * The client's twin of the server's `sr_chat::preview::excerpt` (D-688, D-689), for a preview built from a
 * gateway event instead of a list the server cut. The same rule and the same test cases
 * (`lounge/__tests__/previews.test.ts`): first `max` characters (code points) on one line, runs of white space
 * one space, no cut between a character and the marks that follow it, an ellipsis ends a cut text, `null`
 * when nothing but white space is left.
 */
export const EXCERPT_CHARS = 140;

/** Combining marks, joiners, variation selectors, emoji modifiers and tag characters continue the one before. */
const RANGES: readonly (readonly [number, number])[] = [
  [0x0300, 0x036f],
  [0x0483, 0x0489],
  [0x0591, 0x05bd],
  [0x0610, 0x061a],
  [0x064b, 0x065f],
  [0x0900, 0x0903],
  [0x093a, 0x094f],
  [0x1ab0, 0x1aff],
  [0x1dc0, 0x1dff],
  [0x200c, 0x200d],
  [0x20d0, 0x20ff],
  [0xfe00, 0xfe0f],
  [0xfe20, 0xfe2f],
  [0x1f3fb, 0x1f3ff],
  [0xe0020, 0xe007f],
  [0xe0100, 0xe01ef],
];
const extendsPrev = (c: string): boolean => {
  const cp = c.codePointAt(0) ?? 0;
  return RANGES.some(([lo, hi]) => cp >= lo && cp <= hi);
};
const WHITE = /^\s$/u;

export function excerpt(content: string, max: number = EXCERPT_CHARS): string | null {
  const kept: string[] = [];
  let pendingSpace = false;
  let cut = false;
  for (const c of content) {
    if (WHITE.test(c)) {
      pendingSpace = kept.length > 0;
      continue;
    }
    if (pendingSpace) {
      pendingSpace = false;
      if (kept.length === max) {
        cut = true;
        break;
      }
      kept.push(' ');
    }
    if (kept.length === max) {
      if (extendsPrev(c)) {
        // The cut would split a cluster: take the character before it out too.
        while (kept.length > 0 && extendsPrev(kept[kept.length - 1]!)) kept.pop();
        kept.pop();
      }
      cut = true;
      break;
    }
    kept.push(c);
  }
  if (kept.length === 0) return null;
  while (kept[kept.length - 1] === ' ') kept.pop();
  return kept.join('') + (cut ? '\u2026' : '');
}
