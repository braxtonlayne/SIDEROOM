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
import { useAgeGate } from '../lib/ageGate';
import { browseTitles, getCollections, getFacets, keys, searchTitles } from './api';
import { validateBrowseSearch, type BrowseSearch } from './browseSearch';
import type { AgeFilter, BrowseParams, SortKey, Title } from './types';
import { CardSkeletons, label, TitleCard } from './ui';

const SORTS: { value: SortKey; label: string }[] = [
  { value: 'updated', label: 'Recently updated' },
  { value: 'newest', label: 'Newest added' },
  { value: 'title', label: 'A to Z' },
  { value: 'year', label: 'Release year' },
  { value: 'runtime', label: 'Shortest first' },
];
const AGES: { value: AgeFilter; label: string }[] = [
  { value: 'include', label: 'Show age-restricted titles' },
  { value: 'hide', label: 'Hide age-restricted titles' },
  { value: 'only', label: 'Only age-restricted titles' },
];

type Page = 'movies' | 'series' | 'anime';
interface KindCopy {
  path: '/cinema/movies' | '/cinema/series' | '/cinema/anime';
  kind: 'movie' | 'series' | 'anime';
  /** `surface` of `GET /cinema/collections`. */
  surface: Page;
  heading: string;
  accent?: string;
  sub: string;
}
export const KIND_COPY: Record<Page, KindCopy> = {
  movies: {
    path: '/cinema/movies',
    kind: 'movie',
    surface: 'movies',
    heading: 'The cinema.',
    sub: 'Films for a night well spent.',
  },
  series: {
    path: '/cinema/series',
    kind: 'series',
    surface: 'series',
    heading: 'One more',
    accent: 'episode.',
    sub: 'Stories that take their time.',
  },
  anime: {
    path: '/cinema/anime',
    kind: 'anime',
    surface: 'anime',
    heading: 'The anime',
    accent: 'room. ✦',
    sub: 'Quiet journeys and worlds on the edge.',
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
      title="Filters"
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
            Reset
          </Button>
          <Button onClick={() => void submit()}>Apply</Button>
        </>
      }
    >
      <div className="cin-filters">
        {numField('year_from', 'Year from')}
        {numField('year_to', 'Year to')}
        {numField('runtime_max', 'Longest runtime', 'In minutes')}
        <Select
          aria-label="Rating"
          placeholder="Rating"
          value={d.rating ?? ANY}
          onValueChange={(v) => setD((x) => ({ ...x, rating: v === ANY ? undefined : v }))}
          options={choice(
            facets.data?.ratings.map((r) => r.name),
            'Any rating',
          )}
        />
        <Select
          aria-label="Audio language"
          placeholder="Audio language"
          value={d.language ?? ANY}
          onValueChange={(v) => setD((x) => ({ ...x, language: v === ANY ? undefined : v }))}
          options={choice(
            facets.data?.languages.map((l) => l.name),
            'Any language',
          )}
        />
        <Select
          aria-label="Subtitles"
          placeholder="Subtitles"
          value={d.subtitle_language ?? ANY}
          onValueChange={(v) =>
            setD((x) => ({ ...x, subtitle_language: v === ANY ? undefined : v }))
          }
          options={choice(
            facets.data?.subtitle_languages.map((l) => l.name),
            'Any subtitles',
          )}
        />
        <Select
          aria-label="Age-restricted titles"
          placeholder="Age-restricted titles"
          value={d.age ?? 'include'}
          onValueChange={(v) => setD((x) => ({ ...x, age: v as AgeFilter }))}
          options={AGES}
        />
        <Select
          aria-label="Sort by"
          placeholder="Sort by"
          value={d.sort ?? 'updated'}
          onValueChange={(v) => setD((x) => ({ ...x, sort: v as SortKey }))}
          options={SORTS}
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
        <Eyebrow>THE SIDE ROOM LIBRARY</Eyebrow>
        {copy ? (
          <DisplayHeading as="h1" size="xl" accent={copy.accent}>
            {copy.heading}
          </DisplayHeading>
        ) : (
          <DisplayHeading as="h1" size="xl">
            {q ? `Results for “${q}”` : 'Search.'}
          </DisplayHeading>
        )}
        <p className="cin-sub">{copy?.sub ?? 'Everything in the room that matches.'}</p>
      </header>

      {!searching && (
        <div className="cin-chips">
          <button
            type="button"
            className="cin-chip-all"
            aria-pressed={!search.genres}
            onClick={() => setSearch({ genres: undefined })}
          >
            All
          </button>
          <FilterChips
            label="Genres"
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
              {more ? 'Fewer' : 'More'}
            </button>
          )}
          <DropdownChip
            label="Room selection"
            value={search.collection ?? ALL_ROOMS}
            options={[
              { value: ALL_ROOMS, label: 'All rooms' },
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
            <Icon name="filter" size={16} /> Filters{nFilters ? ` (${nFilters})` : ''}
          </button>
        </div>
      )}

      <p className="cin-count" aria-live="polite">
        {done ? `${total} ${total === 1 ? 'title' : 'titles'}` : ' '}
      </p>

      {pending && <CardSkeletons n={6} />}
      {failed && (
        <Notice variant="danger" title="Could not load titles">
          Try again in a moment.
        </Notice>
      )}
      {done && items.length === 0 && (
        <EmptyState
          title="No titles match"
          description={
            searching ? 'Try another search.' : 'Try a different genre, or clear the filters.'
          }
        />
      )}
      {items.length > 0 && (
        <>
          <h2 className="cin-sr-only">Titles</h2>
          <ul className="cin-grid">
            {items.map((t) => (
              <li key={t.id}>
                <TitleCard title={t} />
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
            {list.isFetchingNextPage ? 'Loading…' : 'Load more'}
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
