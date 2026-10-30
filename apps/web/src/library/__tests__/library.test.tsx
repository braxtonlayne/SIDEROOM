import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../test/server';
import { localProgress, setProgressApi } from '../progress';
import { A, chapter, DONE, renderAt, series, seriesPage, useLibraryApi } from './harness';
import { nav } from '../../lib/enterApp';

beforeEach(() => {
  localStorage.clear();
  setProgressApi(localProgress);
});

describe('Library home', () => {
  it('renders the mockup sections and passes axe', async () => {
    useLibraryApi();
    const { container } = renderAt('/library');
    expect(
      await screen.findByRole('heading', { name: 'Find your next good story.' }),
    ).toBeInTheDocument();
    // chrome
    expect(screen.getByRole('link', { name: 'The library' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(await screen.findByLabelText('6 followed')).toBeInTheDocument();
    // spotlight, trending, genres, fresh, finished, footer
    expect((await screen.findAllByRole('heading', { name: 'Frieren' })).length).toBeGreaterThan(0);
    expect(screen.getByRole('heading', { name: 'Trending this week' })).toBeInTheDocument();
    expect(await screen.findByText('#1 TRENDING')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Follow your curiosity' })).toBeInTheDocument();
    expect(await screen.findByRole('heading', { name: 'Fresh off the press' })).toBeInTheDocument();
    expect(await screen.findAllByText('Chapter 3 · English')).not.toHaveLength(0);
    expect(
      screen.getByRole('heading', { name: 'Finished stories, new beginnings' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('navigation', { name: 'Library footer' })).toBeInTheDocument();
    expect(
      within(screen.getByRole('navigation', { name: 'Library footer' })).getByRole('link', {
        name: 'Help',
      }),
    ).toHaveAttribute('href', '/help'); // D-270
    expect(screen.getByRole('radio', { name: 'Friends' })).toBeEnabled();
    // restricted titles are shown with an 18+ badge (D-184)
    expect((await screen.findAllByText('Dark Tale')).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/Age-restricted, 18 or older/).length).toBeGreaterThan(0);
    expect(await axeViolations(container)).toEqual([]);
  });

  it('shows "Pick up where you left off" from saved progress and links to the page', async () => {
    useLibraryApi();
    await localProgress.save({
      chapterId: 'c36',
      seriesId: '1',
      seriesTitle: 'Frieren',
      chapterNumber: 36,
      pageCount: 6,
      pageIndex: 1,
      percent: 33,
      completed: false,
    });
    renderAt('/library');
    expect(await screen.findByText('Chapter 36 · Page 2 of 6')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Continue Frieren' })).toHaveAttribute(
      'href',
      '/library/r/c36/2',
    );
  });

  it('sends a logged-out visitor to the login page', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/users/@me', () => HttpResponse.json({ message: 'no' }, { status: 401 })),
    );
    vi.spyOn(nav, 'assign').mockImplementation(() => {});
    renderAt('/library');
    await waitFor(() => expect(nav.assign).toHaveBeenCalledWith('/login?next=%2Flibrary'));
  });
});

describe('search', () => {
  it('shows grouped results and opens the active one with the keyboard', async () => {
    useLibraryApi();
    const user = userEvent.setup();
    const { router } = renderAt('/library');
    const box = await screen.findByRole('combobox', { name: 'Search the library' });
    await user.type(box, 'fri');
    const list = await screen.findByRole('listbox', { name: 'Search results' });
    expect(await within(list).findByRole('option', { name: /Frieren/ })).toBeInTheDocument();
    expect(within(list).getByRole('group', { name: 'Authors' })).toBeInTheDocument();
    expect(within(list).getByRole('group', { name: 'Genres' })).toBeInTheDocument();
    await user.keyboard('{ArrowDown}');
    expect(box).toHaveAttribute('aria-activedescendant');
    expect(within(list).getAllByRole('option')[0]).toHaveAttribute('aria-selected', 'true');
    await user.keyboard('{ArrowDown}{ArrowUp}{Enter}');
    await waitFor(() => expect(router.state.location.pathname).toBe('/library/s/1'));
  });

  it('Enter without a selection searches, Escape closes', async () => {
    useLibraryApi();
    const user = userEvent.setup();
    const { router } = renderAt('/library');
    const box = await screen.findByRole('combobox', { name: 'Search the library' });
    await user.type(box, 'fri');
    await screen.findByRole('option', { name: /Frieren/ });
    await user.keyboard('{Escape}');
    expect(box).toHaveAttribute('aria-expanded', 'false');
    await user.keyboard('{Enter}');
    await waitFor(() => expect(router.state.location.pathname).toBe('/library/browse'));
    expect(router.state.location.search).toMatchObject({ q: 'fri' });
  });

  it('"/" focuses the search box', async () => {
    useLibraryApi();
    const user = userEvent.setup();
    renderAt('/library');
    const box = await screen.findByRole('combobox', { name: 'Search the library' });
    await user.keyboard('/');
    expect(box).toHaveFocus();
  });
});

describe('browse', () => {
  it('sends URL filters to the API, pages with Load more, and passes axe', async () => {
    const reqs = useLibraryApi({
      browse: (url) =>
        url.searchParams.get('offset') === '24'
          ? seriesPage([series('9', { title: 'Page Two' })], 25, 24)
          : seriesPage(
              Array.from({ length: 24 }, (_, i) => series(`b${i}`, { title: `Title ${i}` })),
              25,
            ),
    });
    const user = userEvent.setup();
    const { container } = renderAt(
      '/library/browse?kind=manhwa&genres=action&sort=title&order=asc',
    );
    expect(await screen.findByText('Title 0')).toBeInTheDocument();
    const first = new URLSearchParams(reqs[0]);
    expect(first.get('kind')).toBe('manhwa');
    expect(first.get('genres')).toBe('action');
    expect(first.get('sort')).toBe('title');
    expect(first.get('order')).toBe('asc');
    expect(first.get('limit')).toBe('24');
    expect(await axeViolations(container)).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Load more' }));
    expect(await screen.findByText('Page Two')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Load more' })).toBeNull();
  });

  it('applies the filter drawer to the URL', async () => {
    useLibraryApi();
    const user = userEvent.setup();
    const { router } = renderAt('/library/browse');
    await screen.findByText('Frieren');
    await user.click(screen.getByRole('button', { name: /Filters/ }));
    // Genres cycle: once includes, twice leaves out. Tags are typed.
    await user.click(await screen.findByRole('button', { name: /^Action: not filtered/ }));
    await user.click(screen.getByRole('button', { name: /^Fantasy: not filtered/ }));
    await user.click(screen.getByRole('button', { name: /^Fantasy: included/ }));
    await user.type(screen.getByLabelText('Add a tag'), 'magic{Enter}');
    await user.selectOptions(screen.getByLabelText('Rating'), '7');
    await user.selectOptions(screen.getByLabelText('Status'), 'completed');
    await user.selectOptions(screen.getByLabelText('Age-restricted titles'), 'only');
    await user.click(screen.getByRole('button', { name: 'Show results' }));
    await waitFor(() =>
      expect(router.state.location.search).toMatchObject({
        genres: 'action',
        exclude: 'fantasy',
        tags: 'magic',
        rating_min: 7,
        status: 'completed',
        age: 'only',
      }),
    );
  });

  it('defaults the age filter to "hide" when library.hide_age_restricted is set', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ version: 1, settings: { library: { hide_age_restricted: true } } }),
      ),
    );
    const user = userEvent.setup();
    renderAt('/library/browse');
    await screen.findByText('Frieren');
    await user.click(screen.getByRole('button', { name: /Filters/ }));
    await waitFor(() => expect(screen.getByLabelText('Age-restricted titles')).toHaveValue('hide'));
  });

  it('renders the list layout', async () => {
    useLibraryApi();
    renderAt('/library/browse?view=list');
    expect(await screen.findByRole('link', { name: 'Frieren' })).toBeInTheDocument();
  });

  it('shows search results for ?q=', async () => {
    useLibraryApi();
    renderAt('/library/browse?q=fri');
    expect(await screen.findByRole('heading', { name: /Results for/ })).toBeInTheDocument();
    expect(await screen.findByText('Frieren')).toBeInTheDocument();
  });
});

describe('series page', () => {
  it('shows metadata, an 18+ badge, chapters with read marks and the continue target', async () => {
    useLibraryApi({
      chapters: [
        chapter(1, { progress: { page_index: 2, percent: 100, completed: true } }),
        chapter(2, { progress: { page_index: 1, percent: 40, completed: false } }),
        chapter(3),
      ],
    });
    const { container } = renderAt('/library/s/2');
    expect(await screen.findByRole('heading', { name: /Dark Tale/ })).toBeInTheDocument();
    expect(screen.getAllByText(/Age-restricted, 18 or older/).length).toBeGreaterThan(0);
    expect(await screen.findByText('Chapter 3')).toBeInTheDocument();
    expect(screen.getByText('Read')).toBeInTheDocument();
    expect(screen.getByText('Page 2 of 3')).toBeInTheDocument();
    expect(screen.getByText('Unread')).toBeInTheDocument();
    const cont = await screen.findByRole('link', { name: /Continue Ch. 2/ });
    expect(cont).toHaveAttribute('href', '/library/r/c2/2');
    expect(await axeViolations(container)).toEqual([]);
  });

  it('follows and unfollows', async () => {
    useLibraryApi();
    const calls: string[] = [];
    server.use(
      http.put('*/api/v1/library/series/1/follow', () => {
        calls.push('PUT');
        return HttpResponse.json({ shelf: 'reading', notify: true, created_at_ms: 1 });
      }),
      http.delete('*/api/v1/library/series/1/follow', () => {
        calls.push('DELETE');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    renderAt('/library/s/1');
    const btn = await screen.findByRole('button', { name: 'Follow' });
    expect(btn).toHaveAttribute('aria-pressed', 'false');
    // after the PUT the detail is refetched and now reports a follow
    server.use(
      http.get('*/api/v1/library/series/1', () =>
        HttpResponse.json({
          series: A,
          editions: [
            { id: 'e1', series_id: '1', language: 'en', label: 'Official', is_default: true },
          ],
          follow: { shelf: 'reading', notify: true, created_at_ms: 1 },
        }),
      ),
    );
    await user.click(btn);
    await waitFor(() => expect(calls).toEqual(['PUT']));
    const following = await screen.findByRole('button', { name: 'Following' });
    await user.click(following);
    await waitFor(() => expect(calls).toEqual(['PUT', 'DELETE']));
  });

  it('answers a missing series with a message', async () => {
    useLibraryApi();
    renderAt('/library/s/404');
    expect(await screen.findByText('This series is not here')).toBeInTheDocument();
  });
});

describe('chrome and other pages', () => {
  it('renders the sidebar sections and a placeholder page', async () => {
    useLibraryApi();
    renderAt('/library/requests');
    expect(await screen.findByRole('heading', { name: 'Requests' })).toBeInTheDocument();
    const nav = screen.getByRole('navigation', { name: 'Library' });
    for (const name of [
      'Home',
      'Browse',
      'Latest updates',
      'Requests',
      'My library',
      'Reading history',
      'Reading lists',
      'Submissions',
      'Upload',
      'Help & shortcuts',
      'Settings',
    ])
      expect(within(nav).getByText(name)).toBeInTheDocument();
    expect(screen.getByText('Manga, at your pace.')).toBeInTheDocument();
    expect(screen.queryByText(/the board/i)).toBeNull();
  });

  it('latest updates toggles to followed series', async () => {
    useLibraryApi();
    const seen: string[] = [];
    server.use(
      http.get('*/api/v1/library/updates', ({ request }) => {
        seen.push(new URL(request.url).search);
        return HttpResponse.json({ items: [] });
      }),
    );
    const user = userEvent.setup();
    renderAt('/library/updates');
    expect(await screen.findByText('No chapters yet')).toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: 'Following' }));
    await waitFor(() => expect(seen.some((s) => s.includes('following=true'))).toBe(true));
  });

  it('My library lists followed series', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/me/follows', () =>
        HttpResponse.json({
          items: [{ series: DONE, follow: { shelf: 'completed', notify: true, created_at_ms: 1 } }],
          total: 1,
          limit: 60,
          offset: 0,
        }),
      ),
    );
    renderAt('/library/me');
    expect(await screen.findByText('Monster')).toBeInTheDocument();
  });
});
