/**
 * Syncs Lounge layout state with the account's settings blob (D-179, F-18).
 *
 * Keys, all inside `GET/PATCH /users/@me/settings`:
 * - `guild_positions` (guild ids in rail order) and `guild_folders` (Discord's shape: one entry per top-level
 *   rail item, `id` null for a bare server, `color` an integer) for the server rail;
 * - `collapsed_categories`, `last_channel_by_guild` and `right_panel_width` for the rest of the layout.
 *
 * The Lounge UI store keeps its localStorage copy as first paint and the offline cache. After the blob loads,
 * the server wins for keys it has; keys only this device has are pushed up once (migration). Changes are
 * written as a merge patch, debounced by one second. `SR_USER_SETTINGS_UPDATE` from other sessions lands in the
 * store's `userSettings` slice and is applied here.
 */
import { useEffect } from 'react';
import { rest, useStore } from '../../store';
import type { UserSettingsBlob } from '../../store';
import type { RailEntry } from '../data';
import { clampPanel, useLoungeUi, type LoungeUiState } from '../store';
import { flatGuildIds } from './railLayout';

export const SETTINGS_DEBOUNCE_MS = 1000;
const DEFAULT_PANEL_WIDTH = 240;

type Json = unknown;
type Settings = Record<string, Json>;

/** Keys whose objects merge key by key (a removed entry needs an explicit null in the patch). */
const RECORD_KEYS = new Set(['collapsed_categories', 'last_channel_by_guild']);

interface WireFolder {
  id: number | string | null;
  name: string | null;
  color: number | null;
  guild_ids: string[];
}

const hexToInt = (hex: string): number | null => {
  const n = Number.parseInt(hex.replace('#', ''), 16);
  return Number.isFinite(n) ? n : null;
};
const intToHex = (n: number) => `#${n.toString(16).padStart(6, '0')}`;

/** The rail layout in Discord's `guild_positions` / `guild_folders` shapes. */
export function railToSettings(rail: readonly RailEntry[]): {
  guild_positions: string[];
  guild_folders: WireFolder[];
} {
  return {
    guild_positions: flatGuildIds(rail),
    guild_folders: rail.map((e) =>
      e.kind === 'guild'
        ? { id: null, name: null, color: null, guild_ids: [e.guildId] }
        : {
            id: /^\d+$/.test(e.id) ? Number(e.id) : e.id,
            name: e.name,
            color: hexToInt(e.color),
            guild_ids: [...e.guildIds],
          },
    ),
  };
}

const isStringArray = (v: Json): v is string[] =>
  Array.isArray(v) && v.every((x) => typeof x === 'string');

/**
 * Reads a rail layout from settings. `guild_folders` wins; with only `guild_positions` (a plain Discord
 * export) the order alone is used. Returns undefined when neither key is usable.
 */
export function railFromSettings(s: Settings): RailEntry[] | undefined {
  const folders = s.guild_folders;
  if (Array.isArray(folders)) {
    const out: RailEntry[] = [];
    for (const raw of folders as Partial<WireFolder>[]) {
      if (!raw || typeof raw !== 'object' || !isStringArray(raw.guild_ids)) continue;
      const ids = raw.guild_ids;
      if (raw.id == null) {
        for (const guildId of ids) out.push({ kind: 'guild', guildId });
      } else if (ids.length) {
        out.push({
          kind: 'folder',
          id: String(raw.id),
          name: typeof raw.name === 'string' && raw.name ? raw.name : null,
          color: typeof raw.color === 'number' ? intToHex(raw.color) : '#5865f2',
          guildIds: ids,
        });
      }
    }
    return out;
  }
  if (isStringArray(s.guild_positions)) {
    return s.guild_positions.map((guildId): RailEntry => ({ kind: 'guild', guildId }));
  }
  return undefined;
}

type SyncedUi = 'railLayout' | 'collapsedCategories' | 'lastChannelByGuild' | 'rightPanelWidth';

/** The synced values held by the Lounge UI store right now (a rail never customized is omitted). */
export function readLocal(ui: Pick<LoungeUiState, SyncedUi>): Settings {
  const out: Settings = {};
  if (ui.railLayout) Object.assign(out, railToSettings(ui.railLayout));
  const collapsed = Object.entries(ui.collapsedCategories).filter(([, v]) => v);
  out.collapsed_categories = Object.fromEntries(collapsed.map(([k]) => [k, true]));
  out.last_channel_by_guild = { ...ui.lastChannelByGuild };
  out.right_panel_width = ui.rightPanelWidth;
  return out;
}

const isRecord = (v: Json): v is Record<string, Json> =>
  !!v && typeof v === 'object' && !Array.isArray(v);

/** Applies the keys present in `s` to the Lounge UI store. */
export function applySettingsToUi(s: Settings): void {
  const patch: Partial<LoungeUiState> = {};
  const rail = railFromSettings(s);
  if (rail) patch.railLayout = rail;
  if (isRecord(s.collapsed_categories)) {
    patch.collapsedCategories = Object.fromEntries(
      Object.entries(s.collapsed_categories)
        .filter(([, v]) => v === true)
        .map(([k]) => [k, true]),
    );
  }
  if (isRecord(s.last_channel_by_guild)) {
    patch.lastChannelByGuild = Object.fromEntries(
      Object.entries(s.last_channel_by_guild).filter(
        (e): e is [string, string] => typeof e[1] === 'string',
      ),
    );
  }
  if (typeof s.right_panel_width === 'number')
    patch.rightPanelWidth = clampPanel(s.right_panel_width);
  if (Object.keys(patch).length) useLoungeUi.setState(patch);
}

/** Drops untouched defaults the server does not hold yet, so a fresh account is not seeded with noise. */
function withoutDefaults(local: Settings, synced: Settings): Settings {
  const out: Settings = {};
  for (const [key, value] of Object.entries(local)) {
    const empty = isRecord(value)
      ? Object.keys(value).length === 0
      : key === 'right_panel_width' && value === DEFAULT_PANEL_WIDTH;
    if (empty && !(key in synced)) continue;
    out[key] = value;
  }
  return out;
}

const same = (a: Json, b: Json) => JSON.stringify(a) === JSON.stringify(b);

/**
 * The merge patch that turns `synced` (what the server has) into `local`. Arrays and scalars replace;
 * record keys diff entry by entry with `null` for removals. Empty when nothing differs.
 */
export function diffSettings(local: Settings, synced: Settings): Settings {
  const patch: Settings = {};
  for (const [key, value] of Object.entries(local)) {
    if (same(value, synced[key])) continue;
    if (RECORD_KEYS.has(key) && isRecord(value)) {
      const before = isRecord(synced[key]) ? synced[key] : {};
      const d: Record<string, Json> = {};
      for (const [k, v] of Object.entries(value)) if (!same(v, before[k])) d[k] = v;
      for (const k of Object.keys(before)) if (!(k in value)) d[k] = null;
      if (Object.keys(d).length) patch[key] = d;
    } else {
      patch[key] = value;
    }
  }
  return patch;
}

/** Folds a sent patch into the known server state, the way the server merges it. */
function mergePatch(synced: Settings, patch: Settings): Settings {
  const out: Settings = { ...synced };
  for (const [key, value] of Object.entries(patch)) {
    if (RECORD_KEYS.has(key) && isRecord(value)) {
      const merged: Record<string, Json> = isRecord(synced[key]) ? { ...synced[key] } : {};
      for (const [k, v] of Object.entries(value)) {
        if (v === null) delete merged[k];
        else merged[k] = v;
      }
      out[key] = merged;
    } else out[key] = value;
  }
  return out;
}

const SYNC_KEYS = [
  'guild_positions',
  'guild_folders',
  'collapsed_categories',
  'last_channel_by_guild',
  'right_panel_width',
] as const;

/** Starts the sync; returns the stop function. Exported for tests (the shell uses `useUserSettingsSync`). */
export function startUserSettingsSync(debounceMs = SETTINGS_DEBOUNCE_MS): () => void {
  /** What the server is known to hold, in the same normalized form `readLocal` produces. */
  let synced: Settings = {};
  let loaded = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;
  let inflight = false;
  let again = false;
  let ownWrite = false;

  const schedule = () => {
    if (stopped || !loaded) return;
    clearTimeout(timer);
    timer = setTimeout(() => void flush(), debounceMs);
  };

  const adopt = (blob: UserSettingsBlob | null) => {
    if (!blob) return;
    const wanted: Settings = {};
    for (const k of SYNC_KEYS) if (k in blob.settings) wanted[k] = blob.settings[k];
    applySettingsToUi(wanted);
    // Remember the server's values in local normal form so applying them does not echo back as a patch.
    const local = readLocal(useLoungeUi.getState());
    const next: Settings = {};
    for (const k of SYNC_KEYS) {
      if (!(k in blob.settings)) continue;
      next[k] = k in local ? local[k] : blob.settings[k];
    }
    synced = next;
    loaded = true;
    schedule();
  };

  const flush = async (): Promise<void> => {
    timer = undefined;
    if (stopped || !loaded) return;
    if (inflight) {
      again = true;
      return;
    }
    const patch = diffSettings(withoutDefaults(readLocal(useLoungeUi.getState()), synced), synced);
    if (!Object.keys(patch).length) return;
    inflight = true;
    try {
      const saved = await rest.patchUserSettings(patch);
      synced = mergePatch(synced, patch);
      // Record the new version in the store without re-adopting our own write (no echo, no loop).
      ownWrite = true;
      try {
        if (!stopped) useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', saved);
      } finally {
        ownWrite = false;
      }
    } catch {
      // Offline or rejected: localStorage still has it; the next change or the `online` event retries.
    } finally {
      inflight = false;
      if (again) {
        again = false;
        schedule();
      }
    }
  };

  const unUi = useLoungeUi.subscribe((s, prev) => {
    if (
      s.railLayout !== prev.railLayout ||
      s.collapsedCategories !== prev.collapsedCategories ||
      s.lastChannelByGuild !== prev.lastChannelByGuild ||
      s.rightPanelWidth !== prev.rightPanelWidth
    )
      schedule();
  });
  let lastSeen = useStore.getState().userSettings;
  const unStore = useStore.subscribe((s) => {
    if (s.userSettings === lastSeen) return;
    lastSeen = s.userSettings;
    if (!ownWrite) adopt(s.userSettings);
  });
  const onOnline = () => schedule();
  window.addEventListener('online', onOnline);

  // Initial load; a blob already present (a gateway push before the fetch finished) is adopted first.
  adopt(lastSeen);
  rest
    .fetchUserSettings()
    .then((blob) => {
      if (stopped) return;
      useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', blob);
      // An equal-version blob leaves the slice alone but still unlocks writing.
      if (!loaded) adopt(useStore.getState().userSettings);
    })
    .catch(() => {
      /* offline: the localStorage copy keeps working; writing starts once a load succeeds */
    });

  return () => {
    stopped = true;
    clearTimeout(timer);
    unUi();
    unStore();
    window.removeEventListener('online', onOnline);
  };
}

/** Mounts the sync for the Lounge shell's lifetime. */
export function useUserSettingsSync(enabled: boolean): void {
  useEffect(() => {
    if (!enabled) return;
    return startUserSettingsSync();
  }, [enabled]);
}
