// The smaller Library pages: latest updates (LIB-BRW-003), My library (shelves), reading history, and the
// placeholders for sections owned by later inventory items.
import {
  Button,
  EmptyState,
  FilterChips,
  Icon,
  Notice,
  SegmentedControl,
  Skeleton,
  Switch,
} from '@sideroom/ui';
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useMemo, useState } from 'react';
import { useT, type MessageKey } from '../i18n';
import { saveActivity, useActivitySettings } from '../settings/user/activitySettings';
import { getFollows, getUpdates } from './api';
import { progressApi, progressLine } from './progress';
import { getUnread, socialKeys } from './social';
import type { Shelf } from './types';
import { AgeBadge, formatDate, SeriesCard, timeAgo, updateLine } from './ui';

export function UpdatesPage() {
  const t = useT();
  const [following, setFollowing] = useState(false);
  const q = useInfiniteQuery({
    queryKey: ['library', 'updates', 'infinite', following],
    queryFn: ({ pageParam }) => getUpdates({ following, before: pageParam, limit: 30 }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_before ?? undefined,
  });
  const items = q.data?.pages.flatMap((p) => p.items) ?? [];
  return (
    <div className="lib-updates">
      <div className="lib-page-head">
        <h1 className="lib-h1">{t('library.pages.updates.title')}</h1>
        <SegmentedControl
          label={t('library.pages.updates.showFrom')}
          size="sm"
          value={following ? 'following' : 'all'}
          onValueChange={(v) => setFollowing(v === 'following')}
          options={[
            { value: 'all', label: t('library.pages.updates.allSeries') },
            { value: 'following', label: t('library.pages.updates.following') },
          ]}
        />
      </div>
      {q.isPending ? (
        <Skeleton height={200} />
      ) : q.isError ? (
        <Notice variant="danger" title={t('library.pages.updates.loadError')} />
      ) : items.length === 0 ? (
        <EmptyState
          title={
            following
              ? t('library.pages.updates.emptyFollowingTitle')
              : t('library.pages.updates.emptyTitle')
          }
          description={following ? t('library.pages.updates.emptyFollowingDescription') : undefined}
        />
      ) : (
        <>
          <ul className="lib-update-list lib-update-list-wide">
            {items.map((u) => (
              <li key={u.chapter_id} className="lib-update-row">
                <Link
                  to="/library/s/$seriesId"
                  params={{ seriesId: u.series_id }}
                  className="lib-update-cover"
                  aria-hidden
                  tabIndex={-1}
                >
                  {u.cover_url ? (
                    <img src={u.cover_url} alt="" loading="lazy" />
                  ) : (
                    <span>{u.series_title.charAt(0)}</span>
                  )}
                </Link>
                <div className="lib-update-text">
                  <Link
                    to="/library/s/$seriesId"
                    params={{ seriesId: u.series_id }}
                    className="lib-update-title"
                  >
                    {u.series_title}
                  </Link>
                  {u.age_restricted && <AgeBadge />}
                  <span className="lib-update-line">
                    {updateLine(u)}
                    {u.chapter_title
                      ? t('library.pages.updates.chapterTitle', { title: u.chapter_title })
                      : ''}
                  </span>
                </div>
                <span className="lib-update-group">
                  {[u.release_group, timeAgo(u.published_at_ms)].filter(Boolean).join(' · ')}
                </span>
                <Link
                  to="/library/r/$chapterId/{-$page}"
                  params={{ chapterId: u.chapter_id }}
                  className="lib-icon-btn"
                  aria-label={t('library.pages.updates.read', {
                    title: u.series_title,
                    n: u.chapter_number,
                  })}
                >
                  <Icon name="play" size={20} />
                </Link>
              </li>
            ))}
          </ul>
          {q.hasNextPage && (
            <div className="lib-more">
              <Button
                variant="secondary"
                disabled={q.isFetchingNextPage}
                onClick={() => void q.fetchNextPage()}
              >
                {q.isFetchingNextPage
                  ? t('library.pages.updates.loading')
                  : t('library.pages.updates.loadMore')}
              </Button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

const SHELVES: { value: Shelf; labelKey: MessageKey }[] = [
  { value: 'reading', labelKey: 'library.pages.shelf.reading' },
  { value: 'plan_to_read', labelKey: 'library.pages.shelf.planToRead' },
  { value: 'completed', labelKey: 'library.pages.shelf.completed' },
  { value: 'on_hold', labelKey: 'library.pages.shelf.onHold' },
  { value: 'dropped', labelKey: 'library.pages.shelf.dropped' },
];

type LibrarySort = 'followed' | 'updated' | 'title' | 'unread' | 'read';

/** My library: the followed series by shelf, with unread counts and sorting (LIB-SOC-008). */
export function MyLibraryPage() {
  const t = useT();
  const [shelf, setShelf] = useState<Shelf | null>(null);
  const [sort, setSort] = useState<LibrarySort>('followed');
  const q = useQuery({
    queryKey: ['library', 'follows', shelf, 'page'],
    queryFn: () => getFollows({ shelf: shelf ?? undefined, limit: 60 }),
  });
  const unread = useQuery({ queryKey: socialKeys.unread(), queryFn: getUnread });
  const counts = useMemo(
    () => new Map((unread.data?.items ?? []).map((u) => [u.series_id, u])),
    [unread.data],
  );
  const items = useMemo(() => {
    const list = [...(q.data?.items ?? [])];
    const un = (id: string) => counts.get(id)?.unread ?? 0;
    const read = (id: string) => counts.get(id)?.last_read_at_ms ?? 0;
    switch (sort) {
      case 'title':
        return list.sort((a, b) => a.series.title.localeCompare(b.series.title));
      case 'updated':
        return list.sort(
          (a, b) => (b.series.last_chapter_at_ms ?? 0) - (a.series.last_chapter_at_ms ?? 0),
        );
      case 'unread':
        return list.sort((a, b) => un(b.series.id) - un(a.series.id));
      case 'read':
        return list.sort((a, b) => read(b.series.id) - read(a.series.id));
      default:
        return list.sort((a, b) => b.follow.created_at_ms - a.follow.created_at_ms);
    }
  }, [q.data, sort, counts]);
  return (
    <div className="lib-mylib">
      <div className="lib-page-head">
        <h1 className="lib-h1">{t('library.pages.myLibrary.title')}</h1>
        <label className="lib-select">
          <span>{t('library.pages.myLibrary.sort')}</span>
          <select value={sort} onChange={(e) => setSort(e.target.value as LibrarySort)}>
            <option value="followed">{t('library.pages.myLibrary.sortFollowed')}</option>
            <option value="read">{t('library.pages.myLibrary.sortRead')}</option>
            <option value="updated">{t('library.pages.myLibrary.sortUpdated')}</option>
            <option value="unread">{t('library.pages.myLibrary.sortUnread')}</option>
            <option value="title">{t('library.pages.myLibrary.sortTitle')}</option>
          </select>
        </label>
      </div>
      <FilterChips
        label={t('library.pages.myLibrary.shelf')}
        value={shelf}
        onValueChange={(v) => setShelf(v as Shelf | null)}
        options={SHELVES.map((s) => ({ value: s.value, label: t(s.labelKey) }))}
      />
      {q.isPending ? (
        <Skeleton height={200} />
      ) : q.isError ? (
        <Notice variant="danger" title={t('library.pages.myLibrary.loadError')} />
      ) : q.data.items.length === 0 ? (
        <EmptyState
          title={t('library.pages.myLibrary.emptyTitle')}
          description={t('library.pages.myLibrary.emptyDescription')}
          action={
            <Link to="/library/browse" search={{}} className="lib-btn">
              {t('library.pages.myLibrary.browse')}
            </Link>
          }
        />
      ) : (
        <div className="lib-grid">
          {items.map((f) => {
            const n = counts.get(f.series.id)?.unread ?? 0;
            return (
              <SeriesCard
                key={f.series.id}
                series={f.series}
                tag={n > 0 ? t('library.pages.myLibrary.unread', { n }) : undefined}
              />
            );
          })}
        </div>
      )}
    </div>
  );
}

/** Reading history: everything this device (or, once wired, the account) has opened, newest first. */
/** Local-day bucket key of a timestamp (the timeline groups by it). */
const dayKey = (ms: number) => {
  const d = new Date(ms);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
};

export function HistoryPage() {
  const t = useT();
  const qc = useQueryClient();
  // Pausing the history is the Activity Privacy switch (D-190): while it is on nothing you read is shared with
  // friends or counted in the Friends and People-you-know rankings. Your own progress is still saved.
  const activity = useActivitySettings();
  const pause = useMutation({ mutationFn: (paused: boolean) => saveActivity({ paused }) });
  const q = useQuery({
    queryKey: ['library', 'recent', 'all'],
    queryFn: () => progressApi.recent(50),
  });
  const remove = useMutation({
    mutationFn: (chapterId: string) => progressApi.remove(chapterId),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['library', 'recent'] });
      void qc.invalidateQueries({ queryKey: ['library', 'unread'] });
    },
  });
  const recent = q.data;
  const days = useMemo(() => {
    const out: { key: string; label: string; items: NonNullable<typeof recent> }[] = [];
    const today = dayKey(Date.now());
    const yesterday = dayKey(Date.now() - 86_400_000);
    for (const i of recent ?? []) {
      const key = dayKey(i.updatedAtMs);
      let g = out[out.length - 1];
      if (!g || g.key !== key) {
        g = {
          key,
          label:
            key === today
              ? t('library.pages.history.today')
              : key === yesterday
                ? t('library.pages.history.yesterday')
                : formatDate(i.updatedAtMs),
          items: [],
        };
        out.push(g);
      }
      g.items.push(i);
    }
    return out;
  }, [recent, t]);
  return (
    <div className="lib-history">
      <div className="lib-page-head">
        <h1 className="lib-h1">{t('library.pages.history.title')}</h1>
        <Switch
          label={t('library.pages.history.pause')}
          checked={activity.paused}
          disabled={pause.isPending}
          onChange={(e) => pause.mutate(e.target.checked)}
        />
      </div>
      <p className="lib-muted">
        {activity.paused
          ? t('library.pages.history.pausedNote')
          : t('library.pages.history.pauseHint')}
      </p>
      {pause.isError && <Notice variant="danger" title={t('library.pages.history.pauseError')} />}
      {q.isPending ? (
        <Skeleton height={160} />
      ) : (q.data?.length ?? 0) === 0 ? (
        <EmptyState
          title={t('library.pages.history.emptyTitle')}
          description={t('library.pages.history.emptyDescription')}
        />
      ) : (
        <div className="lib-timeline">
          {days.map((day) => (
            <section key={day.key} aria-label={day.label}>
              <h2 className="lib-h3">{day.label}</h2>
              <ul className="lib-update-list lib-update-list-wide">
                {day.items.map((i) => (
                  <li key={i.chapterId} className="lib-update-row">
                    <Link
                      to="/library/s/$seriesId"
                      params={{ seriesId: i.seriesId }}
                      className="lib-update-cover"
                      aria-hidden
                      tabIndex={-1}
                    >
                      {i.coverUrl ? (
                        <img src={i.coverUrl} alt="" loading="lazy" />
                      ) : (
                        <span>{i.seriesTitle.charAt(0)}</span>
                      )}
                    </Link>
                    <div className="lib-update-text">
                      <Link
                        to="/library/s/$seriesId"
                        params={{ seriesId: i.seriesId }}
                        className="lib-update-title"
                      >
                        {i.seriesTitle}
                      </Link>
                      <span className="lib-update-line">{progressLine(i)}</span>
                    </div>
                    <Link
                      to="/library/r/$chapterId/{-$page}"
                      params={{ chapterId: i.chapterId, page: String(i.pageIndex + 1) }}
                      className="lib-btn"
                    >
                      {t('library.pages.history.continue')}
                    </Link>
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={remove.isPending}
                      aria-label={t('library.pages.history.removeItem', { title: i.seriesTitle })}
                      onClick={() => remove.mutate(i.chapterId)}
                    >
                      <Icon name="close" size={16} />
                    </Button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}

const SOON: Record<string, { title: MessageKey; text: MessageKey }> = {
  requests: {
    title: 'library.pages.soon.requests.title',
    text: 'library.pages.soon.requests.text',
  },
  lists: {
    title: 'library.pages.soon.lists.title',
    text: 'library.pages.soon.lists.text',
  },
  upload: {
    title: 'library.pages.soon.upload.title',
    text: 'library.pages.soon.upload.text',
  },
};

export function SoonPage({ page }: { page: keyof typeof SOON }) {
  const t = useT();
  const s = SOON[page]!;
  return (
    <div>
      <div className="lib-page-head">
        <h1 className="lib-h1">{t(s.title)}</h1>
      </div>
      <EmptyState title={t('library.pages.soon.title')} description={t(s.text)} />
    </div>
  );
}
