// Route search parsing, kept apart from the page modules so the router can import it without pulling them in.

export const APP_TABS = [
  'general',
  'oauth2',
  'bot',
  'installation',
  'commands',
  'emojis',
  'richpresence',
  'activities',
  'review',
] as const;
export type AppTab = (typeof APP_TABS)[number];

export const parseTab = (v: unknown): AppTab =>
  typeof v === 'string' && (APP_TABS as readonly string[]).includes(v) ? (v as AppTab) : 'general';

export interface AuthorizeSearch {
  client_id?: string;
  scope?: string;
  redirect_uri?: string;
  response_type?: string;
  state?: string;
  permissions?: string;
  guild_id?: string;
  code_challenge?: string;
  code_challenge_method?: string;
}

const AUTHORIZE_KEYS = [
  'client_id',
  'scope',
  'redirect_uri',
  'response_type',
  'state',
  'permissions',
  'guild_id',
  'code_challenge',
  'code_challenge_method',
];

/** Router search validator: keep only known keys, as strings (the router parses digits into numbers). */
export function validateAuthorizeSearch(s: Record<string, unknown>): AuthorizeSearch {
  const out: Record<string, string> = {};
  for (const k of AUTHORIZE_KEYS) {
    const v = s[k];
    if (typeof v === 'string' || typeof v === 'number') out[k] = String(v);
  }
  return out as AuthorizeSearch;
}
