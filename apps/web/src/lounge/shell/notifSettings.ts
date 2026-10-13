/**
 * Per-server and per-channel notification settings (mute, level). The server owns them
 * (`/users/@me/guilds/{id}/settings`); this store caches what was fetched so the rail and channel list can
 * dim muted items and hide muted channels. Settings load lazily and quietly: a failed fetch leaves a guild
 * un-muted rather than blocking the shell.
 */
import { create } from 'zustand';
import { apiFetch } from '../../api/client';

export interface MuteInfo {
  muted: boolean;
  /** ISO end time; null = until turned off. */
  endsAt: string | null;
}

export interface ChannelNotif extends MuteInfo {
  /** 0 all messages, 1 only mentions, 2 nothing, null = inherit. */
  level: number | null;
}

export interface GuildNotif extends MuteInfo {
  level: number | null;
  suppressEveryone: boolean;
  suppressRoles: boolean;
  channels: Record<string, ChannelNotif>;
}

interface WireMute {
  end_time?: string | null;
}
interface WireGuildSettings {
  muted: boolean;
  mute_config?: WireMute | null;
  message_notifications?: number | null;
  suppress_everyone?: boolean;
  suppress_roles?: boolean;
  channel_overrides?: {
    channel_id: string;
    muted: boolean;
    mute_config?: WireMute | null;
    message_notifications?: number | null;
  }[];
}

export function fromWire(w: WireGuildSettings): GuildNotif {
  const channels: Record<string, ChannelNotif> = {};
  for (const o of w.channel_overrides ?? [])
    channels[o.channel_id] = {
      muted: o.muted,
      endsAt: o.mute_config?.end_time ?? null,
      level: o.message_notifications ?? null,
    };
  return {
    muted: w.muted,
    endsAt: w.mute_config?.end_time ?? null,
    level: w.message_notifications ?? null,
    suppressEveryone: !!w.suppress_everyone,
    suppressRoles: !!w.suppress_roles,
    channels,
  };
}

/** A timed mute that already ended no longer counts. */
export function isMuted(m: MuteInfo | undefined, now = Date.now()): boolean {
  if (!m?.muted) return false;
  if (!m.endsAt) return true;
  const end = Date.parse(m.endsAt);
  return Number.isNaN(end) || end > now;
}

/** Seconds offered by "Mute Server" / "Mute Channel"; -1 = until I turn it back on. */
export const MUTE_DURATIONS: { label: string; seconds: number }[] = [
  { label: 'For 15 Minutes', seconds: 900 },
  { label: 'For 1 Hour', seconds: 3600 },
  { label: 'For 3 Hours', seconds: 10800 },
  { label: 'For 8 Hours', seconds: 28800 },
  { label: 'For 24 Hours', seconds: 86400 },
  { label: 'Until I turn it back on', seconds: -1 },
];

export type MutePatch = { muted: false } | { muted: true; seconds: number };

const mutePatch = (p: MutePatch) =>
  p.muted ? { muted: true, mute_config: { selected_time_window: p.seconds } } : { muted: false };

export interface NotifState {
  byGuild: Record<string, GuildNotif>;
  load(guildId: string, force?: boolean): Promise<void>;
  patchGuild(
    guildId: string,
    patch: {
      mute?: MutePatch;
      level?: number | null;
      suppressEveryone?: boolean;
      suppressRoles?: boolean;
    },
  ): Promise<void>;
  patchChannel(
    guildId: string,
    channelId: string,
    patch: { mute?: MutePatch; level?: number | null },
  ): Promise<void>;
  reset(): void;
}

/** `@me` is the DM scope and is written as is. */
const settingsPath = (guildId: string) =>
  `/users/@me/guilds/${guildId === '@me' ? guildId : encodeURIComponent(guildId)}/settings`;

const inflight = new Set<string>();

export const useNotifSettings = create<NotifState>()((set, get) => ({
  byGuild: {},
  async load(guildId, force) {
    if ((!force && get().byGuild[guildId]) || inflight.has(guildId)) return;
    inflight.add(guildId);
    try {
      const w = await apiFetch<WireGuildSettings>(settingsPath(guildId), { retries: 0 });
      set((s) => ({ byGuild: { ...s.byGuild, [guildId]: fromWire(w) } }));
    } catch {
      /* stays un-muted; the dialog reloads on open */
    } finally {
      inflight.delete(guildId);
    }
  },
  async patchGuild(guildId, patch) {
    const body: Record<string, unknown> = {
      ...(patch.mute ? mutePatch(patch.mute) : {}),
      ...('level' in patch ? { message_notifications: patch.level } : {}),
      ...(patch.suppressEveryone !== undefined
        ? { suppress_everyone: patch.suppressEveryone }
        : {}),
      ...(patch.suppressRoles !== undefined ? { suppress_roles: patch.suppressRoles } : {}),
    };
    const w = await apiFetch<WireGuildSettings>(settingsPath(guildId), {
      method: 'PATCH',
      json: body,
      retries: 0,
    });
    set((s) => ({ byGuild: { ...s.byGuild, [guildId]: fromWire(w) } }));
  },
  async patchChannel(guildId, channelId, patch) {
    const override: Record<string, unknown> = {
      ...(patch.mute ? mutePatch(patch.mute) : {}),
      ...('level' in patch ? { message_notifications: patch.level } : {}),
    };
    const w = await apiFetch<WireGuildSettings>(settingsPath(guildId), {
      method: 'PATCH',
      json: { channel_overrides: { [channelId]: override } },
      retries: 0,
    });
    set((s) => ({ byGuild: { ...s.byGuild, [guildId]: fromWire(w) } }));
  },
  reset: () => set({ byGuild: {} }),
}));
