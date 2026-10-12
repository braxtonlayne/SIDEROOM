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
