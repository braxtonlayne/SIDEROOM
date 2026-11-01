import { apiFetch } from '../api/client';
import type { VisibleShelf, VisibleSource } from './sources';

/** One row of `GET /shelves/visible` (41-C, `crates/sr-api/src/routes/storage/shelf_access.rs`). */
interface VisibleRow {
  shelf_id: string;
  label: string;
  via: 'own' | 'friend' | 'guild';
  owner_id?: string;
  owner_handle?: string;
  guild_id?: string;
  guild_name?: string;
}

/**
 * The shelves the viewer may see, decided on the server by the same function as every read (`scopes::access`): their
 * own, friends' shelves shared with them, and server shelves their role reaches. Own first, then friends', then servers'.
 */
export const visibleShelves: VisibleSource = async (): Promise<VisibleShelf[]> => {
  const r = await apiFetch<{ shelves: VisibleRow[] }>('/shelves/visible', { retries: 0 });
  return r.shelves.map((s) => ({
    shelf_id: s.shelf_id,
    kind: s.via,
    label: s.label,
    ...(s.owner_id ? { owner_id: s.owner_id } : {}),
    ...(s.owner_handle ? { owner_handle: s.owner_handle } : {}),
    ...(s.guild_id ? { guild_id: s.guild_id } : {}),
    ...(s.guild_name ? { guild_name: s.guild_name } : {}),
  }));
};

export const studyKeys = {
  visible: ['study', 'visible'] as const,
  notes: (mountId: string) => ['study', 'notes', mountId] as const,
  dir: (mountId: string, prefix: string) => ['study', 'dir', mountId, prefix] as const,
  recent: (ids: string) => ['study', 'recent', ids] as const,
};
