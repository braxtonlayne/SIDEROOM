// Data layer for the developer portal and the OAuth2 screens. Components never build URLs: they call these.
// Paths follow docs/specs/developer-platform.md section 3. Secrets (client secret, bot token) come back from
// create/reset calls only and must never be stored by callers beyond the reveal dialog.
import { ApiError, apiFetch, apiPost } from '../api/client';
import type {
  AppCreated,
  AppInstall,
  AppPatch,
  AppTester,
  AppVersion,
  ApplyBody,
  AuthorizePreview,
  AuthorizeResult,
  BotUser,
  DevApp,
  DeveloperRequest,
  DeveloperStatus,
  OAuthGrant,
} from './types';

const id = encodeURIComponent;
const ro = { retries: 0 } as const;

// ---- onboarding ----
export const getDeveloperStatus = () => apiFetch<DeveloperStatus>('/developers/status', ro);
export const applyAsDeveloper = (body: ApplyBody) =>
  apiPost<DeveloperRequest>('/developers/apply', body);

// ---- applications ----
export const listApps = () => apiFetch<DevApp[]>('/applications', ro);
export const getApp = (appId: string) => apiFetch<DevApp>(`/applications/${id(appId)}`, ro);
/** Creates the app together with its bot; the client secret and bot token are in this response only. */
export const createApp = (name: string, description?: string) =>
  apiPost<AppCreated>('/applications', { name, ...(description ? { description } : {}) });
export const patchApp = (appId: string, patch: AppPatch) =>
  apiFetch<DevApp>(`/applications/${id(appId)}`, { method: 'PATCH', json: patch, ...ro });
export const renameBot = (appId: string, username: string) =>
  apiFetch<BotUser>(`/applications/${id(appId)}/bot`, {
    method: 'PATCH',
    json: { username },
    ...ro,
  });

/** Re-confirms the account password, rotates the client secret and returns the new one (once). */
export const resetClientSecret = (appId: string, password: string) =>
  apiPost<{ client_secret: string }>(`/applications/${id(appId)}/reset-secret`, { password });
/** Re-confirms the account password, revokes the old bot token and returns the new one (once). */
export const resetBotToken = (appId: string, password: string) =>
  apiPost<{ token: string }>(`/applications/${id(appId)}/bot/reset-token`, { password });
export const deleteApp = (appId: string, password: string) =>
  apiFetch<void>(`/applications/${id(appId)}`, { method: 'DELETE', json: { password }, ...ro });

/** Creates the interactions signing key when the app has none; returns the public key. Idempotent. */
export const createInteractionsKey = (appId: string) =>
  apiPost<{ verify_key: string }>(`/applications/${id(appId)}/interactions-key`, {});

/** Runs the signed PING check against the saved URL and switches a disabled endpoint back on (D-186). */
export const verifyEndpoint = (appId: string) =>
  apiPost<{
    interactions_endpoint_url: string | null;
    interactions_endpoint_state: 'enabled' | 'disabled';
    verify_key: string | null;
  }>(`/applications/${id(appId)}/endpoint/verify`, {});

// ---- testers (D-204) ----
export const listTesters = (appId: string) =>
  apiFetch<AppTester[]>(`/applications/${id(appId)}/testers`, ro);
/** Invite by username; the person answers a system DM. */
export const inviteTester = (appId: string, username: string) =>
  apiPost<AppTester>(`/applications/${id(appId)}/testers`, { username });
export const removeTester = (appId: string, userId: string) =>
  apiFetch<void>(`/applications/${id(appId)}/testers/${id(userId)}`, { method: 'DELETE', ...ro });
/** The invitee's answer (the Accept and Decline buttons of the system DM). */
export const respondTesterInvite = (inviteId: string, accept: boolean) =>
  apiPost<{ application_id: string; name: string; accepted: boolean }>(
    `/developers/tester-invites/${id(inviteId)}/respond`,
    { accept },
  );

export const listInstalls = (appId: string) =>
  apiFetch<AppInstall[]>(`/applications/${id(appId)}/installs`, ro);

// ---- review ----
export const listVersions = (appId: string) =>
  apiFetch<AppVersion[]>(`/applications/${id(appId)}/versions`, ro);
export const submitVersion = (appId: string, note?: string) =>
  apiPost<AppVersion>(`/applications/${id(appId)}/versions`, note ? { note } : {});
export const withdrawVersion = (appId: string, versionId: string) =>
  apiPost<AppVersion>(`/applications/${id(appId)}/versions/${id(versionId)}/withdraw`);

// ---- OAuth2 consent ----
export interface AuthorizeQuery {
  client_id: string;
  scope: string;
  redirect_uri?: string;
  response_type?: string;
  state?: string;
  permissions?: string;
  guild_id?: string;
  code_challenge?: string;
  code_challenge_method?: string;
}
const query = (q: object) => {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(q as Record<string, unknown>))
    if (typeof v === 'string' && v !== '') p.set(k, v);
  return p.toString();
};
export const previewAuthorize = (q: AuthorizeQuery) =>
  apiFetch<AuthorizePreview>(`/oauth2/authorize?${query({ response_type: 'code', ...q })}`, ro);
export const submitAuthorize = (q: AuthorizeQuery & { authorize: boolean }) =>
  apiPost<AuthorizeResult>('/oauth2/authorize', { ...q, response_type: q.response_type ?? 'code' });

// ---- authorized apps ----
export const listGrants = () => apiFetch<OAuthGrant[]>('/oauth2/tokens', ro);
export const revokeGrant = (grantId: string) =>
  apiFetch<void>(`/oauth2/tokens/${id(grantId)}`, { method: 'DELETE', ...ro });

export const keys = {
  apps: ['dev', 'apps'] as const,
  app: (appId: string) => ['dev', 'app', appId] as const,
  installs: (appId: string) => ['dev', 'installs', appId] as const,
  testers: (appId: string) => ['dev', 'testers', appId] as const,
  versions: (appId: string) => ['dev', 'versions', appId] as const,
  status: ['dev', 'status'] as const,
};

/** True when the server does not have this endpoint (yet): show an honest "not available" state. */
export const isUnavailable = (e: unknown) =>
  e instanceof ApiError && (e.status === 404 || e.status === 405 || e.status === 501);

/** 900004: the password re-confirmation was wrong. */
export const isWrongPassword = (e: unknown) => e instanceof ApiError && e.body?.code === 900004;

export function errorText(e: unknown, fallback = 'Something went wrong. Try again.'): string {
  if (e instanceof ApiError) {
    if (e.status === 429)
      return e.retryAfter
        ? `You are doing that too fast. Try again in ${Math.ceil(e.retryAfter)}s.`
        : 'You are doing that too fast. Try again in a moment.';
    const fieldMsgs = Object.values(e.body?.errors ?? {}).flatMap((f) =>
      (f._errors ?? []).map((x) => x.message),
    );
    if (fieldMsgs.length) return fieldMsgs.join(' ');
    if (e.message) return e.message;
  }
  return fallback;
}
