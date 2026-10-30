/**
 * UTL-DEV-001: the pure logic of the developer tools. Nothing here touches React or the network; every
 * function takes text and returns a result or a typed problem, so each tool is a thin view over one of them.
 */

// ------------------------------------------------------------------ JSON

export interface JsonProblem {
  message: string;
  /** 1-based, when the engine said where. */
  line: number | null;
  column: number | null;
}

export type JsonResult = { ok: true; text: string } | { ok: false; problem: JsonProblem };

function locate(text: string, message: string): JsonProblem {
  // V8: "... in JSON at position 12"; Firefox: "... at line 2 column 5"; Safari has neither.
  const lc = /line (\d+) column (\d+)/i.exec(message);
  if (lc) return { message, line: Number(lc[1]), column: Number(lc[2]) };
  const pos = /position (\d+)/i.exec(message);
  if (pos) {
    const at = Math.min(Number(pos[1]), text.length);
    const before = text.slice(0, at);
    const line = before.split('\n').length;
    return { message, line, column: at - (before.lastIndexOf('\n') + 1) + 1 };
  }
  return { message, line: null, column: null };
}

export function parseJson(
  text: string,
): { ok: true; value: unknown } | { ok: false; problem: JsonProblem } {
  try {
    return { ok: true, value: JSON.parse(text) as unknown };
  } catch (e) {
    return { ok: false, problem: locate(text, e instanceof Error ? e.message : String(e)) };
  }
}

function sortKeys(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(sortKeys);
  if (v && typeof v === 'object') {
    return Object.fromEntries(
      Object.keys(v)
        .sort()
        .map((k) => [k, sortKeys((v as Record<string, unknown>)[k])]),
    );
  }
  return v;
}

export function formatJson(
  text: string,
  indent: 0 | 2 | 4 | 'tab' = 2,
  sorted = false,
): JsonResult {
  const r = parseJson(text);
  if (!r.ok) return r;
  const value = sorted ? sortKeys(r.value) : r.value;
  return {
    ok: true,
    text: JSON.stringify(value, null, indent === 'tab' ? '\t' : indent === 0 ? undefined : indent),
  };
}

/** The same value with sorted keys and two-space indentation, so two documents diff by content. */
export function canonicalJson(text: string): JsonResult {
  return formatJson(text, 2, true);
}

// ------------------------------------------------------------------ Base64 and URL

const enc = new TextEncoder();
const dec = new TextDecoder('utf-8', { fatal: true });

export function bytesToBase64(bytes: Uint8Array, urlSafe = false): string {
  let bin = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  const b = btoa(bin);
  return urlSafe ? b.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '') : b;
}

export function base64ToBytes(text: string): Uint8Array | null {
  const s = text.replace(/\s+/g, '').replace(/-/g, '+').replace(/_/g, '/');
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(s)) return null;
  const padded = s + '='.repeat((4 - (s.length % 4)) % 4);
  try {
    const bin = atob(padded);
    return Uint8Array.from(bin, (c) => c.charCodeAt(0));
  } catch {
    return null;
  }
}

export const base64Encode = (text: string, urlSafe = false) =>
  bytesToBase64(enc.encode(text), urlSafe);

/** Decode Base64 (standard or URL-safe) as UTF-8 text; `null` when it is not valid Base64 or not UTF-8. */
export function base64Decode(text: string): string | null {
  const bytes = base64ToBytes(text);
  if (!bytes) return null;
  try {
    return dec.decode(bytes);
  } catch {
    return null;
  }
}

export type UrlMode = 'component' | 'full';

export function urlEncode(text: string, mode: UrlMode = 'component'): string {
  return mode === 'component' ? encodeURIComponent(text) : encodeURI(text);
}

export function urlDecode(text: string, mode: UrlMode = 'component'): string | null {
  try {
    return mode === 'component' ? decodeURIComponent(text) : decodeURI(text);
  } catch {
    return null;
  }
}

/** The query parameters of a URL or a bare query string, in order, decoded. */
export function urlParams(text: string): [string, string][] {
  const q = text.includes('?') ? text.slice(text.indexOf('?') + 1) : text;
  const [query = ''] = q.split('#');
  if (!query) return [];
  return Array.from(new URLSearchParams(query).entries());
}

// ------------------------------------------------------------------ JWT (decode only)

export interface JwtDecoded {
  header: unknown;
  payload: unknown;
  /** The signature segment as written; it is never checked. */
  signature: string;
  /** Registered time claims, as epoch seconds -> ISO strings. */
  times: { claim: 'exp' | 'iat' | 'nbf'; iso: string }[];
}

export function jwtDecode(token: string): JwtDecoded | null {
  const parts = token
    .trim()
    .replace(/^bearer\s+/i, '')
    .split('.');
  if (parts.length !== 3) return null;
  const json = (seg: string): unknown => {
    const s = base64Decode(seg);
    if (s === null) throw new Error('segment');
    return JSON.parse(s) as unknown;
  };
  try {
    const header = json(parts[0] ?? '');
    const payload = json(parts[1] ?? '');
    if (!header || typeof header !== 'object' || !payload || typeof payload !== 'object')
      return null;
    const times: JwtDecoded['times'] = [];
    for (const claim of ['exp', 'iat', 'nbf'] as const) {
      const v = (payload as Record<string, unknown>)[claim];
      if (typeof v === 'number' && Number.isFinite(v)) {
        const d = new Date(v * 1000);
        if (!Number.isNaN(d.getTime())) times.push({ claim, iso: d.toISOString() });
      }
    }
    return { header, payload, signature: parts[2] ?? '', times };
  } catch {
    return null;
  }
}

// ------------------------------------------------------------------ Markdown tables

export type Align = 'left' | 'center' | 'right' | 'none';

/** Split pasted CSV or TSV into rows (a quoted field may hold the delimiter and doubled quotes). */
export function parseDelimited(text: string): string[][] {
  const first = text.split('\n', 1)[0] ?? '';
  const delim = first.includes('\t')
    ? '\t'
    : first.includes(';') && !first.includes(',')
      ? ';'
      : ',';
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = '';
  let quoted = false;
  const t = text.replace(/\r\n?/g, '\n');
  for (let i = 0; i < t.length; i++) {
    const c = t[i]!;
    if (quoted) {
      if (c === '"' && t[i + 1] === '"') {
        cell += '"';
        i++;
      } else if (c === '"') quoted = false;
      else cell += c;
    } else if (c === '"' && cell === '') quoted = true;
    else if (c === delim) {
      row.push(cell);
      cell = '';
    } else if (c === '\n') {
      row.push(cell);
      rows.push(row);
      row = [];
      cell = '';
    } else cell += c;
  }
  if (cell !== '' || row.length) {
    row.push(cell);
    rows.push(row);
  }
  return rows.filter((r) => r.some((c) => c.trim() !== ''));
}

const escapeCell = (c: string) => c.trim().replace(/\|/g, '\\|').replace(/\n/g, '<br>');

/** A GitHub-flavoured table; the first row is the header. Columns are padded so the source reads as a grid. */
export function markdownTable(rows: string[][], aligns: Align[] = [], pretty = true): string {
  if (rows.length === 0) return '';
  const cols = Math.max(...rows.map((r) => r.length));
  const grid = rows.map((r) => Array.from({ length: cols }, (_, i) => escapeCell(r[i] ?? '')));
  const widths = Array.from({ length: cols }, (_, i) =>
    Math.max(3, ...grid.map((r) => (pretty ? [...(r[i] ?? '')].length : 0))),
  );
  const pad = (s: string, i: number, a: Align) => {
    if (!pretty) return s;
    const gap = widths[i]! - [...s].length;
    if (a === 'right') return ' '.repeat(gap) + s;
    if (a === 'center') return ' '.repeat(Math.floor(gap / 2)) + s + ' '.repeat(Math.ceil(gap / 2));
    return s + ' '.repeat(gap);
  };
  const line = (cells: string[]) =>
    `| ${cells.map((c, i) => pad(c, i, aligns[i] ?? 'none')).join(' | ')} |`;
  const sep = Array.from({ length: cols }, (_, i) => {
    const a = aligns[i] ?? 'none';
    const w = pretty ? widths[i]! : 3;
    const dashes = '-'.repeat(Math.max(1, w - (a === 'center' ? 2 : a === 'none' ? 0 : 1)));
    return a === 'left'
      ? `:${dashes}`
      : a === 'right'
        ? `${dashes}:`
        : a === 'center'
          ? `:${dashes}:`
          : dashes.padEnd(w, '-');
  });
  return [line(grid[0]!), `| ${sep.join(' | ')} |`, ...grid.slice(1).map(line)].join('\n');
}

// ------------------------------------------------------------------ counters

export interface TextCounts {
  characters: number;
  charactersNoSpaces: number;
  words: number;
  lines: number;
  sentences: number;
  paragraphs: number;
  bytes: number;
  /** Minutes at 230 words per minute, rounded up (0 for no words). */
  readingMinutes: number;
}

export function countText(text: string): TextCounts {
  const chars = [...text];
  const words = text.trim() === '' ? 0 : text.trim().split(/\s+/u).length;
  const sentences = (text.match(/[^.!?\n]+[.!?]+(?=\s|$)|[^.!?\n]+$/gmu) ?? []).filter((s) =>
    /\S/u.test(s),
  ).length;
  return {
    characters: chars.length,
    charactersNoSpaces: chars.filter((c) => !/\s/u.test(c)).length,
    words,
    lines: text === '' ? 0 : text.split(/\r\n|\r|\n/).length,
    sentences,
    paragraphs: text.split(/\n\s*\n/u).filter((p) => p.trim() !== '').length,
    bytes: enc.encode(text).length,
    readingMinutes: words === 0 ? 0 : Math.ceil(words / 230),
  };
}

// ------------------------------------------------------------------ regex

export interface RegexMatch {
  index: number;
  text: string;
  groups: (string | undefined)[];
  named: Record<string, string | undefined>;
}

export type RegexResult =
  { ok: true; matches: RegexMatch[]; truncated: boolean } | { ok: false; message: string };

export const MAX_REGEX_INPUT = 100_000;
export const MAX_REGEX_MATCHES = 500;

/**
 * Runs the browser's own `RegExp` (the ECMAScript flavour; not PCRE, not RE2: lookbehind and named groups work,
 * possessive quantifiers and recursion do not). The input is capped so a pathological pattern has a bounded
 * amount of text to chew on; a runaway pattern is still the page's to survive, which the tool says.
 */
export function runRegex(pattern: string, flags: string, text: string): RegexResult {
  if (text.length > MAX_REGEX_INPUT) return { ok: false, message: 'input-too-long' };
  if (!/^[dgimsuvy]*$/.test(flags) || new Set(flags).size !== flags.length) {
    return { ok: false, message: 'bad-flags' };
  }
  let re: RegExp;
  try {
    re = new RegExp(pattern, flags.includes('g') ? flags : `${flags}g`);
  } catch (e) {
    return { ok: false, message: e instanceof Error ? e.message : String(e) };
  }
  const matches: RegexMatch[] = [];
  let truncated = false;
  for (const m of text.matchAll(re)) {
    if (matches.length >= MAX_REGEX_MATCHES) {
      truncated = true;
      break;
    }
    matches.push({
      index: m.index ?? 0,
      text: m[0],
      groups: m.slice(1),
      named: { ...(m.groups ?? {}) },
    });
    // An empty match must not stall the iterator (matchAll already advances, this guards huge inputs).
  }
  return { ok: true, matches, truncated };
}
