// Pure helpers for OAuth2 scopes, redirect URIs, the authorize URL builder and the permission calculator.
// The permission catalog is the same one the server settings use (docs/07), so bit numbers never drift.
import { PERM, PERMISSIONS, toBig } from '../settings/server/permissions';
import type { IntentName } from './types';

export interface ScopeInfo {
  id: string;
  label: string;
  description: string;
  /** Needs the app to have a bot user. */
  bot?: boolean;
}

export const SCOPES: ScopeInfo[] = [
  { id: 'identify', label: 'identify', description: 'Read the account name, avatar and id.' },
  { id: 'email', label: 'email', description: 'Read the account email address.' },
  { id: 'guilds', label: 'guilds', description: 'List the servers the account is in.' },
  { id: 'bot', label: 'bot', description: 'Add the app bot user to a server.', bot: true },
  {
    id: 'applications.commands',
    label: 'applications.commands',
    description: 'Let the app register slash commands in a server.',
  },
];

export const SCOPE_BY_ID = new Map(SCOPES.map((s) => [s.id, s]));

/** Plain-language line for one scope on the consent screen. */
export const scopeSentence = (scope: string): string => {
  switch (scope) {
    case 'identify':
      return 'Access your username, avatar and account id';
    case 'email':
      return 'Access your email address';
    case 'guilds':
      return 'Know which servers you are in';
    case 'bot':
      return 'Add a bot to a server';
    case 'applications.commands':
      return 'Create slash commands in a server';
    default:
      return scope;
  }
};

export const INTENTS: { key: IntentName; label: string; description: string }[] = [
  {
    key: 'GUILD_PRESENCES',
    label: 'Presence intent',
    description: 'Receive presence updates (online status and activities).',
  },
  {
    key: 'GUILD_MEMBERS',
    label: 'Server members intent',
    description: 'Receive member join, leave and update events and list members.',
  },
  {
    key: 'MESSAGE_CONTENT',
    label: 'Message content intent',
    description: 'Read the content of messages that do not mention or reply to the bot.',
  },
];

export const MAX_REDIRECTS = 10;
const BLOCKED_SCHEMES = new Set(['javascript:', 'data:', 'file:', 'vbscript:', 'blob:', 'about:']);

/** Error text for one redirect URI, or null when it is acceptable. */
export function redirectUriError(raw: string): string | null {
  const value = raw.trim();
  if (!value) return 'Enter a URL.';
  if (/\s/.test(value)) return 'Redirect URIs cannot contain spaces.';
  if (value.includes('*')) return 'Wildcards are not allowed. Register each exact URI.';
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return 'Enter a full URL, like https://example.com/callback.';
  }
  if (url.protocol !== 'https:' && url.protocol !== 'http:')
    return 'Only https URLs are allowed (http only for localhost).';
  if (url.hash) return 'Redirect URIs cannot have a #fragment.';
  if (url.username || url.password) return 'Redirect URIs cannot contain credentials.';
  if (url.protocol === 'http:') {
    const host = url.hostname;
    if (!(host === 'localhost' || host === '127.0.0.1' || host === '[::1]'))
      return 'Use https. Plain http is only allowed for localhost.';
  }
  if (value.length > 512) return 'That URL is too long (512 characters at most).';
  return null;
}

/** Error text for a privacy policy / terms URL (https, 512 characters at most), or null. */
export function policyUrlError(raw: string): string | null {
  const value = raw.trim();
  if (!value) return null;
  if (value.length > 512) return 'That URL is too long (512 characters at most).';
  try {
    return new URL(value).protocol === 'https:' ? null : 'Use an https URL.';
  } catch {
    return 'Enter a full URL, like https://example.com/privacy.';
  }
}

export interface AuthorizeUrlOptions {
  origin: string;
  clientId: string;
  scopes: string[];
  redirectUri?: string;
  permissions?: bigint;
  responseType?: 'code';
  state?: string;
}

/** The link a person opens to authorize the app (`/oauth2/authorize?...`). */
export function buildAuthorizeUrl(o: AuthorizeUrlOptions): string {
  const p = new URLSearchParams();
  p.set('client_id', o.clientId);
  if (o.scopes.length) p.set('scope', o.scopes.join(' '));
  if (o.redirectUri) {
    p.set('redirect_uri', o.redirectUri);
    p.set('response_type', o.responseType ?? 'code');
  }
  if (o.scopes.includes('bot') && o.permissions !== undefined && o.permissions > 0n)
    p.set('permissions', o.permissions.toString());
  if (o.state) p.set('state', o.state);
  return `${o.origin.replace(/\/$/, '')}/oauth2/authorize?${p.toString()}`;
}

/** Permission ids -> decimal bitfield. */
export function permissionBits(ids: Iterable<string>): bigint {
  let bits = 0n;
  for (const x of ids) bits |= PERM[x] ?? 0n;
  return bits;
}

/** Permission ids set in a (possibly string) bitfield. */
export function permissionIds(bits: string | bigint | null | undefined): string[] {
  const b = typeof bits === 'bigint' ? bits : toBig(bits);
  return PERMISSIONS.filter((x) => (b & PERM[x.id]!) === PERM[x.id]!).map((x) => x.id);
}

/** Permission labels set in a bitfield, for the consent screen. */
export function permissionLabels(bits: string | bigint | null | undefined): string[] {
  const set = new Set(permissionIds(bits));
  return PERMISSIONS.filter((x) => set.has(x.id)).map((x) => x.label);
}

/** Space-separated, de-duplicated, known-first scope list from a query value. */
export function parseScopes(raw: string | undefined): string[] {
  return [...new Set((raw ?? '').split(/[\s,+]+/).filter(Boolean))];
}

/** Does `location` (from the server) go to the redirect URI we were asked about? Guards open redirects. */
export function isSafeRedirect(location: string, redirectUri: string | null): boolean {
  try {
    const to = new URL(location);
    if (BLOCKED_SCHEMES.has(to.protocol)) return false;
    if (!redirectUri) return to.protocol === 'https:' || to.protocol === 'http:';
    const base = new URL(redirectUri);
    return to.origin === base.origin && to.pathname === base.pathname;
  } catch {
    return false;
  }
}

/** Browser navigation to another origin; a seam so tests can observe it. */
export const external = {
  go: (url: string) => window.location.assign(url),
};
