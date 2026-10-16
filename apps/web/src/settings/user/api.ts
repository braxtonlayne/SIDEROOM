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

export interface DeleteAccountBody {
  password: string;
  /** The account's username, typed to confirm (case-insensitive). */
  username: string;
  /** Required with two-factor on, unless this session passed a check in the last 15 minutes. */
  mfa?: { method: 'totp' | 'recovery'; code: string };
}
/**
 * `POST /sr/auth/account/delete` (204, the session cookie is cleared). Refused with a field error `staff`, `owner`
 * or `guilds` while the account is on the staff, is the Owner, or still owns a server.
 */
export const deleteAccount = (body: DeleteAccountBody) =>
  apiPost<void>('/sr/auth/account/delete', body);

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
    avatar_decoration?: string | null;
    /** Profile theme as two 0xRRGGBB colours, primary first. */
    theme_colors?: [number, number] | null;
  };
  connected_accounts: Connection[];
  /** Platform badges (`staff` and `owner` only while `staff.show_badge` is on). */
  badges?: { id: string }[];
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
  /** A slug from `GET /sr/profile-catalog` decorations; null removes it. */
  avatar_decoration?: string | null;
  /** The profile theme: [primary, accent] as 0xRRGGBB; null clears it. */
  theme_colors?: [number, number] | null;
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

// ---- platform invites (G-57, LNG-USET-025..029) ----

export interface MemberInvite {
  /** Display form, `XXXXX-XXXXX`. */
  code: string;
  /** Path of the share link; prefix with the site origin. */
  link: string;
  note: string;
  created_at_ms: number;
  redeemed_by?: { id: string; username: string } | null;
  redeemed_at_ms?: number | null;
}
export interface MemberInvites {
  credits: number;
  open: MemberInvite[];
  redeemed: MemberInvite[];
  /** Why a code cannot be created now; `null` when it can. */
  blocked: {
    reason: 'unverified' | 'too_new' | 'at_cap' | 'no_credits' | 'frozen';
    days?: number | null;
  } | null;
}
export const listMemberInvites = () => apiFetch<MemberInvites>('/sr/invites', { retries: 0 });
export const createMemberInvite = (note: string) =>
  apiPost<MemberInvite>('/sr/invites', { note: note.trim() || undefined }, { retries: 0 });
export const revokeMemberInvite = (code: string) =>
  apiFetch<void>(`/sr/invites/${encodeURIComponent(code)}`, { method: 'DELETE', retries: 0 });

// ---- connections (LNG-USET-023, `/users/@me/connections`) ----

export interface ConnectionRow {
  type: string;
  id: string;
  name: string;
  verified: boolean;
  /** Link to the account's public page, when known. */
  url?: string | null;
  /** Shown on the profile; hidden ones are listed to their owner only. */
  visible: boolean;
  /** Whether the stats line shows. */
  show_activity: boolean;
  stats: Record<string, unknown>;
}
export interface ConnectionProvider {
  id: string;
  name: string;
  /** `true` when linking proves the account (OAuth); `false` for a typed link. */
  verified: boolean;
  /** `false` when the operator has not set the provider up. */
  available: boolean;
  /** Link-only providers that need an `https://` address (websites). */
  requires_url: boolean;
}
export interface ConnectionsResponse {
  connections: ConnectionRow[];
  providers: ConnectionProvider[];
}
export const listConnections = () =>
  apiFetch<ConnectionsResponse>('/users/@me/connections', { retries: 0 });
/** Starts the OAuth flow; send the browser to the returned `url`. */
export const authorizeConnection = (provider: string) =>
  apiPost<{ url: string }>(`/users/@me/connections/${encodeURIComponent(provider)}/authorize`);
export const putConnectionLink = (body: { provider: string; name?: string; url?: string }) =>
  apiFetch<ConnectionRow>('/users/@me/connections/link', { method: 'PUT', json: body, retries: 0 });
const connPath = (c: { type: string; id: string }) =>
  `/users/@me/connections/${encodeURIComponent(c.type)}/${encodeURIComponent(c.id)}`;
export const patchConnection = (
  c: { type: string; id: string },
  body: { visible?: boolean; show_activity?: boolean; name?: string },
) => apiFetch<ConnectionRow>(connPath(c), { method: 'PATCH', json: body, retries: 0 });
export const removeConnection = (c: { type: string; id: string }) =>
  apiFetch<void>(connPath(c), { method: 'DELETE', retries: 0 });
export const refreshConnection = (c: { type: string; id: string }) =>
  apiPost<ConnectionRow>(`${connPath(c)}/refresh`);

// ---- data export (LNG-USET-019, `/users/@me/data-export`) ----

export type DataExportState = 'none' | 'pending' | 'building' | 'ready' | 'failed' | 'expired';
export interface DataExport {
  state: DataExportState;
  id?: string | null;
  requested_at?: string | null;
  ready_at?: string | null;
  expires_at?: string | null;
  size_bytes?: number | null;
  /** API path of the zip, present while `ready`. */
  download_url?: string | null;
}
export const getDataExport = () => apiFetch<DataExport>('/users/@me/data-export', { retries: 0 });
export const requestDataExport = () => apiPost<DataExport>('/users/@me/data-export');
