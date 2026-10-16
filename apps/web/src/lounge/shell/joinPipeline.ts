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
}
export const getSecurity = (guildId: string) =>
  apiFetch<SecuritySettings>(`/guilds/${g(guildId)}/sr/community`, { retries: 0 });
export const patchSecurity = (guildId: string, body: Partial<SecuritySettings>) =>
  send<SecuritySettings>(`/guilds/${g(guildId)}/sr/community`, 'PATCH', body);

/** Per-server flags filled by the join gate so the server menu offers "Server Guide" and "Browse Channels" only where they apply. */
export const useGuideFlags = create<{
  enabled: Record<string, boolean>;
  onboarding: Record<string, boolean>;
  set: (id: string, on: boolean) => void;
  setOnboarding: (id: string, on: boolean) => void;
}>((set) => ({
  enabled: {},
  onboarding: {},
  set: (id, on) => set((s) => ({ enabled: { ...s.enabled, [id]: on } })),
  setOnboarding: (id, on) => set((s) => ({ onboarding: { ...s.onboarding, [id]: on } })),
}));

const doneKey = (userId: string, guildId: string) => `sr.onboarded.${userId}.${guildId}`;
export const onboardingDone = (userId: string, guildId: string) => {
  try {
    return localStorage.getItem(doneKey(userId, guildId)) === '1';
  } catch {
    return false;
  }
};
export const markOnboardingDone = (userId: string, guildId: string) => {
  try {
    localStorage.setItem(doneKey(userId, guildId), '1');
  } catch {
    /* private mode: the dialog may show again on the next visit */
  }
};
