/**
 * The crash card allow-list on the client (OPS-CRASH-001, wave 45-A, threat model T45).
 *
 * The browser builds the report from nothing but the fields below and the server rebuilds the card again from its
 * own copy of the list (`crates/sr-api/src/routes/crash/allow_list.rs`), so a client that does not strip, or a
 * hostile one, changes nothing about what is stored. This file is the Rust file's twin: both are fed the hostile
 * fixture `e2e/fixtures/crash-hostile.json` and must build the same card from it.
 *
 * Nothing is copied by default. There is no field for a message body, a key, a URL, a token or an address; every
 * string that is kept comes from a closed set or a small character set and is tested once more by `suspicious`.
 */

export const MAX_FRAMES = 24;
import {
  MAX_TICKS,
  TICK_INPUTS,
  TICK_SOURCES,
  asTickInput,
  type CrashTick,
  type TickInput,
  type TickSource,
} from './enums';
export { MAX_TICKS, TICK_INPUTS, TICK_SOURCES, asTickInput };
export type { CrashTick, TickInput, TickSource };

export const KINDS = [
  'error',
  'unhandled_rejection',
  'react_boundary',
  'wasm_panic',
  'webgpu_panic',
] as const;
export type CrashKind = (typeof KINDS)[number];

export const CAPS = [
  'webgpu',
  'wasm',
  'wasm_simd',
  'shared_array_buffer',
  'cross_origin_isolated',
  'webrtc',
  'web_codecs',
  'service_worker',
  'offscreen_canvas',
  'web_audio',
  'indexed_db',
  'touch',
  'secure_context',
] as const;
export type CapName = (typeof CAPS)[number];

export const ENGINES = ['chromium', 'firefox', 'webkit', 'other'] as const;
export type Engine = (typeof ENGINES)[number];

export interface CrashFrame {
  file: string;
  line: number;
  col: number;
  fn?: string;
}

export interface CrashReport {
  v: 1;
  commit: string;
  kind: CrashKind;
  route: string;
  frames: CrashFrame[];
  caps: Partial<Record<CapName, boolean>>;
  engine: Engine;
  at: number;
  ticks: CrashTick[];
}

const isObject = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

const isCommit = (c: unknown): c is string => typeof c === 'string' && /^[0-9a-f]{7,40}$/.test(c);

// ---- strings ----------------------------------------------------------------------------------------------------

const TOKEN_PREFIXES = ['eyJ', 'AKIA', 'ASIA', 'ghp', 'gho', 'srg', 'xox', 'sk-'];

/** A piece of a name that looks like an identifier or a secret (the Rust `id_like_piece`). */
export function idLikePiece(p: string): boolean {
  if (!p) return false;
  if (/^[0-9]+$/.test(p) && p.length >= 5) return true;
  if (p.length >= 16 && /^[0-9a-fA-F]+$/.test(p)) return true;
  const hasDigit = /[0-9]/.test(p);
  const hasAlpha = /[A-Za-z]/.test(p);
  if (p.length >= 20 && hasDigit && hasAlpha) return true;
  return p.length >= 12 && hasDigit && TOKEN_PREFIXES.some((x) => p.startsWith(x));
}

/** The detector every kept string goes through once more. A deliberate over-approximation: a hit drops the string. */
export function suspicious(s: string): boolean {
  const l = s.toLowerCase();
  if (l.includes('x-amz-') || l.includes('signature=') || l.includes('begin private')) return true;
  if (s.includes('@') || s.includes('://')) return true;
  if (s.includes('eyJ') && s.length >= 20) return true;
  if (/(?:^|[^A-Za-z0-9])\d{1,3}(?:\.\d{1,3}){3}/.test(s)) return true;
  return s.split(/[^A-Za-z0-9]/).some(idLikePiece);
}

function cleanRoute(r: unknown): string | null {
  if (typeof r !== 'string' || !r || r.length > 64) return null;
  if (!/^[A-Za-z0-9._:-]+$/.test(r)) return null;
  if (r.split(/[._:-]/).some(idLikePiece) || suspicious(r)) return null;
  return r;
}

function cleanFunction(f: unknown): string | undefined {
  if (typeof f !== 'string' || !f || f.length > 80) return undefined;
  if (!/^[A-Za-z0-9_$.<>:[\]-]+$/.test(f)) return undefined;
  if (f.split(/[.\-_:<>[\]$]/).some(idLikePiece) || suspicious(f)) return undefined;
  return f;
}

const isUuid = (s: string) =>
  /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/.test(s);

function cleanSegment(s: string): string {
  if (s.length > 64 || !/^[A-Za-z0-9._-]+$/.test(s) || isUuid(s)) return ':id';
  return s
    .split(/([._-])/)
    .map((piece) => (idLikePiece(piece) ? ':id' : piece))
    .join('');
}

/**
 * A frame's file name without anything that identifies a person, a host or a session: no scheme or host, no query or
 * fragment, no drive letter or home folder, at most the last four path pieces, an id-like piece replaced by `:id`.
 * `wasm://wasm/<hash>` becomes `wasm`. Returns `null` for a name nothing is left of.
 */
export function normalizeFile(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed || trimmed.length > 512) return null;
  const q = trimmed.search(/[?#]/);
  let path = (q === -1 ? trimmed : trimmed.slice(0, q)).replace(/\\/g, '/');
  const scheme = path.indexOf('://');
  if (scheme !== -1) {
    if (path.slice(0, scheme).toLowerCase() === 'wasm') return 'wasm';
    const rest = path.slice(scheme + 3);
    const slash = rest.indexOf('/');
    if (slash === -1) return null;
    path = rest.slice(slash);
  }
  const raws = path.split('/').filter((s) => s && s !== '.' && s !== '..');
  if (!raws.length) return null;
  // A home folder's owner is a person: the piece after `Users` or `home` is never kept.
  const segs = raws.map((s, i) =>
    i > 0 && ['users', 'home'].includes((raws[i - 1] ?? '').toLowerCase()) ? ':id' : s,
  );
  const joined = segs.slice(-4).map(cleanSegment).join('/');
  return suspicious(joined) ? null : joined;
}

function cleanFrame(v: unknown): CrashFrame | null {
  if (!isObject(v) || typeof v.file !== 'string') return null;
  const file = normalizeFile(v.file);
  if (file === null) return null;
  const num = (x: unknown) =>
    typeof x === 'number' && Number.isInteger(x) && x >= 0 && x <= 100_000_000 ? x : 0;
  const fn = cleanFunction(v.fn);
  return { file, line: num(v.line), col: num(v.col), ...(fn ? { fn } : {}) };
}

// ---- caps and ticks ---------------------------------------------------------------------------------------------

function cleanCaps(v: unknown): { caps: Partial<Record<CapName, boolean>>; engine: Engine } {
  const caps: Partial<Record<CapName, boolean>> = {};
  let engine: Engine = 'other';
  if (isObject(v)) {
    for (const name of CAPS) {
      const b = v[name];
      if (typeof b === 'boolean') caps[name] = b;
    }
    if (typeof v.engine === 'string') engine = ENGINES.find((e) => e === v.engine) ?? 'other';
  }
  return { caps, engine };
}

function cleanTicks(v: unknown): CrashTick[] {
  if (!Array.isArray(v)) return [];
  const out: CrashTick[] = [];
  for (const t of v.slice(-MAX_TICKS)) {
    if (!isObject(t)) continue;
    const src = TICK_SOURCES.find((s) => s === t.src);
    if (!src) continue;
    const when =
      typeof t.t === 'number' && Number.isInteger(t.t) && t.t >= 0 && t.t < 2 ** 40 ? t.t : 0;
    out.push({ t: when, src, input: asTickInput(t.input) });
  }
  return out;
}

/**
 * Build the report from a raw value. Fields off the allow-list are not read at all. `null` when there is no valid
 * commit or kind (nothing worth sending).
 */
export function buildCrashCard(raw: unknown, nowMs: number): CrashReport | null {
  if (!isObject(raw) || !isCommit(raw.commit)) return null;
  const kind = KINDS.find((k) => k === raw.kind);
  if (!kind) return null;
  const frames = (Array.isArray(raw.frames) ? raw.frames : [])
    .map(cleanFrame)
    .filter((f): f is CrashFrame => f !== null)
    .slice(0, MAX_FRAMES);
  const { caps, engine } = cleanCaps(raw.caps);
  const at =
    typeof raw.at === 'number' && Number.isInteger(raw.at) && Math.abs(raw.at - nowMs) <= 864e5
      ? raw.at
      : nowMs;
  return {
    v: 1,
    commit: raw.commit,
    kind,
    route: cleanRoute(raw.route) ?? 'unknown',
    frames,
    caps,
    engine,
    at,
    ticks: cleanTicks(raw.ticks),
  };
}
