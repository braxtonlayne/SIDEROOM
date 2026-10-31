import { apiFetch } from '../api/client';
import { listShelf, type ShelfSection } from '../storage/shelves/api';
import { listMounts } from '../settings/user/storage/api';
import { legacyVisible, type VisibleShelf, type VisibleSource, type LegacyInputs } from './sources';

interface Incoming {
  mount_id: string;
  owner_id: string;
  label: string;
}
interface Roms {
  roms: { mount_id: string; mount_label: string; scope: string; guild_id?: string }[];
}

const SECTIONS: ShelfSection[] = ['cinema', 'library', 'music'];

/** A failed side query leaves its part empty: the sidebar shows what it could learn, never an error page. */
const soft = <T>(p: Promise<T>, empty: T): Promise<T> => p.catch(() => empty);

/**
 * Where the visible shelves come from today: the mount, friend-share, shelf and games APIs, combined by
 * `legacyVisible`. 41-C replaces this one function by `GET /shelves/visible` (same `VisibleShelf` rows).
 */
export const visibleShelves: VisibleSource = async (): Promise<VisibleShelf[]> => {
  const [own, incoming, roms, ...sections] = await Promise.all([
    soft(listMounts({ kind: 'user' }), []),
    soft(apiFetch<Incoming[]>('/users/@me/storage/shares', { retries: 0 }), []),
    soft(apiFetch<Roms>('/users/@me/games/roms', { retries: 0 }), { roms: [] }),
    ...SECTIONS.map((s) => soft(listShelf(s, { limit: 1 }), null)),
  ]);
  const inputs: LegacyInputs = {
    own: own.map((m) => ({ id: m.id, label: m.label, status: m.status })),
    incoming,
    shelf: sections.flatMap((s) => s?.sources ?? []),
    roms: roms.roms,
  };
  return legacyVisible(inputs);
};

export const studyKeys = {
  visible: ['study', 'visible'] as const,
  notes: (mountId: string) => ['study', 'notes', mountId] as const,
  dir: (mountId: string, prefix: string) => ['study', 'dir', mountId, prefix] as const,
  recent: (ids: string) => ['study', 'recent', ids] as const,
};
