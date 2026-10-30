// Typed Entrance calls (docs 05/06). Endpoint paths and shapes are documented in docs/05-api-and-gateway.md section 5.1; passkey endpoints are in ./passkeys.ts.
import { apiFetch, apiPost, refreshCsrf } from './client';

export type InviteCheck = { valid: true } | { valid: false; reason?: 'closed' };
export const checkInvite = (code: string) =>
  apiFetch<InviteCheck>(`/sr/invites/${encodeURIComponent(code)}`, { retries: 0 });

export const checkUsername = (username: string) =>
  apiFetch<{ available: boolean }>(`/sr/usernames/check?username=${encodeURIComponent(username)}`, {
    retries: 0,
  });

export interface RegisterRequest {
  code: string;
  email: string;
  username: string;
  display_name?: string;
  password: string;
  accepted_house_notes: true;
  /** The minimum age the person affirmed ("I am at least {age}"); the server refuses less than its own minimum.
   * Omitted when the age box is not ticked (D-311); the server then answers 400 `errors.age`. */
  confirmed_min_age?: number;
}
/** Registration signs the new account in (cookie), so the CSRF token is reloaded afterwards. */
export const register = async (req: RegisterRequest) => {
  const res = await apiPost<{ id: string }>('/sr/register', req);
  await refreshCsrf();
  return res;
};

export interface DiscordLink {
  id: string;
  username: string;
  email?: string;
}
export const getPendingDiscord = () =>
  apiFetch<DiscordLink | null>('/sr/auth/discord/pending', { retries: 0 });
export const unlinkPendingDiscord = () =>
  apiFetch<void>('/sr/auth/discord/pending', { method: 'DELETE', retries: 0 });
/** Full-page redirect target that starts the Discord OAuth round trip. */
export const discordStartUrl = (intent: 'login' | 'register', next?: string) =>
  `/api/v1/sr/auth/discord/start?intent=${intent}${next ? `&next=${encodeURIComponent(next)}` : ''}`;

/** Methods the server offers after a password check (doc 05 section 5.1). */
export type MfaMethod = 'totp' | 'passkey' | 'recovery';
export type LoginResult = { status: 'ok' } | { status: 'mfa_required'; methods: MfaMethod[] };
export const login = async (req: { login: string; password: string }) => {
  const res = await apiPost<LoginResult>('/sr/auth/login', req);
  if (res.status === 'ok') await refreshCsrf();
  return res;
};

/**
 * Owner one-time login link (open question 43): `POST /sr/auth/login-link {token}` exchanges the token for
 * a session cookie. The token is single use and lasts an hour; it never resets a password.
 */
export const loginWithLink = async (token: string) => {
  const res = await apiPost<{ status: 'ok' }>('/sr/auth/login-link', { token });
  await refreshCsrf();
  return res;
};

export interface TwoFactorRequest {
  method: 'totp' | 'recovery' | 'passkey';
  /** For `passkey`: the JSON text of `{ceremony_id, credential}` (see PasskeyButton). */
  code: string;
  trust_device: boolean;
}
export const submitTwoFactor = async (req: TwoFactorRequest) => {
  const res = await apiPost<{ status: 'ok' }>('/sr/auth/2fa', req);
  await refreshCsrf();
  return res;
};

export const forgotPassword = (email: string) => apiPost<void>('/sr/auth/forgot', { email });
export const resetPassword = (token: string, password: string) =>
  apiPost<void>('/sr/auth/reset', { token, password });

/** ENT-UI-020: `POST /sr/auth/verify-email` (204, or 400 with `errors.token`). */
export const verifyEmail = (token: string) => apiPost<void>('/sr/auth/verify-email', { token });
/** Banner resend (session + CSRF). Server answers 429 when asked more than once a minute. */
/** `POST /sr/auth/email/cancel` (204, or 400 `errors.token`): the "This wasn't me" link of an email change. */
export const cancelEmailChange = (token: string) =>
  apiPost<void>('/sr/auth/email/cancel', { token });
export const resendVerification = () => apiPost<void>('/sr/auth/verify-email/resend');

export const getMe = () => apiFetch<{ id: string }>('/users/@me', { retries: 0 });

export interface ServerInvitePreview {
  code: string;
  /** 0 = server invite, 1 = group DM invite. Switch on this, never on `guild` (D-221). */
  type: 0 | 1;
  /** `null` on a group DM invite, which names the group in `channel` instead. */
  guild: { id: string; name: string; icon: string | null } | null;
  channel?: { id: string; name?: string | null; type: number; icon?: string | null };
  approximate_member_count: number;
  approximate_presence_count: number;
  inviter?: { username: string; avatar: string | null };
}
export const getServerInvite = (code: string) =>
  apiFetch<ServerInvitePreview>(`/invites/${encodeURIComponent(code)}?with_counts=true`, {
    retries: 0,
  });

/** The ban screen: the credentials prove the (session-less) banned account. `POST /moderation/appeals`. */
export const fileAppeal = (body: { login: string; password: string; message: string }) =>
  apiPost<void>('/moderation/appeals', body);
