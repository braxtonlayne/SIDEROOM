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
import { getFollows, getUpdates } from './api';
import { progressApi, progressLine } from './progress';
import type { Shelf } from './types';
import { AgeBadge, SeriesCard, timeAgo, updateLine } from './ui';

export function UpdatesPage() {
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
        <h1 className="lib-h1">Latest updates</h1>
        <SegmentedControl
          label="Show updates from"
          size="sm"
          value={following ? 'following' : 'all'}
          onValueChange={(v) => setFollowing(v === 'following')}
          options={[
            { value: 'all', label: 'All series' },
            { value: 'following', label: 'Following' },
          ]}
        />
      </div>
      {q.isPending ? (
        <Skeleton height={200} />
      ) : q.isError ? (
        <Notice variant="danger" title="We could not load updates" />
      ) : items.length === 0 ? (
        <EmptyState
          title={following ? 'Nothing new from your follows' : 'No chapters yet'}
          description={following ? 'Follow a series to see its new chapters here.' : undefined}
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
                    {u.chapter_title ? `: ${u.chapter_title}` : ''}
                  </span>
                </div>
                <span className="lib-update-group">
                  {[u.release_group, timeAgo(u.published_at_ms)].filter(Boolean).join(' · ')}
                </span>
                <Link
                  to="/library/r/$chapterId/{-$page}"
                  params={{ chapterId: u.chapter_id }}
                  className="lib-icon-btn"
                  aria-label={`Read ${u.series_title} chapter ${u.chapter_number}`}
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
                {q.isFetchingNextPage ? 'Loading…' : 'Load more'}
              </Button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

const SHELVES: { value: Shelf; label: string }[] = [
  { value: 'reading', label: 'Reading' },
  { value: 'plan_to_read', label: 'Plan to read' },
  { value: 'completed', label: 'Completed' },
  { value: 'on_hold', label: 'On hold' },
  { value: 'dropped', label: 'Dropped' },
];

/** My library: the followed series by shelf (LIB-SOC-008 will add unread counts and sorting). */
export function MyLibraryPage() {
  const [shelf, setShelf] = useState<Shelf | null>(null);
  const q = useQuery({
    queryKey: ['library', 'follows', shelf, 'page'],
    queryFn: () => getFollows({ shelf: shelf ?? undefined, limit: 60 }),
  });
  return (
    <div className="lib-mylib">
      <div className="lib-page-head">
        <h1 className="lib-h1">My library</h1>
      </div>
      <FilterChips
        label="Shelf"
        value={shelf}
        onValueChange={(v) => setShelf(v as Shelf | null)}
        options={SHELVES.map((s) => ({ value: s.value, label: s.label }))}
      />
      {q.isPending ? (
        <Skeleton height={200} />
      ) : q.isError ? (
        <Notice variant="danger" title="We could not load your library" />
      ) : q.data.items.length === 0 ? (
        <EmptyState
          title="Nothing here yet"
          description="Follow a series from its page and it will show up on your shelves."
          action={
            <Link to="/library/browse" search={{}} className="lib-btn">
              Browse the library
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
  const q = useQuery({
    queryKey: ['library', 'recent', 'all'],
    queryFn: () => progressApi.recent(50),
  });
  return (
    <div className="lib-history">
      <div className="lib-page-head">
        <h1 className="lib-h1">Reading history</h1>
      </div>
      {q.isPending ? (
        <Skeleton height={160} />
      ) : (q.data?.length ?? 0) === 0 ? (
        <EmptyState title="No reading yet" description="Chapters you open will be listed here." />
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
                Continue
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

const SOON: Record<string, { title: string; text: string }> = {
  requests: {
    title: 'Requests',
    text: 'Asking for a series to be added arrives with the submissions work.',
  },
  lists: {
    title: 'Reading lists',
    text: 'Lists you can order, share and feature arrive with LIB-SOC-010.',
  },
  upload: { title: 'Upload', text: 'Member uploads arrive with the submissions pipeline.' },
};

export function SoonPage({ page }: { page: keyof typeof SOON }) {
  const s = SOON[page]!;
  return (
    <div>
      <div className="lib-page-head">
        <h1 className="lib-h1">{s.title}</h1>
      </div>
      <EmptyState title="Not open yet" description={s.text} />
    </div>
  );
}
