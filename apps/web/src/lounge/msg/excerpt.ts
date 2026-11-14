/**
 * The client's twin of the server's `sr_chat::preview::excerpt` (D-688, D-689, D-728), for a preview built from
 * a gateway event instead of a list the server cut. The same rule and the same test cases
 * (`crates/sr-chat/src/preview_cases.json`, run by `lounge/__tests__/p70Plain.test.tsx` and by `preview.rs`):
 * first `max` characters (code points) on one line, runs of white space one space, no cut between a character
 * and the marks that follow it, an ellipsis ends a cut text, `null` when nothing but white space is left. A cut
 * also never falls inside markup: the one-line text of the cut text must be the start of the one-line text of
 * the whole message. When it is not, the cut text first gets the closing mark of the run it opened (D-735: one of
 * ||, **, __, ~~, *, _ or a backtick, tried in that order; the first whose one-line text, trimmed at the end, is
 * the start of the whole's wins, and the result is cut text, closer, ellipsis). Only if none does, the cut moves
 * back (to before the last markup opener, then to the previous white space, alternately, at most 16 times);
 * none left is `null`. A text of `max` characters or fewer is never
 * parsed. When a cut is needed and the parser is not loaded the result is `null` (no text), never the raw cut.
 */
import { currentMarkdownEngine, parseMarkdown } from '../markdown';
import { singleLineText } from './SingleLine';

export const EXCERPT_CHARS = 140;

/** The word the comparison uses for a spoiler (the server's `[spoiler]`). */
const SPOILER = '[spoiler]';
/** The closing marks a cut text may be given, in the order they are tried (D-735). */
const CLOSERS = ['||', '**', '__', '~~', '*', '_', '`'] as const;
/** How many times the cut may move back looking for a place that splits no markup. */
const CUT_TRIES = 16;

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

/** The text on one line as code points: runs of white space one space, both ends trimmed. */
function fold(content: string): string[] {
  const kept: string[] = [];
  let pendingSpace = false;
  for (const c of content) {
    if (WHITE.test(c)) {
      pendingSpace = kept.length > 0;
      continue;
    }
    if (pendingSpace) {
      pendingSpace = false;
      kept.push(' ');
    }
    kept.push(c);
  }
  return kept;
}

/** Where the first `max` characters of `folded` end: not before a mark that continues the one before. */
function cutIndex(folded: readonly string[], max: number): number {
  let cut = max;
  if (extendsPrev(folded[cut]!)) {
    // The cut would split a cluster: take the character before it out too.
    while (cut > 0 && extendsPrev(folded[cut - 1]!)) cut--;
    cut = Math.max(0, cut - 1);
  }
  return cut;
}

const trimEnd = (folded: readonly string[], n: number): number => {
  while (n > 0 && folded[n - 1] === ' ') n--;
  return n;
};

/**
 * The plain cut, with no regard for markup: for a text that already is plain (a notification's one-line text,
 * `notifyText.ts`), where nothing can be split.
 */
export function excerptPlain(content: string, max: number = EXCERPT_CHARS): string | null {
  const folded = fold(content);
  if (folded.length === 0) return null;
  if (folded.length <= max) return folded.join('');
  const n = trimEnd(folded, cutIndex(folded, max));
  return folded.slice(0, n).join('') + '\u2026';
}

const opener = (s: readonly string[], i: number): boolean => {
  const c = s[i]!;
  if (c === '|') return s[i + 1] === '|';
  return c === '<' || c === '`' || c === '[' || c === '*' || c === '_' || c === '~';
};

const lineOf = (chars: readonly string[]): string =>
  singleLineText(parseMarkdown(chars.join('')), undefined, SPOILER);

/** The preview text of a message: see the file comment. */
export function excerpt(content: string, max: number = EXCERPT_CHARS): string | null {
  const folded = fold(content);
  if (folded.length === 0) return null;
  if (folded.length <= max) return folded.join('');
  if (!currentMarkdownEngine()) return null;
  let cut = trimEnd(folded, cutIndex(folded, max));
  const full = lineOf(folded);
  let byOpener = true;
  for (let i = 0; i <= CUT_TRIES; i++) {
    if (cut === 0) return null;
    if (full.startsWith(lineOf(folded.slice(0, cut)))) {
      return folded.slice(0, cut).join('') + '\u2026';
    }
    // The first cut only: close the run the cut opened, if one closing mark makes the text read as the start of
    // the whole (D-735). At most seven more short parses.
    if (i === 0) {
      for (const closer of CLOSERS) {
        const closed = folded.slice(0, cut).join('') + closer;
        const line = lineOf([...closed]).trimEnd();
        if (line !== '' && full.startsWith(line)) return closed + '\u2026';
      }
    }
    const head = folded.slice(0, cut);
    let next = -1;
    if (byOpener) {
      for (let k = cut - 1; k >= 0; k--) {
        if (opener(head, k)) {
          next = k;
          break;
        }
      }
    } else {
      next = head.lastIndexOf(' ');
    }
    byOpener = !byOpener;
    // Nothing to move to: one character back.
    cut = trimEnd(folded, next < 0 ? cut - 1 : next);
  }
  return null;
}
