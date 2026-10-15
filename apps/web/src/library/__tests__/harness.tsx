import { render } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import type {
  Chapter,
  ChapterList,
  PageManifest,
  Series,
  SeriesDetail,
  UpdateItem,
} from '../types';

export const series = (id: string, o: Partial<Series> = {}): Series => ({
  id,
  kind: 'manga',
  title: `Series ${id}`,
  alt_titles: [],
  authors: ['An Author'],
  artists: [],
  status: 'ongoing',
  synopsis: 'A first sentence. A second sentence.',
  genres: ['fantasy', 'adventure'],
  tags: [],
  reading_direction: 'rtl',
  age_restricted: false,
  published: true,
  chapter_count: 3,
  follower_count: 10,
  cover_url: `/cdn/cover-${id}.webp`,
  created_at_ms: 1,
  updated_at_ms: 2,
  ...o,
});

export const A = series('1', { title: 'Frieren' });
export const R = series('2', { title: 'Dark Tale', age_restricted: true });
export const DONE = series('3', { title: 'Monster', status: 'completed' });

export const chapter = (n: number, o: Partial<Chapter> = {}): Chapter => ({
  id: `c${n}`,
  series_id: '1',
  edition_id: 'e1',
  number: n,
  title: null,
  page_count: 3,
  format: 'images',
  published: true,
  published_at_ms: 1_700_000_000_000,
  release_group: 'Group X',
  ...o,
});

export const pages = (n = 3) =>
  Array.from({ length: n }, (_, i) => ({
    index: i,
    width: 1000,
    height: 1500,
    is_double_spread: false,
    url: `/cdn/p${i}.webp`,
    variants: [{ width: 720, url: `/cdn/p${i}_720.webp` }],
    blurhash: null,
  }));

export const manifest = (n: number, o: Partial<PageManifest> = {}): PageManifest => ({
  chapter: chapter(n),
  reading_direction: 'rtl',
  pages: pages(),
  expires_at: 9_999_999_999,
  next_chapter_id: `c${n + 1}`,
  prev_chapter_id: n > 1 ? `c${n - 1}` : null,
  ...o,
});

export const update = (n: number, s = A): UpdateItem => ({
  chapter_id: `c${n}`,
  chapter_number: n,
  published_at_ms: Date.now() - 3600_000,
  language: 'en',
  release_group: 'Group X',
  series_id: s.id,
  series_title: s.title,
  kind: s.kind,
  age_restricted: s.age_restricted,
  cover_url: s.cover_url,
});

export const seriesPage = (items: Series[], total = items.length, offset = 0) => ({
  items,
  total,
  limit: 24,
  offset,
});

/** Handlers for a logged-in member and a small catalog. Override with `server.use` in a test. */
export function useLibraryApi(
  o: {
    browse?: (url: URL) => Record<string, unknown>;
    chapters?: Chapter[];
    detail?: Partial<SeriesDetail>;
  } = {},
) {
  const requests: string[] = [];
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/library/series', ({ request }) => {
      const url = new URL(request.url);
      requests.push(url.search);
      return HttpResponse.json(o.browse ? o.browse(url) : seriesPage([A, R, DONE]));
    }),
    http.get('*/api/v1/library/series/:id', ({ params }) => {
      const s = [A, R, DONE].find((x) => x.id === params.id);
      if (!s) return HttpResponse.json({ message: 'Not found' }, { status: 404 });
      return HttpResponse.json({
        series: s,
        editions: [
          { id: 'e1', series_id: s.id, language: 'en', label: 'Official', is_default: true },
        ],
        follow: null,
        ...o.detail,
      } satisfies SeriesDetail);
    }),
    http.get('*/api/v1/library/series/:id/chapters', () =>
      HttpResponse.json({
        editions: [],
        volumes: [],
        chapters: o.chapters ?? [chapter(1), chapter(2), chapter(3)],
      } satisfies ChapterList),
    ),
    http.get('*/api/v1/library/updates', () =>
      HttpResponse.json({ items: [update(3), update(2)] }),
    ),
    http.get('*/api/v1/library/me/follows', () =>
      HttpResponse.json({ items: [], total: 6, limit: 1, offset: 0 }),
    ),
    // Social routes (phase 11): empty by default; tests override them with server.use.
    http.get('*/api/v1/library/series/:id/ratings', () =>
      HttpResponse.json({
        mine: null,
        global: { average: null, count: 0 },
        friends: { average: null, count: 0 },
        known: { average: null, count: 0 },
        distribution: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
      }),
    ),
    http.get('*/api/v1/library/me/lists', () => HttpResponse.json({ items: [] })),
    http.get('*/api/v1/library/lists/featured', () => HttpResponse.json({ items: [] })),
    http.get('*/api/v1/library/me/unread', () => HttpResponse.json({ items: [] })),
    // Phase 13: empty by default.
    http.get('*/api/v1/library/recommendations', () => HttpResponse.json({ rows: [] })),
    http.get('*/api/v1/library/me/notifications', () => HttpResponse.json({ unread: 0 })),
    http.get('*/api/v1/library/me/downloads', () =>
      HttpResponse.json({ entitled: false, per_day: 0, left: 0 }),
    ),
    http.put('*/api/v1/library/chapters/:id/presence', () => new HttpResponse(null, { status: 204 })),
    http.delete('*/api/v1/library/me/presence', () => new HttpResponse(null, { status: 204 })),
    http.get('*/api/v1/library/chapters/:id/comments', () =>
      HttpResponse.json({ items: [], next_before: null, total: 0 }),
    ),
    http.get('*/api/v1/library/search', () =>
      HttpResponse.json({ series: [A, R], authors: ['An Author'], genres: ['fantasy'], tags: [] }),
    ),
  );
  return requests;
}

export function renderAt(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  const view = render(<RouterProvider router={router} />);
  return { router, ...view };
}
