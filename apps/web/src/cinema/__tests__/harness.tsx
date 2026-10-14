import { render } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';

/** Wire-shaped titles (what the server sends); api.ts adapts them. */
export interface WTitle {
  id: string;
  kind: 'movie' | 'series' | 'anime';
  title: string;
  year: number;
  runtime_min: number | null;
  content_rating: string;
  synopsis: string;
  genres: string[];
  poster_url: string;
  backdrop_url: string;
  age_restricted: boolean;
  has_trailer: boolean;
  season_count: number;
  progress_percent?: number;
}

export const title = (id: string, o: Partial<WTitle> = {}): WTitle => ({
  id,
  kind: 'movie',
  title: `Film ${id}`,
  year: 2024,
  runtime_min: 127,
  content_rating: 'PG-13',
  synopsis: 'The desert holds a destiny.',
  genres: ['science fiction', 'adventure'],
  poster_url: `/cdn/p${id}.webp`,
  backdrop_url: `/cdn/b${id}.webp`,
  age_restricted: false,
  has_trailer: false,
  season_count: 0,
  ...o,
});

export const DUNE = title('1', { title: 'Dune: Part Two' });
export const SHOW = title('2', {
  kind: 'series',
  title: 'Severance',
  year: 2022,
  runtime_min: null,
  season_count: 2,
});
export const ANIME = title('3', { kind: 'anime', title: 'Cowboy Bebop', year: 1998 });
export const RESTRICTED = title('4', { title: 'Alien', age_restricted: true });
export const STARTED = title('5', { title: 'Akira' });

const row = (key: string, kind: string, name: string, items: WTitle[], o: object = {}) => ({
  key,
  kind,
  display: 'row',
  title: name,
  items,
  ...o,
});

export const discover = () => ({
  hero: row('spotlight', 'curated', 'Tonight', [DUNE, STARTED], { display: 'hero' }),
  rows: [
    row('collection:1', 'curated', 'A good place to start.', [DUNE, SHOW, RESTRICTED], {
      eyebrow: 'Selected for the room',
      explore: 'kind=movie&collection=1',
    }),
    row('animation-banner', 'generated', 'Animation. Without limits.', [ANIME], {
      display: 'banner',
    }),
    row('anime-room', 'generated', 'The anime room.', [ANIME], { explore: 'kind=anime' }),
    row('trending', 'provided', 'Trending now.', [SHOW]),
    row('continue-watching', 'provided', 'Continue watching.', [STARTED]),
  ],
});

export const episode = (n: number, o: object = {}) => ({
  id: `e${n}`,
  title_id: '2',
  season_id: 's1',
  season_number: 1,
  number: n,
  title: `Episode ${n} title`,
  synopsis: `Synopsis ${n}`,
  runtime_min: 48,
  still_url: null,
  ...o,
});

export const detail = (t: WTitle, o: Record<string, unknown> = {}) => ({
  title: {
    ...t,
    cast: ['An Actor'],
    directors: ['A Director'],
    writers: [],
    crew: [],
    studios: ['Studio X'],
    languages: ['en'],
    subtitle_languages: ['en', 'fr'],
  },
  seasons: [],
  variants: [],
  trailers: [],
  markers: [],
  ...o,
});

export interface Write {
  method: string;
  url: string;
  body: unknown;
}
export interface Calls {
  browse: string[];
  searches: string[];
  writes: Write[];
}

const GENRE_NAMES = [
  'science fiction',
  'adventure',
  'fantasy',
  'drama',
  'thriller',
  'comedy',
  'horror',
  'romance',
  'war',
];

/** Handlers for a signed-in member and a small catalog. Override with `server.use` in a test. */
export function useCinemaApi(
  o: {
    detail?: unknown;
    browse?: (u: URL) => unknown;
    reviews?: unknown[];
    comments?: unknown[];
  } = {},
) {
  const calls: Calls = { browse: [], searches: [], writes: [] };
  const record = async (method: string, request: Request) => {
    calls.writes.push({
      method,
      url: new URL(request.url).pathname.replace('/api/v1', ''),
      body: await request.json().catch(() => null),
    });
    return new HttpResponse(null, { status: 204 });
  };
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/cinema/discover', () => HttpResponse.json(discover())),
    http.get('*/api/v1/cinema/titles', ({ request }) => {
      const url = new URL(request.url);
      calls.browse.push(url.search);
      return HttpResponse.json(o.browse?.(url) ?? { items: [DUNE, RESTRICTED], total: 2 });
    }),
    http.get('*/api/v1/cinema/facets', () =>
      HttpResponse.json({
        genres: GENRE_NAMES.map((name) => ({ name, count: 3 })),
        ratings: [{ name: 'PG-13', count: 2 }],
        languages: [{ name: 'en', count: 2 }],
        subtitle_languages: [{ name: 'fr', count: 1 }],
      }),
    ),
    http.get('*/api/v1/cinema/collections', () =>
      HttpResponse.json({ items: [{ id: 'c1', name: 'Staff picks' }] }),
    ),
    http.get('*/api/v1/cinema/search', ({ request }) => {
      calls.searches.push(new URL(request.url).searchParams.get('q') ?? '');
      return HttpResponse.json({ titles: [DUNE, RESTRICTED], people: [], genres: [], tags: [] });
    }),
    http.get('*/api/v1/cinema/titles/:id/episodes', ({ request }) => {
      const season = new URL(request.url).searchParams.get('season');
      return HttpResponse.json({
        seasons: [],
        episodes:
          season === '2'
            ? [episode(1, { id: 's2e1', season_number: 2, title: 'Season two opener' })]
            : [episode(1), episode(2)],
      });
    }),
    http.get('*/api/v1/cinema/titles/:id/watch-state', () =>
      HttpResponse.json({
        items: [
          { episode_id: 'e1', percent: 100, completed: true },
          { episode_id: 'e2', percent: 45, completed: false },
        ],
      }),
    ),
    http.get('*/api/v1/cinema/titles/:id/ratings', () =>
      HttpResponse.json({ mine: null, global: { average: 8, count: 3 }, friends: {}, known: {} }),
    ),
    http.get('*/api/v1/cinema/titles/:id/reviews', () =>
      HttpResponse.json({ items: o.reviews ?? [], next_before: null }),
    ),
    http.get('*/api/v1/cinema/titles/:id/comments', () =>
      HttpResponse.json({ items: o.comments ?? [], next_before: null }),
    ),
    http.get('*/api/v1/cinema/titles/:id', ({ params }) => {
      if (params.id === 'missing') return HttpResponse.json({ message: 'nf' }, { status: 404 });
      const t = [DUNE, SHOW, ANIME, RESTRICTED, STARTED].find((x) => x.id === params.id) ?? DUNE;
      return HttpResponse.json(o.detail ?? detail(t));
    }),
    http.get('*/api/v1/cinema/me/list', () => HttpResponse.json({ items: [], next_before: null })),
    http.get('*/api/v1/cinema/me/continue-watching', () =>
      HttpResponse.json({
        items: [
          {
            reason: 'resume',
            title: { id: '5' },
            progress: { percent: 40, position_secs: 2530 },
          },
        ],
      }),
    ),
    http.put('*/api/v1/cinema/*', ({ request }) => record('PUT', request)),
    http.post('*/api/v1/cinema/*', ({ request }) => record('POST', request)),
    http.delete('*/api/v1/cinema/*', ({ request }) => record('DELETE', request)),
  );
  return calls;
}

export function renderAt(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  const view = render(<RouterProvider router={router} />);
  return { router, ...view };
}

/** Make `matchMedia` answer true for max-width queries (a phone) until restored. */
export function phone() {
  const original = window.matchMedia;
  window.matchMedia = ((q: string) => ({
    matches: q.includes('max-width'),
    media: q,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  })) as unknown as typeof window.matchMedia;
  return () => {
    window.matchMedia = original;
  };
}
