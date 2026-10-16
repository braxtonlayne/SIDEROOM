import { Button, EmptyState, Icon } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import type * as S from '../../store/types';
import { formatMessageTime } from '../time';
import { type Channel, useUsers } from '../data';
import { snowflakeTime } from '../liveMap';
import { useLoungeGotoThread } from '../nav';
import { threadsOf, useThreads, type ThreadInfo } from '../threads/threadStore';
import { NewPostDialog } from './NewPostDialog';
import {
  defaultLayout,
  toForumTag,
  visiblePosts,
  type ForumLayout,
  type ForumSort,
  type ForumTag,
} from './forumLogic';
import './forum.css';

interface ForumChannelInfo {
  available_tags?: unknown[] | null;
  default_sort_order?: number | null;
  default_forum_layout?: number | null;
  flags?: number | null;
}

interface Starter {
  text: string;
  thumb: string | null;
  /** Reactions on the opening message, all emoji together (LNG-FRM-003). */
  reactions: number;
}

const starters = new Map<string, Starter | null>();

/** The first message of a post, fetched once per post (preview text and media thumbnail). */
function useStarter(threadId: string): Starter | null | undefined {
  const [v, setV] = useState<Starter | null | undefined>(starters.get(threadId));
  useEffect(() => {
    if (starters.has(threadId)) return;
    let live = true;
    apiFetch<S.Message[]>(`/channels/${threadId}/messages?after=0&limit=1`, { retries: 0 })
      .then((list) => {
        const m = list[0];
        const img = m?.attachments?.find((a) => (a.content_type ?? '').startsWith('image/'));
        const s = m
          ? {
              text: m.content,
              thumb: img?.url ?? null,
              reactions: (m.reactions ?? []).reduce((n, r) => n + r.count, 0),
            }
          : null;
        starters.set(threadId, s);
        if (live) setV(s);
      })
      .catch(() => {
        starters.set(threadId, null);
        if (live) setV(null);
      });
    return () => {
      live = false;
    };
  }, [threadId]);
  return v;
}

function PostCard({
  post,
  tags,
  layout,
  onOpen,
}: {
  post: ThreadInfo;
  tags: Map<string, ForumTag>;
  layout: ForumLayout;
  onOpen: () => void;
}) {
  const t = useT();
  const users = useUsers();
  const starter = useStarter(post.id);
  const author = (post.ownerId && users[post.ownerId]?.displayName) || t('lounge.forum.unknown');
  const last = snowflakeTime(post.lastMessageId ?? post.id);
  return (
    <li className="lng-post" data-layout={layout}>
      <button type="button" className="lng-post-btn" onClick={onOpen}>
        {starter?.thumb && layout === 'gallery' && (
          <img className="lng-post-thumb" src={starter.thumb} alt="" loading="lazy" />
        )}
        <span className="lng-post-body">
          <strong className="lng-post-title">
            {post.pinned && <Icon name="pin" size={16} />} {post.name}
          </strong>
          {post.appliedTags.length > 0 && (
            <span className="lng-post-tags">
              {post.appliedTags.map((id) => (
                <span key={id} className="lng-tag">
                  {tags.get(id)?.emoji ?? ''} {tags.get(id)?.name ?? id}
                </span>
              ))}
            </span>
          )}
          {starter?.text && <span className="lng-post-preview">{starter.text}</span>}
          <span className="lng-muted lng-post-meta">
            {author}
            {' · '}
            {t('lounge.forum.messages', { count: post.messageCount })}
            {starter && starter.reactions > 0
              ? ` · ${t('lounge.forum.reactions', { count: starter.reactions })}`
              : ''}
            {last ? ` · ${t('lounge.forum.lastActive', { when: formatMessageTime(last) })}` : ''}
          </span>
        </span>
        {starter?.thumb && layout === 'list' && (
          <img className="lng-post-thumb" src={starter.thumb} alt="" loading="lazy" />
        )}
      </button>
    </li>
  );
}

/** Forum and media channel body: guidelines, New Post, search, sort, list/gallery, tag chips (LNG-FRM-002..005). */
export function ForumView({ guildId, channel }: { guildId: string; channel: Channel }) {
  const t = useT();
  const isMedia = channel.type === 'media';
  const byId = useThreads((s) => s.byId);
  const loaded = useThreads((s) => !!s.loadedGuilds[guildId]);
  const openThread = useLoungeGotoThread();
  const [info, setInfo] = useState<ForumChannelInfo | null>(null);
  const [error, setError] = useState(false);
  const [more, setMore] = useState(false);
  const [query, setQuery] = useState('');
  const [sort, setSort] = useState<ForumSort | null>(null);
  const [layout, setLayout] = useState<ForumLayout | null>(null);
  const [tagIds, setTagIds] = useState<ReadonlySet<string>>(new Set());
  const [composing, setComposing] = useState(false);

  useEffect(() => {
    let live = true;
    apiFetch<ForumChannelInfo>(`/channels/${channel.id}`, { retries: 0 })
      .then((c) => live && setInfo(c))
      .catch(() => live && setError(true));
    return () => {
      live = false;
    };
  }, [channel.id]);
  useEffect(() => {
    if (!loaded)
      useThreads
        .getState()
        .loadActive(guildId)
        .catch(() => setError(true));
  }, [loaded, guildId]);
  useEffect(() => {
    useThreads
      .getState()
      .loadArchived(channel.id)
      .then(setMore)
      .catch(() => setError(true));
  }, [channel.id]);

  const tags = useMemo(
    () => (info?.available_tags ?? []).map(toForumTag).filter((x): x is ForumTag => x !== null),
    [info],
  );
  const tagMap = useMemo(() => new Map(tags.map((x) => [x.id, x])), [tags]);
  const effSort: ForumSort = sort ?? (info?.default_sort_order === 1 ? 'created' : 'activity');
  const effLayout = layout ?? defaultLayout(info?.default_forum_layout, isMedia);
  const all = useMemo(
    () => [...threadsOf(byId, channel.id), ...threadsOf(byId, channel.id, { archived: true })],
    [byId, channel.id],
  );
  const posts = useMemo(
    () => visiblePosts(all, { sort: effSort, query, tagIds }),
    [all, effSort, query, tagIds],
  );
  const toggleTag = (id: string) =>
    setTagIds((cur) => {
      const next = new Set(cur);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  const loadMore = () => {
    const archived = threadsOf(byId, channel.id, { archived: true });
    const last = archived[archived.length - 1]?.archiveTimestamp;
    useThreads
      .getState()
      .loadArchived(channel.id, last ?? undefined)
      .then(setMore)
      .catch(() => setError(true));
  };
  const viewLabel = (v: ForumLayout) =>
    v === 'list' ? t('lounge.forum.viewList') : t('lounge.forum.viewGallery');

  return (
    <div className="lng-forum">
      {channel.topic && (
        <aside className="lng-forum-guidelines" aria-label={t('lounge.forum.guidelines')}>
          <strong>{t('lounge.forum.guidelines')}</strong>
          <p>{channel.topic}</p>
        </aside>
      )}
      <div className="lng-forum-tools">
        {channel.canSend && (
          <Button onClick={() => setComposing(true)}>
            {isMedia ? t('lounge.forum.newMedia') : t('lounge.forum.newPost')}
          </Button>
        )}
        <input
          type="search"
          className="lng-forum-search"
          value={query}
          placeholder={t('lounge.forum.search')}
          aria-label={t('lounge.forum.search')}
          onChange={(e) => setQuery(e.target.value)}
        />
        <label className="lng-forum-select">
          <span className="lng-muted">{t('lounge.forum.sort')}</span>
          <select value={effSort} onChange={(e) => setSort(e.target.value as ForumSort)}>
            <option value="activity">{t('lounge.forum.sortActivity')}</option>
            <option value="created">{t('lounge.forum.sortCreated')}</option>
          </select>
        </label>
        <div role="group" aria-label={t('lounge.forum.view')} className="lng-forum-views">
          {(['list', 'gallery'] as const).map((v) => (
            <button
              key={v}
              type="button"
              className="lng-icon-btn"
              aria-pressed={effLayout === v}
              aria-label={viewLabel(v)}
              title={viewLabel(v)}
              onClick={() => setLayout(v)}
            >
              <Icon name={v === 'list' ? 'list' : 'grid'} size={20} />
            </button>
          ))}
        </div>
      </div>
      {tags.length > 0 && (
        <div role="group" aria-label={t('lounge.forum.tags')} className="lng-forum-chips">
          {tags.map((tag) => (
            <button
              key={tag.id}
              type="button"
              className="lng-tag lng-tag-filter"
              aria-pressed={tagIds.has(tag.id)}
              onClick={() => toggleTag(tag.id)}
            >
              {tag.emoji ?? ''} {tag.name}
            </button>
          ))}
        </div>
      )}
      {error && (
        <p className="lng-muted" role="alert">
          {t('lounge.forum.loadError')}
        </p>
      )}
      {posts.length === 0 && !error ? (
        <EmptyState
          title={t('lounge.forum.emptyTitle')}
          description={
            query || tagIds.size > 0
              ? t('lounge.forum.emptyFiltered')
              : t('lounge.forum.emptyDescription')
          }
        />
      ) : (
        <ul className="lng-posts" data-layout={effLayout} aria-label={t('lounge.forum.posts')}>
          {posts.map((p) => (
            <PostCard
              key={p.id}
              post={p}
              tags={tagMap}
              layout={effLayout}
              onOpen={() => openThread(guildId, channel.id, p.id)}
            />
          ))}
        </ul>
      )}
      {more && (
        <button type="button" className="lng-link-btn" onClick={loadMore}>
          {t('lounge.forum.loadMore')}
        </button>
      )}
      {composing && (
        <NewPostDialog
          guildId={guildId}
          channelId={channel.id}
          tags={tags}
          requireTag={((info?.flags ?? 0) & 16) !== 0}
          onClose={() => setComposing(false)}
        />
      )}
    </div>
  );
}
