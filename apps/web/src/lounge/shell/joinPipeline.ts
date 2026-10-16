// REST wrappers and a small store for the join pipeline of a server (LNG-GLD-030/031/033/034): membership
// screening, onboarding answers, the server guide and the security actions. Paths mirror
// crates/sr-api/src/routes/settings/{community,guide}.rs.
import { create } from 'zustand';
import { apiFetch } from '../../api/client';

const send = <T>(path: string, method: string, json?: unknown) =>
  apiFetch<T>(path, { method, retries: 0, ...(json !== undefined ? { json } : {}) });

const g = (id: string) => encodeURIComponent(id);

export interface OwnMember {
  pending?: boolean;
  joined_at?: string;
  roles?: string[];
  /** Discord member flags; bit 2 is COMPLETED_ONBOARDING (D-245). */
  flags?: number;
}
/** `GET /users/@me/guilds/{id}/member`: the viewer's own membership (`pending` until they agree to the rules). */
export const getOwnMember = (guildId: string) =>
  apiFetch<OwnMember>(`/users/@me/guilds/${g(guildId)}/member`, { retries: 0 });

export interface MemberVerification {
  enabled: boolean;
  description: string | null;
  form_fields: { field_type: string; label: string; values: string[]; required: boolean }[];
}
export const getMemberVerification = (guildId: string) =>
  apiFetch<MemberVerification>(`/guilds/${g(guildId)}/member-verification`, { retries: 0 });
/** `PUT /guilds/{id}/requests/@me`: agree to the rules and stop being pending. */
export const acceptScreening = (guildId: string) =>
  send<unknown>(`/guilds/${g(guildId)}/requests/@me`, 'PUT', { agreed: true });

export interface OnboardingOption {
  id?: string | null;
  title: string;
  description?: string | null;
  emoji?: string | null;
  channel_ids: string[];
  role_ids: string[];
}
export interface OnboardingPrompt {
  id?: string | null;
  title: string;
  single_select: boolean;
  required: boolean;
  in_onboarding: boolean;
  options: OnboardingOption[];
}
export interface Onboarding {
  guild_id: string;
  enabled: boolean;
  default_channel_ids: string[];
  prompts: OnboardingPrompt[];
}
export const getOnboarding = (guildId: string) =>
  apiFetch<Onboarding>(`/guilds/${g(guildId)}/onboarding`, { retries: 0 });
export const postOnboardingResponses = (guildId: string, optionIds: string[]) =>
  send<{ channel_ids: string[]; role_ids: string[] }>(
    `/guilds/${g(guildId)}/onboarding-responses`,
    'POST',
    { option_ids: optionIds },
  );

export interface Guide {
  guild_id: string;
  enabled: boolean;
  welcome_message: string | null;
  todos: {
    id?: string | null;
    title: string;
    description?: string | null;
    channel_id?: string | null;
  }[];
  resources: { id?: string | null; title: string; content: string }[];
}
export const getGuide = (guildId: string) =>
  apiFetch<Guide>(`/guilds/${g(guildId)}/sr/guide`, { retries: 0 });

export interface SecuritySettings {
  invites_paused: boolean;
  dm_spam_filter?: boolean;
  /** The join-rate alert pauses invites for 24 hours by itself (D-243). */
  raid_auto_pause_invites?: boolean;
}
export const getSecurity = (guildId: string) =>
  apiFetch<SecuritySettings>(`/guilds/${g(guildId)}/sr/community`, { retries: 0 });
export const patchSecurity = (guildId: string, body: Partial<SecuritySettings>) =>
  send<SecuritySettings>(`/guilds/${g(guildId)}/sr/community`, 'PATCH', body);

/** `incidents_data` of the guild object (D-243). A time in the past means the pause ended. */
export interface IncidentsData {
  invites_disabled_until: string | null;
  dms_disabled_until: string | null;
  raid_detected_at: string | null;
}
export const getIncidents = async (guildId: string): Promise<IncidentsData | null> =>
  (await apiFetch<{ incidents_data?: IncidentsData }>(`/guilds/${g(guildId)}`, { retries: 0 }))
    .incidents_data ?? null;
/** `PUT /guilds/{id}/incident-actions`: both times are replaced; `null` clears one. At most 24 hours ahead. */
export const putIncidentActions = (
  guildId: string,
  body: { invites_disabled_until: string | null; dms_disabled_until: string | null },
) => send<IncidentsData>(`/guilds/${g(guildId)}/incident-actions`, 'PUT', body);

/** Whether a pause time from `incidents_data` is still running at `now`. */
export const pauseActive = (until: string | null | undefined, now = Date.now()) =>
  !!until && Date.parse(until) > now;

/** Discord's `COMPLETED_ONBOARDING` member flag (D-245). */
export const COMPLETED_ONBOARDING = 2;
export const completedOnboarding = (flags: number | undefined) =>
  ((flags ?? 0) & COMPLETED_ONBOARDING) !== 0;

/**
 * Per-server facts filled by the join gate so the server menu offers "Server Guide" and "Browse Channels" only where
 * they apply, and the channel list offers "Channels & Roles" to a member who has not answered the questions yet.
 */
export const useGuideFlags = create<{
  enabled: Record<string, boolean>;
  onboarding: Record<string, boolean>;
  /** The viewer's own member `flags` per server, once read (the server owner's are never read). */
  memberFlags: Record<string, number>;
  /** Servers whose questions the person skipped since this page loaded (in memory only, D-245). */
  skipped: Record<string, boolean>;
  set: (id: string, on: boolean) => void;
  setOnboarding: (id: string, on: boolean) => void;
  setMemberFlags: (id: string, flags: number) => void;
  skip: (id: string) => void;
}>((set) => ({
  enabled: {},
  onboarding: {},
  memberFlags: {},
  skipped: {},
  set: (id, on) => set((s) => ({ enabled: { ...s.enabled, [id]: on } })),
  setOnboarding: (id, on) => set((s) => ({ onboarding: { ...s.onboarding, [id]: on } })),
  setMemberFlags: (id, flags) => set((s) => ({ memberFlags: { ...s.memberFlags, [id]: flags } })),
  skip: (id) => set((s) => ({ skipped: { ...s.skipped, [id]: true } })),
}));
