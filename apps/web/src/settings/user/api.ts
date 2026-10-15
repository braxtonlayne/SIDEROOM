// REST calls for User Settings (LNG-USET-*). Paths and shapes come from crates/sr-api routes
// (auth/{sessions,trusted,mfa,recovery,account}.rs, social/{profile,relationships,notifications,presence}.rs,
// openapi.rs). Thin wrappers over apiFetch; mutations pass retries: 0 so lockouts surface.
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError, apiFetch, apiPost, clearCsrf, type ApiErrorBody } from '../../api/client';

// ---- account ----

export interface Me {
  id: string;
  username?: string;
  global_name?: string | null;
  display_name?: string | null;
  /** CDN hashes (`/cdn/avatars/{id}/{hash}.png`). */
  avatar?: string | null;
  banner?: string | null;
  email?: string;
  verified?: boolean;
  mfa_enabled?: boolean;
  locale?: string;
}
export const getMe = () => apiFetch<Me>('/users/@me', { retries: 0 });

export interface SessionRow {
  id: string;
  device_name: string | null;
  created_at: string;
  last_seen_at: string;
  expires_at: string;
  current: boolean;
}
export const listSessions = () => apiFetch<SessionRow[]>('/sr/auth/sessions', { retries: 0 });
export const revokeSession = (id: string) =>
  apiFetch<void>(`/sr/auth/sessions/${encodeURIComponent(id)}`, { method: 'DELETE', retries: 0 });
export const logout = async () => {
  await apiPost<void>('/sr/auth/logout');
  clearCsrf();
};
/** Revokes every session, this one included. */
export const logoutAll = async () => {
  await apiPost<void>('/sr/auth/logout-all');
  clearCsrf();
};

export interface TrustedDevice {
  id: string;
  device_name: string | null;
  created_at: string;
  expires_at: string;
}
export const listTrustedDevices = () =>
  apiFetch<TrustedDevice[]>('/sr/auth/trusted-devices', { retries: 0 });
export const revokeTrustedDevice = (id: string) =>
  apiFetch<void>(`/sr/auth/trusted-devices/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    retries: 0,
  });

export const resendVerification = () => apiPost<void>('/sr/auth/verify-email/resend');
/** The emailed reset link (for people who forgot the current password). */
export const sendPasswordReset = (email: string) => apiPost<void>('/sr/auth/forgot', { email });

export interface ChangePasswordBody {
  current_password: string;
  new_password: string;
  /** Default true: every other session and trusted device is signed out. */
  revoke_other_sessions?: boolean;
}
/** `POST /sr/auth/password` (204). The current password is checked first and counts toward the lockout. */
export const changePassword = (body: ChangePasswordBody) =>
  apiPost<void>('/sr/auth/password', body);
/** `PUT /sr/auth/username`: password re-auth, 2 to 32 of a-z 0-9 _ . and two changes a day. */
export const changeUsername = (username: string, password: string) =>
  apiFetch<{ username: string }>('/sr/auth/username', {
    method: 'PUT',
    json: { username, password },
    retries: 0,
  });

export interface ChangeEmailBody {
  new_email: string;
  password: string;
  /** Required when two-factor is on, unless this session passed a check in the last 15 minutes. */
  mfa?: { method: 'totp' | 'recovery'; code: string };
}
/** `POST /sr/auth/email/change`: mails a confirmation link to the new address (204). */
export const changeEmail = (body: ChangeEmailBody) => apiPost<void>('/sr/auth/email/change', body);

export interface TotpEnrollment {
  otpauth_uri: string;
  manual_entry_key: string;
}
export const totpEnroll = () => apiPost<TotpEnrollment>('/sr/auth/totp/enroll');
export const totpVerify = (code: string) =>
  apiPost<{ recovery_codes: string[] }>('/sr/auth/totp/verify', { code });
export const totpDisable = (password: string) =>
  apiFetch<void>('/sr/auth/totp', { method: 'DELETE', json: { password }, retries: 0 });
export const regenerateRecoveryCodes = (password: string) =>
  apiPost<{ recovery_codes: string[] }>('/sr/auth/recovery-codes', { password });

export interface Connection {
  type: string;
  id: string;
  name: string;
  verified: boolean;
}

// ---- profile ----

export interface CustomStatus {
  text: string | null;
  emoji_name: string | null;
  emoji_id: string | null;
  expires_at: string | null;
}
export interface OwnProfile {
  user: {
    id: string;
    username?: string;
    global_name?: string | null;
    avatar?: string | null;
    banner?: string | null;
  };
  user_profile: {
    bio: string | null;
    pronouns: string | null;
    banner: string | null;
    accent_color: number | null;
    profile_effect?: string | null;
  };
  connected_accounts: Connection[];
}
export const getOwnProfile = () => apiFetch<OwnProfile>('/users/@me/profile', { retries: 0 });
export const getPresence = () =>
  apiFetch<{ status: string; custom_status: CustomStatus | null }>('/users/@me/presence', {
    retries: 0,
  });

export interface ProfilePatch {
  /** Display name (32 characters at most); null shows the username. */
  global_name?: string | null;
  /** An image data URI (png, jpeg, gif or webp; 8 MiB at most); null removes it. */
  avatar?: string | null;
  banner?: string | null;
  bio?: string | null;
  pronouns?: string | null;
  accent_color?: number | null;
  custom_status?: { text: string | null } | null;
  /** A slug from `GET /sr/profile-catalog`; null removes the effect. */
  profile_effect?: string | null;
}
export interface ProfileCatalog {
  effects: { slug: string; label_key: string }[];
  decorations: { slug: string; asset?: string | null; label_key: string }[];
}
export const getProfileCatalog = () =>
  apiFetch<ProfileCatalog>('/sr/profile-catalog', { retries: 0 });
export const patchSelf = (body: ProfilePatch) =>
  apiFetch<{ user?: OwnProfile['user'] }>('/users/@me', {
    method: 'PATCH',
    json: body,
    retries: 0,
  });

// ---- notifications ----

export interface MuteConfig {
  end_time: string | null;
  selected_time_window: number | null;
}
export interface GuildSettings {
  guild_id: string | null;
  /** 0 all messages, 1 only mentions, 2 nothing, null inherits. */
  message_notifications: number | null;
  muted: boolean;
  mute_config: MuteConfig | null;
  suppress_everyone: boolean;
  suppress_roles: boolean;
  suppress_highlights: boolean;
  mute_scheduled_events: boolean;
  mobile_push: boolean;
}
export interface GuildLite {
  id: string;
  name: string;
}
export const listGuilds = () => apiFetch<GuildLite[]>('/users/@me/guilds', { retries: 0 });
export const getGuildSettings = (guildId: string) =>
  apiFetch<GuildSettings>(`/users/@me/guilds/${guildId}/settings`, { retries: 0 });
export interface GuildSettingsPatch {
  message_notifications?: number | null;
  muted?: boolean;
  mute_config?: { selected_time_window: number };
  suppress_everyone?: boolean;
  suppress_roles?: boolean;
}
export const patchGuildSettings = (guildId: string, body: GuildSettingsPatch) =>
  apiFetch<GuildSettings>(`/users/@me/guilds/${guildId}/settings`, {
    method: 'PATCH',
    json: body,
    retries: 0,
  });

// ---- privacy & safety ----

export interface UserLite {
  id: string;
  username?: string;
  global_name?: string | null;
  avatar?: string | null;
}
export interface Relationship {
  id: string;
  /** 1 friend, 2 blocked, 3 incoming request, 4 outgoing request. */
  type: number;
  nickname: string | null;
  since: string;
  user: UserLite;
}
export const listRelationships = () =>
  apiFetch<Relationship[]>('/users/@me/relationships', { retries: 0 });
/** Removes a block (the server unblocks when the row is a block). */
export const unblockUser = (id: string) =>
  apiFetch<void>(`/users/@me/relationships/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    retries: 0,
  });
export const listIgnores = () => apiFetch<UserLite[]>('/users/@me/ignores', { retries: 0 });
export const unignoreUser = (id: string) =>
  apiFetch<void>(`/users/@me/ignores/${encodeURIComponent(id)}`, { method: 'DELETE', retries: 0 });

/** `GET/PATCH /users/@me/privacy`: who may send friend requests and DMs. */
export interface PrivacySettings {
  friend_requests_everyone: boolean;
  friend_requests_friends_of_friends: boolean;
  friend_requests_server_members: boolean;
  /** DMs from members of shared servers. */
  allow_server_member_dms: boolean;
  /** DMs from non-friends land in Message Requests. */
  message_requests: boolean;
}
export const getPrivacy = () => apiFetch<PrivacySettings>('/users/@me/privacy', { retries: 0 });
export const patchPrivacy = (body: Partial<PrivacySettings>) =>
  apiFetch<PrivacySettings>('/users/@me/privacy', { method: 'PATCH', json: body, retries: 0 });

/** `GET/PATCH /users/@me/notification-defaults`: 0 all messages, 1 only mentions, 2 nothing, null per server. */
export interface NotificationDefaults {
  message_notifications: number | null;
}
export const getNotificationDefaults = () =>
  apiFetch<NotificationDefaults>('/users/@me/notification-defaults', { retries: 0 });
export const patchNotificationDefaults = (message_notifications: number | null) =>
  apiFetch<NotificationDefaults>('/users/@me/notification-defaults', {
    method: 'PATCH',
    json: { message_notifications },
    retries: 0,
  });

// ---- per-server profile ----

export interface MemberLite {
  nick: string | null;
}
export const getMyMember = (guildId: string, userId: string) =>
  apiFetch<MemberLite>(`/guilds/${guildId}/members/${encodeURIComponent(userId)}`, { retries: 0 });
/** The server nickname (needs Change Nickname there); null clears it. */
export const patchMyNick = (guildId: string, userId: string, nick: string | null) =>
  apiFetch<MemberLite>(`/guilds/${guildId}/members/${encodeURIComponent(userId)}`, {
    method: 'PATCH',
    json: { nick },
    retries: 0,
  });

// ---- helpers ----

/** First field-level message in a Discord-style error tree, else the top-level message. */
export function errorText(e: unknown, fallback = 'Something went wrong. Try again.'): string {
  if (!(e instanceof ApiError)) return fallback;
  if (e.status === 429) {
    const s = e.retryAfter;
    return s
      ? `Too many tries. Wait ${Math.max(1, Math.ceil(s / 60))} minute(s).`
      : 'Too many tries.';
  }
  return (
    firstFieldError(e.body) ?? (e.message && e.message !== 'Bad Request' ? e.message : fallback)
  );
}
export function firstFieldError(body: ApiErrorBody | null): string | null {
  for (const f of Object.values(body?.errors ?? {})) {
    const m = f._errors?.[0]?.message;
    if (m) return m;
  }
  return null;
}

export interface Resource<T> {
  data: T | undefined;
  error: unknown;
  loading: boolean;
  reload: () => void;
  setData: (v: T | ((prev: T | undefined) => T | undefined)) => void;
}

/** Minimal fetch-on-mount hook (the app has no data-fetching layer wired in tests). */
export function useResource<T>(load: () => Promise<T>): Resource<T> {
  const [data, setData] = useState<T | undefined>(undefined);
  const [error, setError] = useState<unknown>(undefined);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);
  const loadRef = useRef(load);
  loadRef.current = load;
  useEffect(() => {
    let live = true;
    setLoading(true);
    loadRef
      .current()
      .then((v) => {
        if (live) {
          setData(v);
          setError(undefined);
        }
      })
      .catch((e: unknown) => live && setError(e))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [tick]);
  const reload = useCallback(() => setTick((t) => t + 1), []);
  return { data, error, loading, reload, setData };
}

// ---- server tag adoption (LNG-SSET-012) ----

export interface PrimaryGuild {
  identity_guild_id: string;
  identity_enabled: boolean;
  tag: string;
  badge: number;
  color: number;
}
export interface GuildTagChoice {
  guildId: string;
  guildName: string;
  tag: string;
  badge: number;
  color: number;
}
export const getPrimaryGuild = () =>
  apiFetch<{ primary_guild?: PrimaryGuild | null }>('/users/@me/primary-guild', {
    retries: 0,
  }).then((r) => r.primary_guild ?? null);
/** \`null\` stops displaying a tag. */
export const putPrimaryGuild = (guildId: string | null) =>
  apiFetch<{ primary_guild?: PrimaryGuild | null }>('/users/@me/primary-guild', {
    method: 'PUT',
    retries: 0,
    json: { guild_id: guildId },
  });
/** Tags of the signed-in person's servers that members may display. */
export async function listGuildTags(): Promise<GuildTagChoice[]> {
  const guilds = await listGuilds();
  const tags = await Promise.all(
    guilds.map((g) =>
      apiFetch<{ tag: string; badge: number; color: number; members_can_display: boolean }>(
        `/guilds/${g.id}/tag`,
        { retries: 0 },
      ).then(
        (tag) => ({ g, tag }),
        () => null, // 404: the server has no tag
      ),
    ),
  );
  return tags.flatMap((r) =>
    r && r.tag.members_can_display
      ? [
          {
            guildId: r.g.id,
            guildName: r.g.name,
            tag: r.tag.tag,
            badge: r.tag.badge,
            color: r.tag.color,
          },
        ]
      : [],
  );
}
