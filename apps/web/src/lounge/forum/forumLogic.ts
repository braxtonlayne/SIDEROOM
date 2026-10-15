import type { ThreadInfo } from '../threads/threadStore';

export type ForumSort = 'activity' | 'created';
export type ForumLayout = 'list' | 'gallery';

export interface ForumTag {
  id: string;
  name: string;
  moderated: boolean;
  emoji: string | null;
}

/** Wire forum tag (`available_tags` of a channel object) to the view model. */
export function toForumTag(w: unknown): ForumTag | null {
  const o = (w ?? {}) as Record<string, unknown>;
  if (typeof o.id !== 'string' || typeof o.name !== 'string') return null;
  return {
    id: o.id,
    name: o.name,
    moderated: !!o.moderated,
    emoji: typeof o.emoji_name === 'string' ? o.emoji_name : null,
  };
}

const cmp = (a: string, b: string) =>
  a.length !== b.length ? b.length - a.length : a < b ? 1 : a > b ? -1 : 0;

/** Pinned posts first; then newest activity or newest creation first. Search and tag filter apply. */
export function visiblePosts(
  posts: readonly ThreadInfo[],
  opts: { sort: ForumSort; query: string; tagIds: ReadonlySet<string> },
): ThreadInfo[] {
  const q = opts.query.trim().toLowerCase();
  return posts
    .filter((p) => !q || p.name.toLowerCase().includes(q))
    .filter((p) => opts.tagIds.size === 0 || p.appliedTags.some((t) => opts.tagIds.has(t)))
    .sort((a, b) => {
      if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
      return opts.sort === 'created'
        ? cmp(a.id, b.id)
        : cmp(a.lastMessageId ?? a.id, b.lastMessageId ?? b.id);
    });
}

/** Default layout from the channel's setting: 1 list, 2 gallery, otherwise gallery for media channels. */
export function defaultLayout(setting: number | null | undefined, isMedia: boolean): ForumLayout {
  if (setting === 1) return 'list';
  if (setting === 2) return 'gallery';
  return isMedia ? 'gallery' : 'list';
}
