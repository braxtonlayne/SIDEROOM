// The Study's sidebar (STU-SHF-001): by source, never by genre.
//
// The sidebar is built from ONE data-source interface shaped like 41-C's `GET /api/v1/shelves/visible` (a list of
// shelves the viewer may see, each tagged own / friend / guild); `./api#visibleShelves` fetches it. `legacyVisible`
// derives the same list from the older mount, friend-share, shelf and games APIs (kept for tests and as a fallback
// shape). The server decides every access either way.

export type StudyGroup = 'mine' | 'friends' | 'guilds';

/** One shelf the viewer may see: the row shape of `/shelves/visible` (a shelf is a storage mount today). */
export interface VisibleShelf {
  shelf_id: string;
  kind: 'own' | 'friend' | 'guild';
  label: string;
  owner_id?: string;
  /** The owner's handle, as the server knows it (the badge falls back to it). */
  owner_handle?: string;
  guild_id?: string;
  /** The server's name, as the server knows it. */
  guild_name?: string;
}

/** The single seam: where the visible shelves come from. */
export type VisibleSource = () => Promise<VisibleShelf[]>;

/** One mount the viewer reaches. */
export interface StudyMount {
  mountId: string;
  label: string;
}

/** One entry of the sidebar: My Files, one friend, or one guild. */
export interface StudyEntry {
  /** `mine`, `friend:<owner id>` or `guild:<guild id>`. */
  id: string;
  group: StudyGroup;
  ownerId?: string;
  ownerHandle?: string;
  guildId?: string;
  guildName?: string;
  mounts: StudyMount[];
}

export interface StudySidebar {
  mine: StudyEntry | null;
  friends: StudyEntry[];
  guilds: StudyEntry[];
}

/** The existing APIs' answers `legacyVisible` is built from. */
export interface LegacyInputs {
  /** `GET /users/@me/storage/mounts`. */
  own: { id: string; label: string; status?: string }[];
  /** `GET /users/@me/storage/shares`. */
  incoming: { mount_id: string; owner_id: string; label: string }[];
  /** The `sources` of `GET /shelves/{section}` for each section. */
  shelf: {
    mount_id: string;
    label: string;
    scope: 'own' | 'friend' | 'guild' | 'staff';
    owner_id?: string;
    guild_id?: string;
  }[];
  /** `GET /users/@me/games/roms`: guild shelves whose role holds `view_games`. */
  roms: { mount_id: string; mount_label: string; scope: string; guild_id?: string }[];
}

/** A mount in these states cannot be browsed. */
const UNUSABLE = new Set(['disabled', 'delisted']);

/**
 * Derive the `/shelves/visible` list from the existing APIs. A mount appears once: as the viewer's own if it is
 * theirs, else as a friend's, else as a guild's.
 */
export function legacyVisible(i: LegacyInputs): VisibleShelf[] {
  const seen = new Set<string>();
  const out: VisibleShelf[] = [];
  const add = (v: VisibleShelf) => {
    if (seen.has(v.shelf_id)) return;
    seen.add(v.shelf_id);
    out.push(v);
  };
  for (const m of i.own) {
    if (!UNUSABLE.has(m.status ?? '')) add({ shelf_id: m.id, kind: 'own', label: m.label });
  }
  for (const s of i.shelf) {
    if (s.scope === 'own') add({ shelf_id: s.mount_id, kind: 'own', label: s.label });
  }
  for (const s of i.incoming) {
    add({ shelf_id: s.mount_id, kind: 'friend', label: s.label, owner_id: s.owner_id });
  }
  for (const s of i.shelf) {
    if (s.scope === 'friend' && s.owner_id) {
      add({ shelf_id: s.mount_id, kind: 'friend', label: s.label, owner_id: s.owner_id });
    } else if (s.scope === 'guild' && s.guild_id) {
      add({ shelf_id: s.mount_id, kind: 'guild', label: s.label, guild_id: s.guild_id });
    }
  }
  for (const r of i.roms) {
    if (r.scope === 'guild' && r.guild_id) {
      add({ shelf_id: r.mount_id, kind: 'guild', label: r.mount_label, guild_id: r.guild_id });
    }
  }
  return out;
}

/** Group the visible shelves: My Files, one entry per friend, one entry per guild. */
export function buildSidebar(visible: readonly VisibleShelf[]): StudySidebar {
  const mine: StudyMount[] = [];
  const friends = new Map<string, StudyEntry>();
  const guilds = new Map<string, StudyEntry>();
  const seen = new Set<string>();
  for (const v of visible) {
    if (seen.has(v.shelf_id)) continue;
    seen.add(v.shelf_id);
    const mount = { mountId: v.shelf_id, label: v.label };
    if (v.kind === 'own') mine.push(mount);
    else if (v.kind === 'friend' && v.owner_id) {
      const e = friends.get(v.owner_id) ?? {
        id: `friend:${v.owner_id}`,
        group: 'friends' as const,
        ownerId: v.owner_id,
        ...(v.owner_handle ? { ownerHandle: v.owner_handle } : {}),
        mounts: [],
      };
      e.mounts.push(mount);
      friends.set(v.owner_id, e);
    } else if (v.kind === 'guild' && v.guild_id) {
      const e = guilds.get(v.guild_id) ?? {
        id: `guild:${v.guild_id}`,
        group: 'guilds' as const,
        guildId: v.guild_id,
        ...(v.guild_name ? { guildName: v.guild_name } : {}),
        mounts: [],
      };
      e.mounts.push(mount);
      guilds.set(v.guild_id, e);
    }
  }
  return {
    mine: mine.length > 0 ? { id: 'mine', group: 'mine', mounts: mine } : null,
    friends: [...friends.values()],
    guilds: [...guilds.values()],
  };
}

/** Every entry in sidebar order. */
export const allEntries = (s: StudySidebar): StudyEntry[] => [
  ...(s.mine ? [s.mine] : []),
  ...s.friends,
  ...s.guilds,
];

/** Sort entries by the name the client knows them by. */
export function sortEntries(
  entries: StudyEntry[],
  nameOf: (e: StudyEntry) => string,
): StudyEntry[] {
  return [...entries].sort((a, b) => nameOf(a).localeCompare(nameOf(b)));
}

/** What a provenance badge says: whose bucket, or which guild. */
export type Provenance =
  | { kind: 'own'; handle: string | null }
  | { kind: 'friend'; handle: string | null }
  | { kind: 'guild'; name: string | null };

export function provenanceOf(
  entry: StudyEntry,
  handleOf: (userId: string) => string | null,
  guildNameOf: (guildId: string) => string | null,
  selfId: string | null,
): Provenance {
  if (entry.group === 'mine') return { kind: 'own', handle: selfId ? handleOf(selfId) : null };
  if (entry.group === 'friends') {
    const known = entry.ownerId ? handleOf(entry.ownerId) : null;
    return { kind: 'friend', handle: known ?? entry.ownerHandle ?? null };
  }
  const known = entry.guildId ? guildNameOf(entry.guildId) : null;
  return { kind: 'guild', name: known ?? entry.guildName ?? null };
}

/** The entry that holds a mount, or null. */
export function entryOfMount(s: StudySidebar, mountId: string): StudyEntry | null {
  return allEntries(s).find((e) => e.mounts.some((m) => m.mountId === mountId)) ?? null;
}
