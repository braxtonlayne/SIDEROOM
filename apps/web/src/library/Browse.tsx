// Browse (LIB-BRW-001): kind tabs, filter drawer, sort, grid/list, paginated results. Filters live in the URL
// search string so a view can be shared and the back button works.
import {
  Button,
  Drawer,
  EmptyState,
  FilterChips,
  Icon,
  Notice,
  SegmentedControl,
  Skeleton,
  TextInput,
} from '@sideroom/ui';
import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { Link, useNavigate } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { useT, type MessageKey } from '../i18n';
import { browseSeries, keys, searchLibrary } from './api';
import { GenrePicker, TagField } from './BrowseFilters';
import { VirtualGrid } from './VirtualGrid';
import { defaultAgeFilter, useLibrarySettings } from './prefs';
import type { AgeFilter, BrowseParams, SeriesKind, SeriesStatus, SortKey } from './types';
import { AgeBadge, KIND_LABEL, label, SeriesCard, STATUS_LABEL } from './ui';

export interface BrowseSearch {
  q?: string;
  kind?: SeriesKind;
  status?: SeriesStatus;
  genres?: string;
  exclude?: string;
  tags?: string;
  language?: string;
  year_from?: number;
  year_to?: number;
  rating_min?: number;
  age?: AgeFilter;
  sort?: SortKey;
  order?: 'asc' | 'desc';
  view?: 'grid' | 'list';
}

const KINDS = Object.keys(KIND_LABEL) as SeriesKind[];
const STATUSES = Object.keys(STATUS_LABEL) as SeriesStatus[];
const SORTS: { value: SortKey; labelKey: MessageKey }[] = [
  { value: 'updated', labelKey: 'library.browse.sort.updated' },
  { value: 'newest', labelKey: 'library.browse.sort.newest' },
  { value: 'title', labelKey: 'library.browse.sort.title' },
  { value: 'followers', labelKey: 'library.browse.sort.followers' },
  { value: 'rating', labelKey: 'library.browse.sort.rating' },
];
const AGES: AgeFilter[] = ['include', 'hide', 'only'];
const PAGE = 24;
const DEFAULT_GENRES = [
  'action',
  'adventure',
  'comedy',
  'drama',
  'fantasy',
  'horror',
  'mystery',
  'romance',
  'slice of life',
  'sci-fi',
];

const oneOf = <T extends string>(v: unknown, all: readonly T[]): T | undefined =>
  typeof v === 'string' && (all as readonly string[]).includes(v) ? (v as T) : undefined;
const str = (v: unknown) => (typeof v === 'string' && v.trim() ? v.trim() : undefined);
const num = (v: unknown) => {
  const n = typeof v === 'string' ? Number(v) : v;
  return typeof n === 'number' && Number.isInteger(n) ? n : undefined;
};

/** Router `validateSearch`: unknown keys and bad values are dropped, never thrown. */
export function validateBrowseSearch(s: Record<string, unknown>): BrowseSearch {
  const out: BrowseSearch = {};
  const set = <K extends keyof BrowseSearch>(k: K, v: BrowseSearch[K] | undefined) => {
    if (v !== undefined) out[k] = v;
  };
  set('q', str(s.q));
  set('kind', oneOf(s.kind, KINDS));
  set('status', oneOf(s.status, STATUSES));
  set('genres', str(s.genres));
  set('exclude', str(s.exclude));
  set('tags', str(s.tags));
  set('language', str(s.language));
  set('year_from', num(s.year_from));
  set('year_to', num(s.year_to));
  set('rating_min', ((n) => (n !== undefined && n >= 1 && n <= 10 ? n : undefined))(num(s.rating_min)));
  set('age', oneOf(s.age, AGES));
  set(
    'sort',
    oneOf(
      s.sort,
      SORTS.map((x) => x.value),
    ),
  );
  set('order', oneOf(s.order, ['asc', 'desc'] as const));
  set('view', oneOf(s.view, ['grid', 'list'] as const));
  return out;
}

const csv = (v?: string) =>
  v
    ? v
        .split(',')
        .map((x) => x.trim())
        .filter(Boolean)
    : undefined;

export function toParams(s: BrowseSearch): BrowseParams {
  return {
    kind: s.kind,
    status: s.status,
    genres: csv(s.genres),
    exclude_genres: csv(s.exclude),
    tags: csv(s.tags),
    language: s.language,
    year_from: s.year_from,
    year_to: s.year_to,
    rating_min: s.rating_min,
    age: s.age,
    sort: s.sort,
    order: s.order,
  };
}

const AGE_LABEL: Record<AgeFilter, MessageKey> = {
  include: 'library.browse.age.include',
  hide: 'library.browse.age.hide',
  only: 'library.browse.age.only',
};

export function Browse({ search }: { search: BrowseSearch }) {
  const t = useT();
  const navigate = useNavigate();
  const [filters, setFilters] = useState(false);
  const { hideRestricted } = useLibrarySettings();
  const view = search.view ?? 'grid';
  const go = (patch: Partial<BrowseSearch>) =>
    void navigate({ to: '/library/browse', search: { ...search, ...patch } });

  const active = [
    'status',
    'genres',
    'exclude',
    'tags',
    'language',
    'year_from',
    'year_to',
    'rating_min',
    'age',
  ].filter((k) => search[k as keyof BrowseSearch] !== undefined).length;

  return (
    <div className="lib-browse">
      <div className="lib-page-head">
        <h1 className="lib-h1">
          {search.q ? t('library.browse.resultsFor', { q: search.q }) : t('library.browse.title')}
        </h1>
        {search.q && (
          <Link to="/library/browse" search={{}} className="lib-link">
            {t('library.browse.clearSearch')}
          </Link>
        )}
      </div>

      {search.q ? (
        <SearchResults q={search.q} view={view} />
      ) : (
        <>
          <div className="lib-browse-bar">
            <FilterChips
              label={t('library.browse.kind')}
              value={search.kind ?? null}
              onValueChange={(v) => go({ kind: (v as SeriesKind | null) ?? undefined })}
              options={KINDS.map((k) => ({ value: k, label: KIND_LABEL[k] }))}
            />
            <div className="lib-browse-tools">
              <label className="lib-select">
                <span className="lib-sr-only">{t('library.browse.sortBy')}</span>
                <select
                  value={search.sort ?? 'updated'}
                  onChange={(e) => go({ sort: e.target.value as SortKey })}
                >
                  {SORTS.map((s) => (
                    <option key={s.value} value={s.value}>
                      {t(s.labelKey)}
                    </option>
                  ))}
                </select>
              </label>
              <button
                type="button"
                className="lib-icon-btn"
                aria-label={
                  search.order === 'asc'
                    ? t('library.browse.sortedAsc')
                    : t('library.browse.sortedDesc')
                }
                aria-pressed={search.order === 'asc'}
                onClick={() => go({ order: search.order === 'asc' ? 'desc' : 'asc' })}
              >
                <Icon name="filter" size={20} />
              </button>
              <SegmentedControl
                label={t('library.browse.layout')}
                size="sm"
                value={view}
                onValueChange={(v) => go({ view: v as 'grid' | 'list' })}
                options={[
                  { value: 'grid', label: t('library.browse.grid') },
                  { value: 'list', label: t('library.browse.list') },
                ]}
              />
              <Button variant="secondary" size="sm" onClick={() => setFilters(true)}>
                {active
                  ? t('library.browse.filtersCount', { n: active })
                  : t('library.browse.filters')}
              </Button>
            </div>
          </div>
          <Results params={toParams(search)} view={view} />
        </>
      )}

      <FilterDrawer
        open={filters}
        onClose={() => setFilters(false)}
        search={search}
        ageDefault={defaultAgeFilter(hideRestricted)}
        onApply={(patch) => {
          setFilters(false);
          go(patch);
        }}
        onReset={() => {
          setFilters(false);
          void navigate({
            to: '/library/browse',
            search: { kind: search.kind, sort: search.sort, view: search.view },
          });
        }}
      />
    </div>
  );
}

/* ---------------------------------------------------------------- results */

function Results({ params, view }: { params: BrowseParams; view: 'grid' | 'list' }) {
  const t = useT();
  const q = useInfiniteQuery({
    queryKey: ['library', 'browse', 'infinite', params],
    queryFn: ({ pageParam }) => browseSeries({ ...params, limit: PAGE, offset: pageParam }),
    initialPageParam: 0,
    getNextPageParam: (last) => {
      const next = last.offset + last.items.length;
      return next < last.total ? next : undefined;
    },
  });
  const sentinel = useRef<HTMLDivElement>(null);
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = q;
  useEffect(() => {
    const el = sentinel.current;
    if (!el || !hasNextPage || typeof IntersectionObserver === 'undefined') return;
    const io = new IntersectionObserver(
      (e) => {
        if (e.some((x) => x.isIntersecting) && !isFetchingNextPage) void fetchNextPage();
      },
      { rootMargin: '400px' },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);

  if (q.isPending) return <SkeletonGrid />;
  if (q.isError)
    return (
      <Notice variant="danger" title={t('library.browse.loadError')}>
        <Button size="sm" onClick={() => void q.refetch()}>
          {t('library.browse.tryAgain')}
        </Button>
      </Notice>
    );
  const items = q.data.pages.flatMap((p) => p.items);
  const total = q.data.pages[0]?.total ?? 0;
  if (items.length === 0)
    return (
      <EmptyState
        title={t('library.browse.noMatchTitle')}
        description={t('library.browse.noMatchDescription')}
      />
    );
  return (
    <>
      <p className="lib-count" role="status">
        {t('library.browse.count', { total })}
      </p>
      <SeriesList items={items} view={view} />
      <div ref={sentinel} />
      {hasNextPage && (
        <div className="lib-more">
          <Button
            variant="secondary"
            disabled={isFetchingNextPage}
            onClick={() => void fetchNextPage()}
          >
            {isFetchingNextPage ? t('library.browse.loading') : t('library.browse.loadMore')}
          </Button>
        </div>
      )}
    </>
  );
}

function SearchResults({ q, view }: { q: string; view: 'grid' | 'list' }) {
  const t = useT();
  const res = useQuery({
    queryKey: ['library', 'search-page', q],
    queryFn: () => searchLibrary(q, 50),
  });
  if (res.isPending) return <SkeletonGrid />;
  if (res.isError) return <Notice variant="danger" title={t('library.browse.searchUnavailable')} />;
  if (res.data.series.length === 0)
    return (
      <EmptyState
        title={t('library.browse.nothingFound')}
        description={t('library.browse.noSeriesMatch', { q })}
      />
    );
  return <SeriesList items={res.data.series} view={view} />;
}

function SeriesList({ items, view }: { items: import('./types').Series[]; view: 'grid' | 'list' }) {
  const t = useT();
  const heading = <h2 className="lib-sr-only">{t('library.browse.seriesHeading')}</h2>;
  if (view === 'list')
    return (
      <>
        {heading}
        <ul className="lib-list-view">
          {items.map((s) => (
            <li key={s.id} className="lib-list-row">
              <Link
                to="/library/s/$seriesId"
                params={{ seriesId: s.id }}
                className="lib-list-cover"
                aria-hidden
                tabIndex={-1}
              >
                {s.cover_url ? (
                  <img src={s.cover_url} alt="" loading="lazy" />
                ) : (
                  <span>{s.title.charAt(0)}</span>
                )}
              </Link>
              <div className="lib-list-text">
                <Link
                  to="/library/s/$seriesId"
                  params={{ seriesId: s.id }}
                  className="lib-list-name"
                >
                  {s.title}
                </Link>
                {s.age_restricted && <AgeBadge />}
                <span className="lib-muted">
                  {t('library.browse.listMeta', {
                    kind: KIND_LABEL[s.kind],
                    status: STATUS_LABEL[s.status],
                    n: s.chapter_count,
                  })}
                </span>
                <span className="lib-muted">{s.genres.map(label).join(' · ')}</span>
              </div>
            </li>
          ))}
        </ul>
      </>
    );
  return (
    <>
      {heading}
      <VirtualGrid items={items} keyOf={(s) => s.id} render={(s) => <SeriesCard series={s} />} />
    </>
  );
}

function SkeletonGrid() {
  return (
    <div className="lib-grid" aria-hidden="true">
      {Array.from({ length: 10 }, (_, i) => (
        <Skeleton key={i} height={260} />
      ))}
    </div>
  );
}

/* ----------------------------------------------------------- filter drawer */

function FilterDrawer({
  open,
  onClose,
  search,
  ageDefault,
  onApply,
  onReset,
}: {
  open: boolean;
  onClose: () => void;
  search: BrowseSearch;
  ageDefault: AgeFilter;
  onApply: (patch: Partial<BrowseSearch>) => void;
  onReset: () => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState<Record<string, string>>({});
  // Suggested genres: the common ones plus those on the most followed series.
  const popular = useQuery({
    queryKey: keys.browse({ sort: 'followers', limit: 24 }),
    queryFn: () => browseSeries({ sort: 'followers', limit: 24 }),
    enabled: open,
    staleTime: 5 * 60_000,
  });
  const suggestions = [
    ...new Set([...DEFAULT_GENRES, ...(popular.data?.items ?? []).flatMap((s) => s.genres)]),
  ];
  useEffect(() => {
    if (open)
      setDraft({
        genres: search.genres ?? '',
        exclude: search.exclude ?? '',
        tags: search.tags ?? '',
        language: search.language ?? '',
        year_from: search.year_from?.toString() ?? '',
        year_to: search.year_to?.toString() ?? '',
        rating_min: search.rating_min?.toString() ?? '',
        status: search.status ?? '',
        age: search.age ?? ageDefault,
      });
  }, [open, search, ageDefault]);
  const field = (k: string) => ({
    value: draft[k] ?? '',
    onChange: (e: React.ChangeEvent<HTMLInputElement>) =>
      setDraft((d) => ({ ...d, [k]: e.target.value })),
  });
  const apply = () => {
    const v = validateBrowseSearch({
      genres: draft.genres,
      exclude: draft.exclude,
      tags: draft.tags,
      language: draft.language,
      year_from: draft.year_from,
      year_to: draft.year_to,
      rating_min: draft.rating_min,
      status: draft.status,
      age: draft.age === ageDefault ? undefined : draft.age,
    });
    onApply({
      genres: v.genres,
      exclude: v.exclude,
      tags: v.tags,
      language: v.language,
      year_from: v.year_from,
      year_to: v.year_to,
      rating_min: v.rating_min,
      status: v.status,
      age: v.age,
    });
  };
  return (
    <Drawer
      open={open}
      onClose={onClose}
      title={t('library.browse.filters')}
      side="right"
      footer={
        <>
          <Button variant="secondary" onClick={onReset}>
            {t('library.browse.reset')}
          </Button>
          <Button onClick={apply}>{t('library.browse.showResults')}</Button>
        </>
      }
    >
      <div className="lib-filters">
        <GenrePicker
          include={draft.genres ?? ''}
          exclude={draft.exclude ?? ''}
          options={suggestions}
          onChange={(n) => setDraft((d) => ({ ...d, genres: n.genres, exclude: n.exclude }))}
        />
        <TagField value={draft.tags ?? ''} onChange={(tags) => setDraft((d) => ({ ...d, tags }))} />
        <label className="lib-select lib-select-block">
          <span>{t('library.browse.status')}</span>
          <select
            value={draft.status ?? ''}
            onChange={(e) => setDraft((d) => ({ ...d, status: e.target.value }))}
          >
            <option value="">{t('library.browse.any')}</option>
            {STATUSES.map((s) => (
              <option key={s} value={s}>
                {STATUS_LABEL[s]}
              </option>
            ))}
          </select>
        </label>
        <TextInput
          label={t('library.browse.language')}
          hint={t('library.browse.languageHint')}
          {...field('language')}
        />
        <div className="lib-filters-years">
          <TextInput
            label={t('library.browse.yearFrom')}
            inputMode="numeric"
            {...field('year_from')}
          />
          <TextInput label={t('library.browse.yearTo')} inputMode="numeric" {...field('year_to')} />
        </div>
        <label className="lib-select lib-select-block">
          <span>{t('library.browse.ratingMin')}</span>
          <select
            value={draft.rating_min ?? ''}
            onChange={(e) => setDraft((d) => ({ ...d, rating_min: e.target.value }))}
          >
            <option value="">{t('library.browse.any')}</option>
            {[5, 6, 7, 8, 9].map((n) => (
              <option key={n} value={n}>
                {t('library.browse.ratingAtLeast', { n })}
              </option>
            ))}
          </select>
        </label>
        <label className="lib-select lib-select-block">
          <span>{t('library.browse.ageRestricted')}</span>
          <select
            value={draft.age ?? ageDefault}
            onChange={(e) => setDraft((d) => ({ ...d, age: e.target.value }))}
          >
            {AGES.map((a) => (
              <option key={a} value={a}>
                {t(AGE_LABEL[a])}
              </option>
            ))}
          </select>
        </label>
      </div>
    </Drawer>
  );
}
