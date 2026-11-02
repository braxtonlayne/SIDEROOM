// Creator tip links (ARC-ROOM-001). The same rules as `crates/sr-core/src/tips.rs`, which is the authority: the
// server refuses anything else when a manifest, a cartridge or a profile Links block is saved. The web mirrors them
// so a link that slipped past (an older row, a hand-edited document) is never drawn as a badge or opened.

export const TIP_KINDS = ['btc', 'lightning', 'kofi', 'patreon'] as const;
export type TipKind = (typeof TIP_KINDS)[number];

export interface TipLink {
  kind: TipKind;
  url: string;
}

export const MAX_TIP_LINKS = 4;

/** Allow-listed hosts of the web kinds; an exact match, no subdomains. */
export const TIP_HOSTS: Readonly<Record<'kofi' | 'patreon', readonly string[]>> = {
  kofi: ['ko-fi.com', 'www.ko-fi.com'],
  patreon: ['patreon.com', 'www.patreon.com'],
};

const BASE58 = /^[13][1-9A-HJ-NP-Za-km-z]{25,34}$/;
const BECH32 = /^bc1[qpzry9x8gf2tvdw0s3jn54khce6mua7l]{11,71}$/;
const SEGMENT = /^[A-Za-z0-9._~%-]+$/;
const QUERY = /^[A-Za-z0-9=&._-]{1,60}$/;
const PLAIN_PARAM = /^[A-Za-z0-9._~%-]{1,80}$/;

export type TipProblem = 'kind' | 'long' | 'malformed' | 'host' | 'many' | 'duplicate';

const isKind = (k: string): k is TipKind => (TIP_KINDS as readonly string[]).includes(k);

const BECH32_CHARS = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l';
const BECH32_GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];

function bech32Polymod(values: number[]): number {
  let chk = 1;
  for (const v of values) {
    const top = chk >>> 25;
    chk = (((chk & 0x1ffffff) << 5) ^ v) >>> 0;
    for (let i = 0; i < 5; i++) if ((top >>> i) & 1) chk = (chk ^ BECH32_GEN[i]!) >>> 0;
  }
  return chk;
}

/**
 * FU-5: a `bc1` address must carry a valid checksum (bech32 for witness version 0, bech32m for 1 to 16), a
 * witness program of 2 to 40 bytes (20 or 32 for version 0) and clean padding; the same as `segwit_data_ok` in
 * `crates/sr-core/src/tips.rs`. A legacy base58 address is checked for shape only (its checksum needs SHA-256).
 */
function segwitOk(addr: string): boolean {
  const values = [...addr.slice(3)].map((c) => BECH32_CHARS.indexOf(c));
  if (values.length < 7 || values.includes(-1)) return false;
  const version = values[0]!;
  if (version > 16) return false;
  const want = version === 0 ? 1 : 0x2bc830a3;
  if (bech32Polymod([3, 3, 0, 2, 3, ...values]) !== want) return false;
  let acc = 0;
  let bits = 0;
  let len = 0;
  for (const v of values.slice(1, -6)) {
    acc = (acc << 5) | v;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      acc &= (1 << bits) - 1;
      len++;
    }
  }
  if (bits >= 5 || acc !== 0) return false;
  return version === 0 ? len === 20 || len === 32 : len >= 2 && len <= 40;
}

function bitcoinOk(v: string): boolean {
  if (!v.startsWith('bitcoin:')) return false;
  const rest = v.slice('bitcoin:'.length);
  const q = rest.indexOf('?');
  const addr = q < 0 ? rest : rest.slice(0, q);
  if (!BASE58.test(addr) && !(BECH32.test(addr) && segwitOk(addr))) return false;
  if (q < 0) return true;
  const query = rest.slice(q + 1);
  if (!query) return false;
  const seen = new Set<string>();
  for (const pair of query.split('&')) {
    const eq = pair.indexOf('=');
    if (eq < 0) return false;
    const key = pair.slice(0, eq);
    const val = pair.slice(eq + 1);
    if (!['amount', 'label', 'message'].includes(key) || seen.has(key)) return false;
    seen.add(key);
    if (!PLAIN_PARAM.test(val)) return false;
    if (key === 'amount' && !/^[0-9.]+$/.test(val)) return false;
  }
  return true;
}

const LIGHTNING_PREFIXES = ['lnbcrt', 'lnbc', 'lntbs', 'lntb', 'lnsb', 'lnurl1', 'lno1'];

function lightningOk(v: string): boolean {
  if (!v.startsWith('lightning:')) return false;
  const rest = v.slice('lightning:'.length);
  const prefix = LIGHTNING_PREFIXES.find((p) => rest.startsWith(p));
  if (!prefix) return false;
  const body = rest.slice(prefix.length);
  return body.length >= 20 && body.length <= 1100 && /^[a-z0-9]+$/.test(body);
}

function webProblem(kind: 'kofi' | 'patreon', v: string): TipProblem | null {
  if (!v.startsWith('https://')) return 'malformed';
  const rest = v.slice('https://'.length);
  const slash = rest.indexOf('/');
  const host = slash < 0 ? rest : rest.slice(0, slash);
  const tail = slash < 0 ? '' : rest.slice(slash);
  if (!host || !/^[a-z0-9.-]+$/.test(host)) return 'malformed';
  if (!TIP_HOSTS[kind].includes(host)) return 'host';
  const qi = tail.indexOf('?');
  const path = qi < 0 ? tail : tail.slice(0, qi);
  const query = qi < 0 ? null : tail.slice(qi + 1);
  const segs = path
    .split('/')
    .slice(1)
    .filter((s) => s !== '');
  if (segs.length === 0 || segs.length > 4 || path.includes('//')) return 'malformed';
  if (!segs.every((s) => s !== '.' && s !== '..' && SEGMENT.test(s))) return 'malformed';
  if (query !== null && !QUERY.test(query)) return 'malformed';
  return null;
}

/** What is wrong with one link, or `null` when it is acceptable. */
export function tipProblem(kind: string, url: string): TipProblem | null {
  if (!isKind(kind)) return 'kind';
  if (url.length > (kind === 'lightning' ? 1200 : 512)) return 'long';
  switch (kind) {
    case 'btc':
      return bitcoinOk(url) ? null : 'malformed';
    case 'lightning':
      return lightningOk(url) ? null : 'malformed';
    default:
      return webProblem(kind, url);
  }
}

/** The first problem of a list (at most four, one per kind, each valid), or `null`. */
export function tipsProblem(links: readonly { kind: string; url: string }[]): TipProblem | null {
  if (links.length > MAX_TIP_LINKS) return 'many';
  for (let i = 0; i < links.length; i++) {
    const l = links[i]!;
    const p = tipProblem(l.kind, l.url);
    if (p) return p;
    if (links.slice(0, i).some((o) => o.kind === l.kind)) return 'duplicate';
  }
  return null;
}

/** The links that pass, in order, at most one per kind and four in all: what a badge row may draw. */
export function safeTips(links: readonly { kind: string; url: string }[] | undefined): TipLink[] {
  const out: TipLink[] = [];
  for (const l of links ?? []) {
    if (out.length >= MAX_TIP_LINKS) break;
    if (tipProblem(l.kind, l.url) === null && !out.some((o) => o.kind === l.kind)) {
      out.push({ kind: l.kind as TipKind, url: l.url });
    }
  }
  return out;
}

/** Whether the kind is drawn as a QR code (a payment URI) rather than opened as a web link. */
export const isQrKind = (k: TipKind): boolean => k === 'btc' || k === 'lightning';
