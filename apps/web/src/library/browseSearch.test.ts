import { describe, expect, it } from 'vitest';
import { validateBrowseSearch } from './browseSearch';
import { createLibraryRoutes } from './routes';
import { createRootRoute, createRouter, createMemoryHistory } from '@tanstack/react-router';

describe('library browse search (SEC-AUD-144)', () => {
  it('keeps good values and drops bad ones', () => {
    expect(
      validateBrowseSearch({
        kind: 'manga',
        status: 'nope',
        rating_min: '7',
        sort: 'rating',
        order: 'asc',
        junk: 1,
      }),
    ).toEqual({ kind: 'manga', rating_min: 7, sort: 'rating', order: 'asc' });
    expect(validateBrowseSearch({ rating_min: 11, age: 'only' })).toEqual({ age: 'only' });
  });

  it('is validated by the route itself, so Browse need not be loaded', () => {
    const root = createRootRoute();
    const router = createRouter({
      routeTree: root.addChildren([createLibraryRoutes(root)]),
      history: createMemoryHistory({ initialEntries: ['/library/browse'] }),
    });
    const browse = (router.routesById as Record<string, { options: { validateSearch?: unknown } }>)[
      '/library/browse'
    ];
    expect(browse?.options.validateSearch).toBe(validateBrowseSearch);
  });
});
