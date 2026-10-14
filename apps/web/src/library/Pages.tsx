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
} from '@sideroom/ui';
import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';
import { useT, type MessageKey } from '../i18n';
import { getFollows, getUpdates } from './api';
import { progressApi, progressLine } from './progress';
import type { Shelf } from './types';
import { AgeBadge, SeriesCard, timeAgo, updateLine } from './ui';

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

/** My library: the followed series by shelf (LIB-SOC-008 will add unread counts and sorting). */
export function MyLibraryPage() {
  const t = useT();
  const [shelf, setShelf] = useState<Shelf | null>(null);
  const q = useQuery({
    queryKey: ['library', 'follows', shelf, 'page'],
    queryFn: () => getFollows({ shelf: shelf ?? undefined, limit: 60 }),
  });
  return (
    <div className="lib-mylib">
      <div className="lib-page-head">
        <h1 className="lib-h1">{t('library.pages.myLibrary.title')}</h1>
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
          {q.data.items.map((f) => (
            <SeriesCard key={f.series.id} series={f.series} />
          ))}
        </div>
      )}
    </div>
  );
}

/** Reading history: everything this device (or, once wired, the account) has opened, newest first. */
export function HistoryPage() {
  const t = useT();
  const q = useQuery({
    queryKey: ['library', 'recent', 'all'],
    queryFn: () => progressApi.recent(50),
  });
  return (
    <div className="lib-history">
      <div className="lib-page-head">
        <h1 className="lib-h1">{t('library.pages.history.title')}</h1>
      </div>
      {q.isPending ? (
        <Skeleton height={160} />
      ) : (q.data?.length ?? 0) === 0 ? (
        <EmptyState
          title={t('library.pages.history.emptyTitle')}
          description={t('library.pages.history.emptyDescription')}
        />
      ) : (
        <ul className="lib-update-list lib-update-list-wide">
          {q.data!.map((i) => (
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
            </li>
          ))}
        </ul>
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
