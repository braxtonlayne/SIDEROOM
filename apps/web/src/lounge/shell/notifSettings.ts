/**
 * Per-server and per-channel notification settings (mute, level). The server owns them
 * (`/users/@me/guilds/{id}/settings`); this store caches what was fetched so the rail and channel list can
 * dim muted items and hide muted channels. Settings load quietly: a failed fetch leaves a guild
 * un-muted rather than blocking the shell.
 *
 * RDY-SCALE-002: the rail loads every server's settings with ONE request (`/users/@me/guild-settings`,
 * `loadAll`), so an account in 100 servers sends a constant number of settings requests at load, not
 * one per server. A single server is fetched on its own only when it is opened (`load`) and the batch
 * has not already answered for it.
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
  suppressHighlights: boolean;
  mobilePush: boolean;
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
  suppress_highlights?: boolean;
  mobile_push?: boolean;
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
    suppressHighlights: !!w.suppress_highlights,
    mobilePush: w.mobile_push ?? true,
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
  /**
   * One request for all of the account's servers; `guildIds` the ones on the rail. A server the batch
   * does not list has the defaults. Runs once per session (a failed batch may be retried); resolves
   * false when it failed, so the caller does not fall back to a request per server.
   */
  loadAll(guildIds: string[]): Promise<boolean>;
  load(guildId: string, force?: boolean): Promise<void>;
  patchGuild(
    guildId: string,
    patch: {
      mute?: MutePatch;
      level?: number | null;
      suppressEveryone?: boolean;
      suppressRoles?: boolean;
      suppressHighlights?: boolean;
      mobilePush?: boolean;
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
let batch: Promise<boolean> | null = null;
let batchDone = false;

interface WireList {
  settings: (WireGuildSettings & { guild_id?: string | null })[];
}

export const useNotifSettings = create<NotifState>()((set, get) => ({
  byGuild: {},
  loadAll(guildIds) {
    if (batchDone) return Promise.resolve(true);
    if (batch) return batch;
    batch = (async () => {
      let ok = false;
      try {
        const list = await apiFetch<WireList>('/users/@me/guild-settings', { retries: 0 });
        const loaded: Record<string, GuildNotif> = {};
        for (const w of list.settings) loaded[w.guild_id ?? '@me'] = fromWire(w);
        set((s) => {
          const next = { ...s.byGuild };
          // A fresher single-server answer (a patch since the request left) wins.
          for (const [id, n] of Object.entries(loaded)) if (!next[id]) next[id] = n;
          for (const id of guildIds) if (!next[id]) next[id] = fromWire({ muted: false });
          return { byGuild: next };
        });
        batchDone = true;
        ok = true;
      } catch {
        /* a server's own settings still load when it is opened */
      } finally {
        batch = null;
      }
      return ok;
    })();
    return batch;
  },
  async load(guildId, force) {
    if (!force && !get().byGuild[guildId] && batch) await batch;
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
      ...(patch.suppressHighlights !== undefined
        ? { suppress_highlights: patch.suppressHighlights }
        : {}),
      ...(patch.mobilePush !== undefined ? { mobile_push: patch.mobilePush } : {}),
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
  reset: () => {
    batchDone = false;
    set({ byGuild: {} });
  },
}));
