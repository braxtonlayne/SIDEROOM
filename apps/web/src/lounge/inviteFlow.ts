/** Shared helpers for the invite flows (accept page, Join a Server, Invite people). */
import { ApiError } from '../api/client';

/**
 * Pulls an invite code out of whatever the person pasted: a bare code, `host/invite/CODE`,
 * `https://host/invite/CODE?x=y`, or `host/join/CODE`. Returns null when nothing code-shaped is found.
 */
export function parseInviteCode(input: string): string | null {
  const text = input.trim();
  if (!text) return null;
  let candidate = text;
  const m = /\/(?:invite|join)\/([^/?#\s]+)/i.exec(text);
  if (m?.[1]) candidate = m[1];
  else if (/[/:]/.test(text)) {
    // A URL without an /invite/ segment: use its last path segment.
    candidate = text.split(/[?#]/)[0]!.replace(/\/+$/, '').split('/').pop() ?? '';
  }
  return /^[A-Za-z0-9-]{2,32}$/.test(candidate) ? candidate : null;
}

/** The shareable link for an invite code. */
export const inviteUrl = (code: string) => `${globalThis.location?.origin ?? ''}/invite/${code}`;

export const UNKNOWN_INVITE = 10006;
export const USER_BANNED = 40007;

/** User-facing copy for a failed resolve/accept call. */
export function inviteErrorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    const code = e.body?.code;
    if (e.status === 404 || code === UNKNOWN_INVITE)
      return 'This invite is invalid or has expired.';
    if (code === USER_BANNED) return 'You are banned from this server.';
    if (e.status === 403) return 'Invites to this server are paused right now. Try again later.';
    if (e.status === 429) return 'Too many tries. Take a breath and try again in a moment.';
    return e.message || 'Something went wrong. Try again.';
  }
  return 'Could not reach the server. Check your connection and try again.';
}

export const EXPIRY_OPTIONS: { label: string; seconds: number }[] = [
  { label: '30 minutes', seconds: 1800 },
  { label: '1 hour', seconds: 3600 },
  { label: '6 hours', seconds: 21600 },
  { label: '12 hours', seconds: 43200 },
  { label: '1 day', seconds: 86400 },
  { label: '7 days', seconds: 604800 },
  { label: 'Never', seconds: 0 },
];

export const USES_OPTIONS: { label: string; uses: number }[] = [
  { label: 'No limit', uses: 0 },
  { label: '1 use', uses: 1 },
  { label: '5 uses', uses: 5 },
  { label: '10 uses', uses: 10 },
  { label: '25 uses', uses: 25 },
  { label: '50 uses', uses: 50 },
  { label: '100 uses', uses: 100 },
];

/** "Expires in 3 hours" / "Never expires" for an invite row. */
export function expiryLabel(expiresAt: string | null | undefined, now = Date.now()): string {
  if (!expiresAt) return 'Never expires';
  const ms = Date.parse(expiresAt) - now;
  if (!Number.isFinite(ms)) return '';
  if (ms <= 0) return 'Expired';
  const mins = Math.round(ms / 60000);
  if (mins < 60) return `Expires in ${Math.max(1, mins)} min`;
  const hours = Math.round(mins / 60);
  if (hours < 48) return `Expires in ${hours} h`;
  return `Expires in ${Math.round(hours / 24)} d`;
}

/** Discord-style channel name: lowercase, spaces to hyphens, junk dropped. */
export function slugChannelName(raw: string): string {
  return raw
    .trim()
    .toLowerCase()
    .replace(/\s+/g, '-')
    .replace(/[^\p{L}\p{N}\-_]/gu, '')
    .replace(/-{2,}/g, '-')
    .slice(0, 100);
}

// ---- message search helpers ----

export interface Segment {
  text: string;
  hit: boolean;
}

/** Splits `text` by server highlight ranges, which are UTF-8 byte offsets. */
export function highlightSegments(text: string, ranges: readonly (readonly [number, number])[]) {
  const bytes = new TextEncoder().encode(text);
  const dec = new TextDecoder();
  const out: Segment[] = [];
  let at = 0;
  for (const [s, e] of [...ranges].sort((a, b) => a[0] - b[0])) {
    if (s < at || e > bytes.length || e <= s) continue;
    if (s > at) out.push({ text: dec.decode(bytes.slice(at, s)), hit: false });
    out.push({ text: dec.decode(bytes.slice(s, e)), hit: true });
    at = e;
  }
  if (at < bytes.length) out.push({ text: dec.decode(bytes.slice(at)), hit: false });
  return out;
}

export interface SearchLookups {
  users: { id: string; names: string[] }[];
  channels: { id: string; name: string }[];
}

/**
 * Turns the search-box text into the server's `q`: `from:name` and `in:#name` become ids (the server's
 * parser takes ids only). Returns an error message when a name does not resolve.
 */
export function buildSearchQuery(
  raw: string,
  lookups: SearchLookups,
): { q: string } | { error: string } {
  const parts: string[] = [];
  const re = /(\S*"[^"]*"\S*|\S+)/g;
  for (const tok of raw.match(re) ?? []) {
    const m = /^(from|in):(.*)$/i.exec(tok);
    if (!m) {
      parts.push(tok);
      continue;
    }
    const key = m[1]!.toLowerCase();
    const value = m[2]!.replace(/^[@#]/, '').toLowerCase();
    if (/^\d+$/.test(value)) {
      parts.push(`${key}:${value}`);
      continue;
    }
    if (!value) return { error: `Add a ${key === 'from' ? 'person' : 'channel'} after ${key}:` };
    if (key === 'from') {
      const u = lookups.users.find((x) => x.names.some((n) => n.toLowerCase() === value));
      if (!u) return { error: `No one called "${m[2]}" found.` };
      parts.push(`from:${u.id}`);
    } else {
      const c = lookups.channels.find((x) => x.name.toLowerCase() === value);
      if (!c) return { error: `No channel called "${m[2]}" found.` };
      parts.push(`in:${c.id}`);
    }
  }
  return { q: parts.join(' ') };
}
