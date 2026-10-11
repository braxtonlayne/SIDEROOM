// Typed Entrance calls (docs 05/06). Auth endpoints under /sr/auth/* are assumptions: see docs/open-questions.md.
import { apiFetch, apiPost } from './client';

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
}
export const register = (req: RegisterRequest) => apiPost<{ id: string }>('/sr/register', req);

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

export type LoginResult =
  { status: 'ok' } | { status: 'mfa_required'; methods: ('totp' | 'webauthn' | 'recovery')[] };
export const login = (req: { login: string; password: string }) =>
  apiPost<LoginResult>('/sr/auth/login', req);

export interface TwoFactorRequest {
  method: 'totp' | 'recovery';
  code: string;
  trust_device: boolean;
}
export const submitTwoFactor = (req: TwoFactorRequest) =>
  apiPost<{ status: 'ok' }>('/sr/auth/2fa', req);

export const forgotPassword = (email: string) => apiPost<void>('/sr/auth/forgot', { email });
export const resetPassword = (token: string, password: string) =>
  apiPost<void>('/sr/auth/reset', { token, password });

/** Passkey ceremony: server issues options (JSON form), the browser signs, server verifies. */
export const passkeyOptions = () => apiPost<unknown>('/sr/auth/passkey/options');
export const passkeyVerify = (credential: unknown) =>
  apiPost<{ status: 'ok' }>('/sr/auth/passkey/verify', credential);

export const getMe = () => apiFetch<{ id: string }>('/users/@me', { retries: 0 });

export interface ServerInvitePreview {
  code: string;
  guild: { id: string; name: string; icon: string | null };
  approximate_member_count: number;
  approximate_presence_count: number;
  inviter?: { username: string; avatar: string | null };
}
export const getServerInvite = (code: string) =>
  apiFetch<ServerInvitePreview>(`/invites/${encodeURIComponent(code)}?with_counts=true`, {
    retries: 0,
  });
