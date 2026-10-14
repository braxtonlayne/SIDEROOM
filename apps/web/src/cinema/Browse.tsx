// Movies / Series / Anime browse pages and search results (CIN-BRW-001..006): header per kind, genre chips,
// collections dropdown, filters drawer, count and poster grid. Filters live in the URL and use the catalog's own
// query names (`GET /cinema/titles`), so a view can be shared and the back button works.
import {
  Button,
  Drawer,
  DisplayHeading,
  DropdownChip,
  EmptyState,
  Eyebrow,
  FilterChips,
  Icon,
  Notice,
  Select,
  TextInput,
} from '@sideroom/ui';
import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useT } from '../i18n';
import { useAgeGate } from '../lib/ageGate';
import { browseTitles, getCollections, getFacets, keys, searchTitles } from './api';
import { validateBrowseSearch, type BrowseSearch } from './browseSearch';
import type { AgeFilter, BrowseParams, SortKey, Title } from './types';
import { CardSkeletons, label, TitleCard } from './ui';

const SORTS: { value: SortKey; label: string }[] = [
  { value: 'updated', label: 'cinema.browse.sortUpdated' },
  { value: 'newest', label: 'cinema.browse.sortNewest' },
  { value: 'title', label: 'cinema.browse.sortTitle' },
  { value: 'year', label: 'cinema.browse.sortYear' },
  { value: 'runtime', label: 'cinema.browse.sortRuntime' },
];
const AGES: { value: AgeFilter; label: string }[] = [
  { value: 'include', label: 'cinema.browse.ageInclude' },
  { value: 'hide', label: 'cinema.browse.ageHide' },
  { value: 'only', label: 'cinema.browse.ageOnly' },
];

type Page = 'movies' | 'series' | 'anime';
interface KindCopy {
  path: '/cinema/movies' | '/cinema/series' | '/cinema/anime';
  kind: 'movie' | 'series' | 'anime';
  /** `surface` of `GET /cinema/collections`. */
  surface: Page;
  /** Catalog keys (heading, accent, sub), resolved at render time. */
  heading: string;
  accent?: string;
  sub: string;
}
export const KIND_COPY: Record<Page, KindCopy> = {
  movies: {
    path: '/cinema/movies',
    kind: 'movie',
    surface: 'movies',
    heading: 'cinema.browse.moviesHeading',
    sub: 'cinema.browse.moviesSub',
  },
  series: {
    path: '/cinema/series',
    kind: 'series',
    surface: 'series',
    heading: 'cinema.browse.seriesHeading',
    accent: 'cinema.browse.seriesAccent',
    sub: 'cinema.browse.seriesSub',
  },
  anime: {
    path: '/cinema/anime',
    kind: 'anime',
    surface: 'anime',
    heading: 'cinema.browse.animeHeading',
    accent: 'cinema.browse.animeAccent',
    sub: 'cinema.browse.animeSub',
  },
};

/** Chips shown before the facets arrive (or when the catalog has no genres yet). */
export const GENRES = [
  'science fiction',
  'adventure',
  'fantasy',
  'drama',
  'thriller',
  'comedy',
  'horror',
];
const SHOWN_GENRES = 7;
const ALL_ROOMS = '__all';
const ANY = '__any';
const PAGE = 24;

function FiltersDrawer({
  open,
  onClose,
  search,
  apply,
  kind,
}: {
  open: boolean;
  onClose: () => void;
  search: BrowseSearch;
  apply: (patch: BrowseSearch) => void;
  kind?: KindCopy['kind'];
}) {
  const t = useT();
  const { ensureAdult } = useAgeGate();
  const facets = useQuery({
    queryKey: keys.facets(kind),
    queryFn: () => getFacets(kind),
    enabled: open,
    staleTime: 5 * 60_000,
  });
  const [d, setD] = useState<BrowseSearch>(search);
  // Re-seed from the URL each time the drawer opens.
  const [wasOpen, setWasOpen] = useState(false);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) setD(search);
  }
  const numField = (key: 'year_from' | 'year_to' | 'runtime_max', text: string, hint?: string) => (
    <TextInput
      label={text}
      hint={hint}
      type="number"
      inputMode="numeric"
      value={d[key] ?? ''}
      onChange={(e) => {
        const n = e.target.value === '' ? undefined : Number(e.target.value);
        setD((x) => ({ ...x, [key]: n }));
      }}
    />
  );
  const choice = (names: string[] | undefined, any: string) => [
    { value: ANY, label: any },
    ...(names ?? []).map((n) => ({ value: n, label: n })),
  ];
  const submit = async () => {
    if (d.age === 'only' && !(await ensureAdult())) return;
    apply({
      year_from: d.year_from,
      year_to: d.year_to,
      rating: d.rating,
      runtime_max: d.runtime_max,
      language: d.language,
      subtitle_language: d.subtitle_language,
      age: d.age,
      sort: d.sort,
    });
    onClose();
  };
  return (
    <Drawer
      open={open}
      onClose={onClose}
      title={t('cinema.browse.filtersTitle')}
      side="right"
      footer={
        <>
          <Button
            variant="secondary"
            onClick={() => {
              apply({
                year_from: undefined,
                year_to: undefined,
                rating: undefined,
                runtime_max: undefined,
                language: undefined,
                subtitle_language: undefined,
                age: undefined,
                sort: undefined,
              });
              onClose();
            }}
          >
            {t('cinema.browse.reset')}
          </Button>
          <Button onClick={() => void submit()}>{t('cinema.browse.apply')}</Button>
        </>
      }
    >
      <div className="cin-filters">
        {numField('year_from', t('cinema.browse.yearFrom'))}
        {numField('year_to', t('cinema.browse.yearTo'))}
        {numField('runtime_max', t('cinema.browse.longestRuntime'), t('cinema.browse.inMinutes'))}
        <Select
          aria-label={t('cinema.browse.rating')}
          placeholder={t('cinema.browse.rating')}
          value={d.rating ?? ANY}
          onValueChange={(v) => setD((x) => ({ ...x, rating: v === ANY ? undefined : v }))}
          options={choice(
            facets.data?.ratings.map((r) => r.name),
            t('cinema.browse.anyRating'),
          )}
        />
        <Select
          aria-label={t('cinema.browse.audioLanguage')}
          placeholder={t('cinema.browse.audioLanguage')}
          value={d.language ?? ANY}
          onValueChange={(v) => setD((x) => ({ ...x, language: v === ANY ? undefined : v }))}
          options={choice(
            facets.data?.languages.map((l) => l.name),
            t('cinema.browse.anyLanguage'),
          )}
        />
        <Select
          aria-label={t('cinema.browse.subtitles')}
          placeholder={t('cinema.browse.subtitles')}
          value={d.subtitle_language ?? ANY}
          onValueChange={(v) =>
            setD((x) => ({ ...x, subtitle_language: v === ANY ? undefined : v }))
          }
          options={choice(
            facets.data?.subtitle_languages.map((l) => l.name),
            t('cinema.browse.anySubtitles'),
          )}
        />
        <Select
          aria-label={t('cinema.browse.ageRestricted')}
          placeholder={t('cinema.browse.ageRestricted')}
          value={d.age ?? 'include'}
          onValueChange={(v) => setD((x) => ({ ...x, age: v as AgeFilter }))}
          options={AGES.map((a) => ({ ...a, label: t(a.label) }))}
        />
        <Select
          aria-label={t('cinema.browse.sortBy')}
          placeholder={t('cinema.browse.sortBy')}
          value={d.sort ?? 'updated'}
          onValueChange={(v) => setD((x) => ({ ...x, sort: v as SortKey }))}
          options={SORTS.map((o) => ({ ...o, label: t(o.label) }))}
        />
      </div>
    </Drawer>
  );
}

/** Count of active drawer filters, for the button badge. */
const activeFilters = (s: BrowseSearch) =>
  [
    s.year_from,
    s.year_to,
    s.rating,
    s.runtime_max,
    s.language,
    s.subtitle_language,
    s.age,
    s.sort,
  ].filter((v) => v !== undefined).length;

export function Browse({ page, search }: { page: Page | 'search'; search: BrowseSearch }) {
  const t = useT();
  const navigate = useNavigate();
  const copy = page === 'search' ? null : KIND_COPY[page];
  const [more, setMore] = useState(false);
  const [drawer, setDrawer] = useState(search.filters === 1);

  const setSearch = (patch: BrowseSearch) => {
    const next = validateBrowseSearch({ ...search, ...patch, filters: undefined });
    void navigate({ to: copy?.path ?? '/cinema/search', search: next, replace: true });
  };

  const params: BrowseParams = {
    kind: copy?.kind,
    genres: search.genres,
    collection: search.collection,
    year_from: search.year_from,
    year_to: search.year_to,
    rating: search.rating,
    runtime_max: search.runtime_max,
    language: search.language,
    subtitle_language: search.subtitle_language,
    age: search.age,
    sort: search.collection && !search.sort ? 'curated' : search.sort,
  } as BrowseParams;
  const searching = page === 'search';
  const list = useInfiniteQuery({
    queryKey: keys.browse(params),
    queryFn: ({ pageParam }) => browseTitles({ ...params, limit: PAGE, cursor: pageParam }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_cursor ?? undefined,
    enabled: !searching,
  });
  // The search page has no filters in the catalog API (search is its own route): one page of matches.
  const q = (search.q ?? '').trim();
  const found = useQuery({
    queryKey: keys.search(`page:${q}`),
    queryFn: ({ signal }) => searchTitles(q, 50, signal),
    enabled: searching && q.length >= 2,
  });
  const facets = useQuery({
    queryKey: keys.facets(copy?.kind),
    queryFn: () => getFacets(copy?.kind),
    enabled: !searching,
    staleTime: 5 * 60_000,
  });
  const collections = useQuery({
    queryKey: keys.collections(copy?.surface),
    queryFn: () => getCollections(copy?.surface),
    enabled: !searching,
    staleTime: 5 * 60_000,
  });

  let items: Title[];
  let total: number;
  let pending: boolean;
  let failed: boolean;
  if (searching) {
    items = found.data?.titles ?? [];
    total = items.length;
    pending = found.isPending && q.length >= 2;
    failed = found.isError;
  } else {
    items = list.data?.pages.flatMap((p) => p.items) ?? [];
    total = list.data?.pages[0]?.total ?? 0;
    pending = list.isPending;
    failed = list.isError;
  }
  const done = !pending && !failed;

  const names = facets.data?.genres.length ? facets.data.genres.map((g) => g.name) : GENRES;
  const genres = more ? names : names.slice(0, SHOWN_GENRES);
  const anime = page === 'anime';
  const nFilters = activeFilters(search);

  return (
    <div className="cin-page" data-anime={anime || undefined}>
      <header className="cin-browse-head">
        <Eyebrow>{t('cinema.browse.eyebrow')}</Eyebrow>
        {copy ? (
          <DisplayHeading as="h1" size="xl" accent={copy.accent ? t(copy.accent) : undefined}>
            {t(copy.heading)}
          </DisplayHeading>
        ) : (
          <DisplayHeading as="h1" size="xl">
            {q ? t('cinema.browse.resultsFor', { q }) : t('cinema.browse.searchHeading')}
          </DisplayHeading>
        )}
        <p className="cin-sub">{copy ? t(copy.sub) : t('cinema.browse.searchSub')}</p>
      </header>

      {!searching && (
        <div className="cin-chips">
          <button
            type="button"
            className="cin-chip-all"
            aria-pressed={!search.genres}
            onClick={() => setSearch({ genres: undefined })}
          >
            {t('cinema.browse.all')}
          </button>
          <FilterChips
            label={t('cinema.browse.genres')}
            options={genres.map((g) => ({ value: g, label: label(g) }))}
            value={search.genres ?? null}
            onValueChange={(v) => setSearch({ genres: v ?? undefined })}
          />
          {names.length > SHOWN_GENRES && (
            <button
              type="button"
              className="cin-chip-more"
              aria-expanded={more}
              onClick={() => setMore((m) => !m)}
            >
              {more ? t('cinema.browse.fewer') : t('cinema.browse.more')}
            </button>
          )}
          <DropdownChip
            label={t('cinema.browse.roomSelection')}
            value={search.collection ?? ALL_ROOMS}
            options={[
              { value: ALL_ROOMS, label: t('cinema.browse.allRooms') },
              ...(collections.data?.items ?? []).map((c) => ({ value: c.id, label: c.title })),
            ]}
            onValueChange={(v) => setSearch({ collection: v === ALL_ROOMS ? undefined : v })}
          />
          <button
            type="button"
            className="cin-filters-btn"
            aria-haspopup="dialog"
            onClick={() => setDrawer(true)}
          >
            <Icon name="filter" size={16} /> {t('cinema.browse.filters')}
            {nFilters ? ` (${nFilters})` : ''}
          </button>
        </div>
      )}

      <p className="cin-count" aria-live="polite">
        {done ? t('cinema.browse.count', { n: total }) : ' '}
      </p>

      {pending && <CardSkeletons n={6} />}
      {failed && (
        <Notice variant="danger" title={t('cinema.browse.loadErrorTitle')}>
          {t('cinema.browse.loadErrorBody')}
        </Notice>
      )}
      {done && items.length === 0 && (
        <EmptyState
          title={t('cinema.browse.emptyTitle')}
          description={searching ? t('cinema.browse.emptySearch') : t('cinema.browse.emptyBrowse')}
        />
      )}
      {items.length > 0 && (
        <>
          <h2 className="cin-sr-only">{t('cinema.browse.titlesHeading')}</h2>
          <ul className="cin-grid">
            {items.map((it) => (
              <li key={it.id}>
                <TitleCard title={it} />
              </li>
            ))}
          </ul>
        </>
      )}
      {!searching && list.hasNextPage && (
        <div className="cin-more">
          <Button
            variant="secondary"
            disabled={list.isFetchingNextPage}
            onClick={() => void list.fetchNextPage()}
          >
            {list.isFetchingNextPage ? t('cinema.browse.loading') : t('cinema.browse.loadMore')}
          </Button>
        </div>
      )}

      {!searching && (
        <FiltersDrawer
          open={drawer}
          onClose={() => setDrawer(false)}
          search={search}
          apply={setSearch}
          kind={copy?.kind}
        />
      )}
    </div>
  );
}
