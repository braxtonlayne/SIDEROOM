/**
 * Server rail ordering and folders (LNG-RAIL-007, LNG-RAIL-008). Pure functions over `RailEntry[]`.
 *
 * The layout lives in the lounge UI store (localStorage, first paint) and is synced to the account's settings
 * blob as `guild_positions` / `guild_folders` (shell/userSettingsSync.ts, D-179).
 */
import type { RailEntry, Snowflake } from '../data';

export type FolderEntry = Extract<RailEntry, { kind: 'folder' }>;

export const FOLDER_COLORS = [
  '#5865f2',
  '#3ba55d',
  '#faa61a',
  '#ed4245',
  '#eb459e',
  '#9b59b6',
  '#1abc9c',
  '#99aab5',
] as const;
export const FOLDER_NAME_MAX = 32;

export const entryKey = (e: RailEntry) => (e.kind === 'guild' ? e.guildId : e.id);
const g = (guildId: Snowflake): RailEntry => ({ kind: 'guild', guildId });

/** Guild ids in rail order, folders flattened (for Ctrl+Alt+Up/Down). */
export const flatGuildIds = (rail: readonly RailEntry[]): Snowflake[] =>
  rail.flatMap((e) => (e.kind === 'guild' ? [e.guildId] : e.guildIds));

/**
 * Applies a stored layout to the guilds the server reports now: guilds that left are dropped, new guilds
 * are appended, and a folder that lost every guild disappears. With no stored layout the base order wins.
 */
export function applyRailLayout(
  base: readonly RailEntry[],
  stored: readonly RailEntry[] | null | undefined,
): RailEntry[] {
  if (!stored?.length) return [...base];
  const known = new Set(flatGuildIds(base));
  const used = new Set<Snowflake>();
  const out: RailEntry[] = [];
  for (const e of stored) {
    if (e.kind === 'guild') {
      if (known.has(e.guildId) && !used.has(e.guildId)) {
        used.add(e.guildId);
        out.push(e);
      }
      continue;
    }
    const ids = e.guildIds.filter((id) => known.has(id) && !used.has(id));
    ids.forEach((id) => used.add(id));
    if (ids.length) out.push({ ...e, guildIds: ids });
  }
  for (const e of base) {
    if (e.kind === 'guild') {
      if (!used.has(e.guildId)) out.push(e);
    } else {
      const ids = e.guildIds.filter((id) => !used.has(id));
      if (ids.length) out.push({ ...e, guildIds: ids });
    }
  }
  return out;
}

/** Removes a guild from wherever it is. A folder left empty is removed. */
function without(rail: readonly RailEntry[], guildId: Snowflake): RailEntry[] {
  const out: RailEntry[] = [];
  for (const e of rail) {
    if (e.kind === 'guild') {
      if (e.guildId !== guildId) out.push(e);
    } else {
      const ids = e.guildIds.filter((id) => id !== guildId);
      if (ids.length) out.push({ ...e, guildIds: ids });
    }
  }
  return out;
}

const indexOfKey = (rail: readonly RailEntry[], key: string) =>
  rail.findIndex((e) => entryKey(e) === key);

/** Move a top-level entry (guild or folder) next to another top-level entry. */
export function moveEntry(
  rail: readonly RailEntry[],
  key: string,
  targetKey: string,
  where: 'before' | 'after',
): RailEntry[] {
  if (key === targetKey) return [...rail];
  const from = indexOfKey(rail, key);
  if (from < 0 || indexOfKey(rail, targetKey) < 0) return [...rail];
  const moving = rail[from]!;
  const rest = rail.filter((_, i) => i !== from);
  const at = indexOfKey(rest, targetKey) + (where === 'after' ? 1 : 0);
  rest.splice(at, 0, moving);
  return rest;
}

/** Move a guild (from the top level or from a folder) next to a top-level entry. */
export function moveGuildNextTo(
  rail: readonly RailEntry[],
  guildId: Snowflake,
  targetKey: string,
  where: 'before' | 'after',
): RailEntry[] {
  const top = rail.findIndex((e) => e.kind === 'guild' && e.guildId === guildId);
  if (top >= 0) return moveEntry(rail, guildId, targetKey, where);
  const rest = without(rail, guildId);
  const at = indexOfKey(rest, targetKey);
  if (at < 0) return [...rail];
  rest.splice(at + (where === 'after' ? 1 : 0), 0, g(guildId));
  return rest;
}

/** Move one entry up or down a place at the top level (keyboard alternative to dragging). */
export function moveBy(rail: readonly RailEntry[], key: string, delta: -1 | 1): RailEntry[] {
  const i = indexOfKey(rail, key);
  const j = i + delta;
  if (i < 0 || j < 0 || j >= rail.length) return [...rail];
  const out = [...rail];
  [out[i], out[j]] = [out[j]!, out[i]!];
  return out;
}

/** Drop a guild onto another guild: both end up in a new folder at the target's place. */
export function makeFolder(
  rail: readonly RailEntry[],
  dragged: Snowflake,
  onto: Snowflake,
  folder: { id: string; color: string; name?: string | null },
): RailEntry[] {
  if (dragged === onto) return [...rail];
  const stripped = without(rail, dragged);
  const at = stripped.findIndex((e) => e.kind === 'guild' && e.guildId === onto);
  if (at < 0) return [...rail];
  const entry: FolderEntry = {
    kind: 'folder',
    id: folder.id,
    name: folder.name ?? null,
    color: folder.color,
    guildIds: [onto, dragged],
  };
  stripped.splice(at, 1, entry);
  return stripped;
}

/** Drop a guild into an existing folder (at the end, or before `beforeGuild`). */
export function addToFolder(
  rail: readonly RailEntry[],
  guildId: Snowflake,
  folderId: string,
  beforeGuild?: Snowflake,
): RailEntry[] {
  if (!rail.some((e) => e.kind === 'folder' && e.id === folderId)) return [...rail];
  return without(rail, guildId).map((e) => {
    if (e.kind !== 'folder' || e.id !== folderId) return e;
    const ids = e.guildIds.filter((id) => id !== guildId);
    const at = beforeGuild ? ids.indexOf(beforeGuild) : -1;
    ids.splice(at < 0 ? ids.length : at, 0, guildId);
    return { ...e, guildIds: ids };
  });
}

/** Take a guild out of its folder, placing it right after the folder. */
export function removeFromFolder(rail: readonly RailEntry[], guildId: Snowflake): RailEntry[] {
  const out: RailEntry[] = [];
  for (const e of rail) {
    if (e.kind === 'folder' && e.guildIds.includes(guildId)) {
      const ids = e.guildIds.filter((id) => id !== guildId);
      if (ids.length) out.push({ ...e, guildIds: ids });
      out.push(g(guildId));
    } else out.push(e);
  }
  return out;
}

/** Dissolve a folder; its guilds take its place in order. */
export function deleteFolder(rail: readonly RailEntry[], folderId: string): RailEntry[] {
  return rail.flatMap((e) => (e.kind === 'folder' && e.id === folderId ? e.guildIds.map(g) : [e]));
}

export function updateFolder(
  rail: readonly RailEntry[],
  folderId: string,
  patch: { name?: string | null; color?: string },
): RailEntry[] {
  return rail.map((e) =>
    e.kind === 'folder' && e.id === folderId
      ? {
          ...e,
          ...(patch.name !== undefined
            ? { name: patch.name?.trim() ? patch.name.trim() : null }
            : {}),
          ...(patch.color ? { color: patch.color } : {}),
        }
      : e,
  );
}

let folderSeq = 0;
/** Numeric like Discord's folder ids, so `guild_folders` stays portable (older layouts keep string ids). */
export const newFolderId = () => String(Date.now() * 1000 + (folderSeq++ % 1000));

/** Where a pointer is inside a rail item: top quarter, bottom quarter, or the middle. */
export function dropZone(offsetY: number, height: number): 'before' | 'into' | 'after' {
  if (height <= 0) return 'into';
  const r = offsetY / height;
  return r < 0.25 ? 'before' : r > 0.75 ? 'after' : 'into';
}
