export interface TextEdit {
  value: string;
  selStart: number;
  selEnd: number;
}

/**
 * Wrap the selection in a markdown mark (`**`, `*`, `__`, `~~`, `||`, `` ` ``). If the selection is already
 * wrapped by exactly this mark it is unwrapped. With an empty selection the marks are inserted with the caret between.
 */
export function toggleWrap(value: string, start: number, end: number, mark: string): TextEdit {
  const before = value.slice(0, start);
  const sel = value.slice(start, end);
  const after = value.slice(end);
  const n = mark.length;
  if (before.endsWith(mark) && after.startsWith(mark)) {
    return {
      value: before.slice(0, -n) + sel + after.slice(n),
      selStart: start - n,
      selEnd: end - n,
    };
  }
  if (sel.length >= 2 * n && sel.startsWith(mark) && sel.endsWith(mark)) {
    const inner = sel.slice(n, -n);
    return { value: before + inner + after, selStart: start, selEnd: start + inner.length };
  }
  return { value: before + mark + sel + mark + after, selStart: start + n, selEnd: end + n };
}

export function insertAt(value: string, start: number, end: number, text: string): TextEdit {
  const next = value.slice(0, start) + text + value.slice(end);
  const caret = start + text.length;
  return { value: next, selStart: caret, selEnd: caret };
}

/** Prefixes every line touched by the selection (block quote `> `, list `- `); a second use removes it. */
export function prefixLines(value: string, start: number, end: number, prefix: string): TextEdit {
  const from = value.lastIndexOf('\n', start - 1) + 1;
  const nl = value.indexOf('\n', end);
  const to = nl === -1 ? value.length : nl;
  const lines = value.slice(from, to).split('\n');
  const all = lines.every((l) => l.startsWith(prefix));
  const next = lines.map((l) => (all ? l.slice(prefix.length) : prefix + l)).join('\n');
  return {
    value: value.slice(0, from) + next + value.slice(to),
    selStart: from,
    selEnd: from + next.length,
  };
}

export const WRAP_KEYS: Record<string, string> = { b: '**', i: '*', u: '__' };
export const WRAP_SHIFT_KEYS: Record<string, string> = { x: '~~', s: '||', c: '`' };

export function typingLabel(names: readonly string[]): string | null {
  if (names.length === 0) return null;
  if (names.length === 1) return `${names[0]} is typing…`;
  if (names.length === 2) return `${names[0]} and ${names[1]} are typing…`;
  if (names.length === 3) return `${names[0]}, ${names[1]}, and ${names[2]} are typing…`;
  return 'Several people are typing…';
}

export const MAX_MESSAGE_LENGTH = 2000;

export type ComposerTokenKind = 'text' | 'mention' | 'channel' | 'emoji' | 'timestamp';
export interface ComposerToken {
  kind: ComposerTokenKind;
  text: string;
}

/** Longest text the highlight backdrop mirrors (D-299); longer drafts are left plain. */
export const BACKDROP_MAX_CHARS = 2000;

// User, nickname and role mentions, channels, custom emoji, timestamps and the two broadcast words.
const TOKEN_RE = /<@[!&]?\d+>|<#\d+>|<a?:\w{2,32}:\d+>|<t:-?\d+(?::[tTdDfFR])?>|@everyone|@here/g;

/**
 * Splits a draft into plain text and the tokens the composer recognises. Concatenating the `text` of every
 * piece gives back the input exactly, so a mirror of it lines up character for character with the textarea.
 */
export function tokenizeComposer(value: string): ComposerToken[] {
  const out: ComposerToken[] = [];
  let last = 0;
  for (const m of value.matchAll(TOKEN_RE)) {
    const at = m.index ?? 0;
    if (at > last) out.push({ kind: 'text', text: value.slice(last, at) });
    const s = m[0];
    const kind: ComposerTokenKind =
      s.startsWith('<@') || s.startsWith('@')
        ? 'mention'
        : s.startsWith('<#')
          ? 'channel'
          : s.startsWith('<t:')
            ? 'timestamp'
            : 'emoji';
    out.push({ kind, text: s });
    last = at + s.length;
  }
  if (last < value.length) out.push({ kind: 'text', text: value.slice(last) });
  return out;
}
