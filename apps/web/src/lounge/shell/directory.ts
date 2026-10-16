// REST wrappers for the Server Directory (`routes/directory.rs`): browse listed servers and join one
// without an invite. Joins use retries: 0 so a failure surfaces instead of being replayed.
import { apiFetch } from '../../api/client';

export interface DirectoryGuild {
  id: string;
  name: string;
  description: string | null;
  icon: string | null;
  banner: string | null;
  category: string;
  featured: boolean;
  approximate_member_count: number;
  approximate_presence_count?: number | null;
  preferred_locale: string;
  vanity_url_code: string | null;
  /** The server's traits (J-07), at most five. */
  traits?: { emoji_id?: string | null; emoji_name?: string | null; label: string }[];
  /** The caller already is a member. */
  joined: boolean;
}

export interface DirectorySearch {
  query?: string;
  category?: string;
  limit?: number;
  offset?: number;
}

/** `GET /directory/categories`: the fixed list of category ids. */
export const listDirectoryCategories = () =>
  apiFetch<string[]>('/directory/categories', { retries: 0 });

/** `GET /directory`: approved listings, featured first, then by member count. */
export function searchDirectory(q: DirectorySearch): Promise<DirectoryGuild[]> {
  const p = new URLSearchParams();
  if (q.query?.trim()) p.set('query', q.query.trim());
  if (q.category) p.set('category', q.category);
  if (q.limit) p.set('limit', String(q.limit));
  if (q.offset) p.set('offset', String(q.offset));
  const qs = p.toString();
  return apiFetch<DirectoryGuild[]>(`/directory${qs ? `?${qs}` : ''}`, { retries: 0 });
}

/** `POST /directory/{guild_id}/join`. */
export const joinListedGuild = (guildId: string) =>
  apiFetch<unknown>(`/directory/${encodeURIComponent(guildId)}/join`, {
    method: 'POST',
    retries: 0,
  });
