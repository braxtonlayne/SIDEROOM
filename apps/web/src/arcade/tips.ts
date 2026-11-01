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

function bitcoinOk(v: string): boolean {
  if (!v.startsWith('bitcoin:')) return false;
  const rest = v.slice('bitcoin:'.length);
  const q = rest.indexOf('?');
  const addr = q < 0 ? rest : rest.slice(0, q);
  if (!BASE58.test(addr) && !BECH32.test(addr)) return false;
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
