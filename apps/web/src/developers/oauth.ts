// Pure helpers for OAuth2 scopes, redirect URIs, the authorize URL builder and the permission calculator.
// The permission catalog is the same one the server settings use (docs/07), so bit numbers never drift.
import { PERM, PERMISSIONS, toBig } from '../settings/server/permissions';
import { t } from '../i18n/runtime';
import type { IntentName } from './types';

export interface ScopeInfo {
  id: string;
  label: string;
  description: string;
  /** Needs the app to have a bot user. */
  bot?: boolean;
}

// The label is the scope's API name (what a developer types); the description is translated when read
// (SEC-AUD-040).
const scope = (id: string, descKey: string, bot?: boolean): ScopeInfo => ({
  id,
  label: id,
  get description() {
    return t(descKey);
  },
  ...(bot ? { bot } : {}),
});

export const SCOPES: ScopeInfo[] = [
  scope('identify', 'developers.scopes.identify'),
  scope('email', 'developers.scopes.email'),
  scope('guilds', 'developers.scopes.guilds'),
  scope('bot', 'developers.scopes.bot', true),
  scope('applications.commands', 'developers.scopes.commands'),
];

export const SCOPE_BY_ID = new Map(SCOPES.map((s) => [s.id, s]));

/** Plain-language line for one scope on the consent screen. */
export const scopeSentence = (scope: string): string => {
  switch (scope) {
    case 'identify':
      return t('developers.consent.identify');
    case 'email':
      return t('developers.consent.email');
    case 'guilds':
      return t('developers.consent.guilds');
    case 'bot':
      return t('developers.consent.bot');
    case 'applications.commands':
      return t('developers.consent.commands');
    default:
      return scope;
  }
};

const intent = (key: IntentName, name: string) => ({
  key,
  get label() {
    return t(`developers.intents.${name}.label`);
  },
  get description() {
    return t(`developers.intents.${name}.description`);
  },
});

export const INTENTS: { key: IntentName; label: string; description: string }[] = [
  // t('developers.intents.presences.label') t('developers.intents.presences.description')
  intent('GUILD_PRESENCES', 'presences'),
  // t('developers.intents.members.label') t('developers.intents.members.description')
  intent('GUILD_MEMBERS', 'members'),
  // t('developers.intents.messageContent.label') t('developers.intents.messageContent.description')
  intent('MESSAGE_CONTENT', 'messageContent'),
];

export const MAX_REDIRECTS = 10;
const BLOCKED_SCHEMES = new Set(['javascript:', 'data:', 'file:', 'vbscript:', 'blob:', 'about:']);

/** Error text for one redirect URI, or null when it is acceptable. */
export function redirectUriError(raw: string): string | null {
  const value = raw.trim();
  if (!value) return t('developers.urls.empty');
  if (/\s/.test(value)) return t('developers.urls.spaces');
  if (value.includes('*')) return t('developers.urls.wildcard');
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return t('developers.urls.notUrl');
  }
  if (url.protocol !== 'https:' && url.protocol !== 'http:') return t('developers.urls.scheme');
  if (url.hash) return t('developers.urls.fragment');
  if (url.username || url.password) return t('developers.urls.credentials');
  if (url.protocol === 'http:') {
    const host = url.hostname;
    if (!(host === 'localhost' || host === '127.0.0.1' || host === '[::1]'))
      return t('developers.urls.httpLocal');
  }
  if (value.length > 512) return t('developers.urls.tooLong', { max: 512 });
  return null;
}

/** Error text for a privacy policy / terms URL (https, 512 characters at most), or null. */
export function policyUrlError(raw: string): string | null {
  const value = raw.trim();
  if (!value) return null;
  if (value.length > 512) return t('developers.urls.tooLong', { max: 512 });
  try {
    return new URL(value).protocol === 'https:' ? null : t('developers.urls.httpsOnly');
  } catch {
    return t('developers.urls.notPolicyUrl');
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
