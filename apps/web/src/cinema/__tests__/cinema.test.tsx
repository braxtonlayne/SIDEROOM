import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { nav } from '../../lib/enterApp';
import { http, HttpResponse, server } from '../../test/server';
import { exploreHref, toTitle } from '../api';
import { splitTitle } from '../Discover';
import { formatClock, formatRuntime } from '../ui';
import {
  ANIME,
  detail,
  DUNE,
  phone,
  renderAt,
  RESTRICTED,
  SHOW,
  STARTED,
  title,
  useCinemaApi,
} from './harness';

describe('pure helpers', () => {
  it('splits a display title and formats runtimes and clocks', () => {
    expect(splitTitle('Dune: Part Two')).toEqual(['Dune:', 'Part Two.']);
    expect(splitTitle('Alien')).toEqual(['Alien', null]);
    expect(formatRuntime(167)).toBe('2h 47m');
    expect(formatRuntime(45)).toBe('45m');
    expect(formatClock(2530)).toBe('42:10');
    expect(formatClock(3723)).toBe('1:02:03');
  });

  it('adapts wire titles and explore queries', () => {
    expect(toTitle({ ...DUNE, progress_percent: 40 }).my).toMatchObject({ progress_percent: 40 });
    expect(toTitle(DUNE).my).toBeNull();
    expect(exploreHref('kind=anime&genres=drama&limit=20')).toBe('/cinema/anime?genres=drama');
    expect(exploreHref('collection=3')).toBe('/cinema/movies?collection=3');
    expect(exploreHref(null)).toBeNull();
  });
});

describe('cinema chrome', () => {
  it('shows the section bar with the cinema active, the cinema nav and the footer, no board', async () => {
    useCinemaApi();
    const { container } = renderAt('/cinema/movies');
    expect(await screen.findByRole('link', { name: 'The cinema' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(screen.queryByText(/the board/i)).toBeNull();
    const nav = screen.getByRole('navigation', { name: 'Cinema' });
    expect(within(nav).getByRole('link', { name: 'Movies' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    for (const n of ['Discover', 'Series', 'Anime', 'Live TV', 'My list'])
      expect(within(nav).getByRole('link', { name: n })).toBeInTheDocument();
    expect(screen.getByText('A little further from the ordinary.')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'About this library' })).toBeInTheDocument();
    await screen.findByText('2 titles');
    expect(await axeViolations(container)).toEqual([]);
  });

  it('the avatar menu lists the cinema destinations', async () => {
    useCinemaApi();
    const user = userEvent.setup();
    renderAt('/cinema/movies');
    await user.click(await screen.findByRole('button', { name: 'Account menu' }));
    const menu = await screen.findByRole('menu', { name: 'Account' });
    for (const n of ['Profile', 'My uploads', 'Upload', 'Requests', 'Settings', 'Back to Lounge'])
      expect(within(menu).getByRole('menuitem', { name: n })).toBeInTheDocument();
  });

  it('on a phone the nav moves into a hamburger drawer', async () => {
    useCinemaApi();
    const restore = phone();
    try {
      const user = userEvent.setup();
      renderAt('/cinema/movies');
      expect(screen.queryByRole('navigation', { name: 'Cinema' })).toBeNull();
      await user.click(await screen.findByRole('button', { name: 'Open cinema menu' }));
      const drawer = await screen.findByRole('dialog', { name: 'The cinema' });
      expect(within(drawer).getByRole('link', { name: 'Series' })).toBeInTheDocument();
    } finally {
      restore();
    }
  });
});

describe('session', () => {
  it('sends a signed-out visitor to the login page and back', async () => {
    vi.spyOn(nav, 'assign').mockImplementation(() => {});
    renderAt('/cinema/movies');
    await waitFor(() => expect(nav.assign).toHaveBeenCalledWith('/login?next=%2Fcinema%2Fmovies'));
    vi.restoreAllMocks();
  });
});

describe('search overlay', () => {
  it('shows instant results with posters, supports arrow keys and opens a title', async () => {
    const calls = useCinemaApi();
    const user = userEvent.setup();
    const { router } = renderAt('/cinema');
    await user.click(await screen.findByRole('button', { name: 'Search the cinema' }));
    const box = await screen.findByRole('combobox', { name: 'Search the cinema' });
    await user.type(box, 'du');
    const list = await screen.findByRole('listbox', { name: 'Search results' });
    const opts = await within(list).findAllByRole('option');
    expect(opts).toHaveLength(2);
    expect(calls.searches).toContain('du');
    await user.keyboard('{ArrowDown}');
    expect(opts[0]).toHaveAttribute('aria-selected', 'true');
    await user.keyboard('{ArrowDown}{ArrowUp}{Enter}');
    await waitFor(() => expect(router.state.location.pathname).toBe('/cinema/t/1'));
  });

  it('Enter without a selection opens the results page; "/" opens the overlay', async () => {
    useCinemaApi();
    const user = userEvent.setup();
    const { router } = renderAt('/cinema');
    await screen.findByRole('button', { name: 'Search the cinema' });
    await user.keyboard('/');
    const box = await screen.findByRole('combobox', { name: 'Search the cinema' });
    await user.type(box, 'bebop{Enter}');
    await waitFor(() => expect(router.state.location.pathname).toBe('/cinema/search'));
    expect(router.state.location.search).toMatchObject({ q: 'bebop' });
    expect(await screen.findByRole('heading', { name: /Results for/ })).toBeInTheDocument();
    expect(await screen.findByText('2 titles')).toBeInTheDocument();
  });
});

describe('Discover', () => {
  it('renders the hero, rows, the animation banner and passes axe', async () => {
    useCinemaApi();
    const { container } = renderAt('/cinema');
    expect(await screen.findByRole('heading', { level: 1, name: /Dune:/ })).toBeInTheDocument();
    expect(screen.getByText('TONIGHT’S FEATURE')).toBeInTheDocument();
    expect(screen.getByText(/01 \/ THE EVENING SELECTION/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /More info/ })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: /A good place to start/ })).toBeInTheDocument();
    expect(screen.getByText('Selected for the room')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: /Animation\./ })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /Step into the anime room/ })).toHaveAttribute(
      'href',
      '/cinema/anime',
    );
    expect(screen.getAllByText('Anime').length).toBeGreaterThan(0); // ANIME corner tag
    expect(screen.getAllByText('Age-restricted, 18 or older').length).toBeGreaterThan(0);
    // "Explore all" follows the row's explore query
    const links = screen.getAllByRole('link', { name: /Explore all/ });
    expect(links.map((l) => l.getAttribute('href'))).toContain('/cinema/movies?collection=1');
    expect(links.map((l) => l.getAttribute('href'))).toContain('/cinema/anime');
    expect(await axeViolations(container)).toEqual([]);
  });

  it('the hero switches feature, shows Resume with the saved position, and trending has a scope switch', async () => {
    useCinemaApi();
    const scopes: (string | null)[] = [];
    server.use(
      http.get('*/api/v1/cinema/discover', ({ request }) => {
        scopes.push(new URL(request.url).searchParams.get('scope'));
        return HttpResponse.json({
          hero: { key: 's', kind: 'curated', display: 'hero', title: 'x', items: [DUNE, STARTED] },
          rows: [
            {
              key: 'trending',
              kind: 'provided',
              display: 'row',
              title: 'Trending now.',
              items: [SHOW],
            },
          ],
        });
      }),
    );
    const user = userEvent.setup();
    renderAt('/cinema');
    await screen.findByRole('heading', { level: 1, name: /Dune:/ });
    await user.click(screen.getByRole('button', { name: 'Show Akira' }));
    expect(await screen.findByRole('button', { name: /Resume 42:10/ })).toBeInTheDocument();
    expect(screen.getByText(/02 \/ THE EVENING SELECTION/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Friends' }));
    await waitFor(() => expect(scopes).toContain('friends'));
  });

  it('shows a Watch trailer button when the title has one', async () => {
    useCinemaApi();
    server.use(
      http.get('*/api/v1/cinema/discover', () =>
        HttpResponse.json({
          hero: {
            key: 's',
            kind: 'curated',
            display: 'hero',
            title: 'x',
            items: [{ ...DUNE, has_trailer: true }],
          },
          rows: [],
        }),
      ),
    );
    renderAt('/cinema');
    expect(await screen.findByRole('button', { name: 'Watch trailer' })).toBeInTheDocument();
  });

  it('quick actions on a poster add to My list and mark watched', async () => {
    const calls = useCinemaApi();
    const user = userEvent.setup();
    renderAt('/cinema');
    await screen.findByRole('heading', { name: /A good place to start/ });
    await user.click(
      (await screen.findAllByRole('button', { name: 'Add Severance to My list' }))[0]!,
    );
    await waitFor(() =>
      expect(calls.writes).toContainEqual({ method: 'PUT', url: '/cinema/me/list/2', body: null }),
    );
    await user.click(
      (await screen.findAllByRole('button', { name: 'Mark Severance watched' }))[0]!,
    );
    await waitFor(() =>
      expect(calls.writes).toContainEqual({
        method: 'POST',
        url: '/cinema/titles/2/watched',
        body: { watched: true },
      }),
    );
  });

  it('the continue-watching card shows progress from the caller’s resume points', async () => {
    useCinemaApi();
    renderAt('/cinema');
    const bar = await screen.findAllByRole('progressbar', { name: 'Akira progress' });
    expect(bar[0]).toHaveAttribute('aria-valuenow', '40');
  });

  it('opening an age-restricted title asks for confirmation first (D-184)', async () => {
    useCinemaApi();
    const posts: unknown[] = [];
    server.use(
      http.post('*/api/v1/sr/age-gate', async ({ request }) => {
        posts.push(await request.json());
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    const { router } = renderAt('/cinema');
    const link = (await screen.findAllByRole('link', { name: 'Alien' }))[0]!;
    await user.click(link);
    expect(router.state.location.pathname).toBe('/cinema');
    const dialog = await screen.findByRole('dialog', { name: 'Are you 18 or older?' });
    await user.click(within(dialog).getByRole('button', { name: 'No' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(router.state.location.pathname).toBe('/cinema');
    await user.click(link);
    await user.click(await screen.findByRole('button', { name: 'Yes' }));
    await waitFor(() => expect(router.state.location.pathname).toBe(`/cinema/t/${RESTRICTED.id}`));
    expect(posts).toEqual([{ over_18: false }, { over_18: true }]);
  });
});

describe('Movies / Series / Anime', () => {
  it('movies: header copy, count, genre chips and the room selection filter the request', async () => {
    const calls = useCinemaApi();
    const user = userEvent.setup();
    const { router } = renderAt('/cinema/movies');
    expect(
      await screen.findByRole('heading', { level: 1, name: 'The cinema.' }),
    ).toBeInTheDocument();
    expect(screen.getByText('THE SIDE ROOM LIBRARY')).toBeInTheDocument();
    expect(screen.getByText('Films for a night well spent.')).toBeInTheDocument();
    expect(await screen.findByText('2 titles')).toBeInTheDocument();
    expect(calls.browse[0]).toContain('kind=movie');
    await user.click(screen.getByRole('button', { name: 'Fantasy' }));
    await waitFor(() => expect(router.state.location.search).toMatchObject({ genres: 'fantasy' }));
    await waitFor(() => expect(calls.browse.at(-1)).toContain('genres=fantasy'));
    await user.click(screen.getByRole('button', { name: 'All' }));
    await waitFor(() => expect(router.state.location.search).toEqual({}));
    expect(screen.queryByRole('button', { name: 'Romance' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'More' }));
    expect(screen.getByRole('button', { name: 'Romance' })).toBeInTheDocument();
    // collections dropdown (curated sort when a room is chosen)
    await user.click(screen.getByRole('combobox', { name: 'Room selection' }));
    await user.click(await screen.findByRole('option', { name: 'Staff picks' }));
    await waitFor(() => expect(calls.browse.at(-1)).toContain('collection=c1'));
    expect(calls.browse.at(-1)).toContain('sort=curated');
  });

  it('the filters drawer applies year, runtime, rating, language, age and sort, and resets', async () => {
    const calls = useCinemaApi();
    const user = userEvent.setup();
    const { router } = renderAt('/cinema/series');
    expect(await screen.findByRole('heading', { level: 1, name: /One more/ })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Filters/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Filters' });
    await user.type(within(dialog).getByLabelText('Year from'), '1990');
    await user.type(within(dialog).getByLabelText('Longest runtime'), '60');
    await user.click(within(dialog).getByRole('combobox', { name: 'Sort by' }));
    await user.click(await screen.findByRole('option', { name: 'Newest added' }));
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(calls.browse.at(-1)).toContain('year_from=1990'));
    expect(calls.browse.at(-1)).toContain('runtime_max=60');
    expect(calls.browse.at(-1)).toContain('sort=newest');
    expect(calls.browse.at(-1)).toContain('kind=series');
    expect(router.state.location.search).toMatchObject({ year_from: 1990, sort: 'newest' });
    expect(screen.getByRole('button', { name: 'Filters (3)' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Filters/ }));
    await user.click(
      within(await screen.findByRole('dialog', { name: 'Filters' })).getByRole('button', {
        name: 'Reset',
      }),
    );
    await waitFor(() => expect(router.state.location.search).toEqual({}));
  });

  it('the filters drawer offers the catalog’s ratings and languages', async () => {
    const calls = useCinemaApi();
    const user = userEvent.setup();
    renderAt('/cinema/movies');
    await user.click(await screen.findByRole('button', { name: /Filters/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Filters' });
    await user.click(within(dialog).getByRole('combobox', { name: 'Rating' }));
    await user.click(await screen.findByRole('option', { name: 'PG-13' }));
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(calls.browse.at(-1)).toContain('rating=PG-13'));
  });

  it('anime has its own accent styling hook and heading', async () => {
    useCinemaApi();
    const { container } = renderAt('/cinema/anime');
    expect(
      await screen.findByRole('heading', { level: 1, name: /The anime room/ }),
    ).toBeInTheDocument();
    expect(container.querySelector('.cin-page[data-anime]')).not.toBeNull();
    expect(screen.getByRole('link', { name: 'Anime' })).toHaveAttribute('aria-current', 'page');
  });

  it('pages with the cursor via Load more and shows an empty state', async () => {
    const calls = useCinemaApi({
      browse: (u) =>
        u.searchParams.get('cursor') === 'c2'
          ? { items: [title('9', { title: 'Second page' })], total: 2 }
          : { items: [DUNE], total: 2, next_cursor: 'c2' },
    });
    const user = userEvent.setup();
    renderAt('/cinema/movies');
    await user.click(await screen.findByRole('button', { name: 'Load more' }));
    expect(await screen.findByText('Second page')).toBeInTheDocument();
    expect(calls.browse.at(-1)).toContain('cursor=c2');
    expect(screen.queryByRole('button', { name: 'Load more' })).toBeNull();

    server.use(
      http.get('*/api/v1/cinema/titles', () => HttpResponse.json({ items: [], total: 0 })),
    );
    renderAt('/cinema/series');
    expect(await screen.findByText('No titles match')).toBeInTheDocument();
  });
});

describe('Title page', () => {
  it('lists credited contributors with what they added (CIN-SOC-003)', async () => {
    useCinemaApi({ detail: detail(DUNE) });
    server.use(
      http.get('*/api/v1/cinema/titles/1/contributors', () =>
        HttpResponse.json({
          contributors: [
            { user_id: 42, name: 'Mina', titles: 1, episodes: 0 },
            { user_id: 43, name: 'Ravi', titles: 0, episodes: 3 },
          ],
        }),
      ),
    );
    const user = userEvent.setup();
    renderAt('/cinema/t/1');
    const tab = await screen.findByRole('tab', { name: 'Contributors' });
    await waitFor(() => expect(tab).toBeEnabled());
    await user.click(tab);
    expect(await screen.findByText('Mina')).toBeInTheDocument();
    expect(screen.getByText(/Added the title/)).toBeInTheDocument();
    expect(screen.getByText(/3 episodes/)).toBeInTheDocument();
  });

  it('a film: header actions, details tab, social tabs, rating, variants and My list', async () => {
    const calls = useCinemaApi({
      detail: detail(
        { ...DUNE, has_trailer: true },
        { variants: [{ id: 'v2', title_id: '1', label: 'Director’s Cut' }] },
      ),
    });
    const user = userEvent.setup();
    const { container } = renderAt('/cinema/t/1');
    expect(await screen.findByRole('heading', { level: 1, name: /Dune/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Play/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Watch together/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Share to Lounge/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Download/ })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Trailer' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Episodes' })).toBeNull();
    expect(screen.getByRole('tab', { name: 'Reviews' })).toBeEnabled();
    expect(screen.getByRole('tab', { name: 'Comments' })).toBeEnabled();
    expect(screen.getByRole('tab', { name: 'Contributors' })).toBeDisabled();
    await user.click(screen.getByRole('tab', { name: 'Details' }));
    expect(await screen.findByText('An Actor')).toBeInTheDocument();
    expect(screen.getByText('A Director (Director)')).toBeInTheDocument();
    // variant picker
    await user.click(screen.getByRole('combobox', { name: 'Version' }));
    await user.click(await screen.findByRole('option', { name: 'Director’s Cut' }));
    // My list + rating
    await user.click(screen.getByRole('button', { name: /My list/ }));
    await waitFor(() =>
      expect(calls.writes).toContainEqual({ method: 'PUT', url: '/cinema/me/list/1', body: null }),
    );
    await user.click(screen.getByRole('button', { name: /Rate/ }));
    await user.click(await screen.findByRole('menuitemcheckbox', { name: '8 out of 10' }));
    await waitFor(() =>
      expect(calls.writes).toContainEqual({
        method: 'PUT',
        url: '/cinema/titles/1/rating',
        body: { score: 8 },
      }),
    );
    expect(await axeViolations(container)).toEqual([]);
  });

  it('Download opens the download modal; a non-Patron is told downloads are for Patrons', async () => {
    useCinemaApi({ detail: detail(DUNE) });
    server.use(
      http.get('*/api/v1/cinema/titles/1/downloads', () =>
        HttpResponse.json({ code: 40300, message: 'Patrons only' }, { status: 403 }),
      ),
    );
    const user = userEvent.setup();
    renderAt('/cinema/t/1');
    await user.click(await screen.findByRole('button', { name: /Download/ }));
    expect(await screen.findByText(/Downloads are for Patrons/)).toBeInTheDocument();
  });

  it('Play carries the chosen variant to the player route', async () => {
    useCinemaApi({
      detail: detail(DUNE, { variants: [{ id: 'v2', title_id: '1', label: 'Director’s Cut' }] }),
    });
    const user = userEvent.setup();
    const { router } = renderAt('/cinema/t/1');
    await user.click(await screen.findByRole('combobox', { name: 'Version' }));
    await user.click(await screen.findByRole('option', { name: 'Director’s Cut' }));
    await user.click(screen.getByRole('button', { name: /Play/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/cinema/watch/1'));
    expect(router.state.location.searchStr).toContain('variant=v2');
  });

  it('a series: season selector, episode progress and watched toggles, mark season watched', async () => {
    const calls = useCinemaApi({
      detail: detail(SHOW, {
        seasons: [
          { id: 's1', title_id: '2', number: 1, episode_count: 2 },
          { id: 's2', title_id: '2', number: 2, episode_count: 1 },
        ],
      }),
      browse: () => ({ items: [SHOW, ANIME], total: 2 }),
    });
    const user = userEvent.setup();
    renderAt('/cinema/t/2');
    expect(await screen.findByText('Episode 2 title')).toBeInTheDocument();
    expect(screen.getByRole('progressbar', { name: 'Episode 2 progress' })).toHaveAttribute(
      'aria-valuenow',
      '45',
    );
    expect(screen.getByRole('button', { name: 'Mark episode 1 unwatched' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await user.click(screen.getByRole('button', { name: 'Mark episode 2 watched' }));
    await waitFor(() =>
      expect(calls.writes).toContainEqual({
        method: 'POST',
        url: '/cinema/titles/2/watched',
        body: { watched: true, episode_id: 'e2' },
      }),
    );
    await user.click(screen.getByRole('button', { name: 'Mark season watched' }));
    await waitFor(() =>
      expect(calls.writes).toContainEqual({
        method: 'POST',
        url: '/cinema/titles/2/watched',
        body: { watched: true, season: 1 },
      }),
    );
    await user.click(screen.getByRole('combobox', { name: 'Season' }));
    await user.click(await screen.findByRole('option', { name: 'Season 2' }));
    expect(await screen.findByText('Season two opener')).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'More like this' }));
    expect(await screen.findByRole('link', { name: 'Cowboy Bebop' })).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Severance' })).toBeNull(); // not itself
    expect(calls.browse.at(-1)).toContain('kind=series');
  });

  it('reviews: lists them, hides spoilers until asked, posts, and marks helpful', async () => {
    const review = {
      id: 'r1',
      author_id: '9',
      rating: 9,
      body: 'The ending was something.',
      spoiler: true,
      helpful_count: 2,
      voted: false,
      mine: false,
      created_at_ms: 1_700_000_000_000,
    };
    const calls = useCinemaApi({ reviews: [review] });
    const user = userEvent.setup();
    renderAt('/cinema/t/1');
    await user.click(await screen.findByRole('tab', { name: 'Reviews' }));
    expect(await screen.findByText('9/10')).toBeInTheDocument();
    expect(screen.queryByText('The ending was something.')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Show spoiler' }));
    expect(screen.getByText('The ending was something.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /^Helpful/ }));
    await waitFor(() =>
      expect(calls.writes).toContainEqual({
        method: 'PUT',
        url: '/cinema/reviews/r1/helpful',
        body: null,
      }),
    );
    await user.type(screen.getByLabelText('Write a review'), 'Loved it');
    await user.click(screen.getByRole('button', { name: 'Post review' }));
    await waitFor(() =>
      expect(calls.writes).toContainEqual({
        method: 'PUT',
        url: '/cinema/titles/1/review',
        body: { body: 'Loved it', spoiler: false },
      }),
    );
  });

  it('comments: lists, posts with the spoiler flag, likes', async () => {
    const comment = {
      id: 'c1',
      author_id: '9',
      body: 'Great opening.',
      spoiler: false,
      like_count: 1,
      reply_count: 0,
      liked: false,
      mine: false,
      created_at_ms: 1_700_000_000_000,
    };
    const calls = useCinemaApi({ comments: [comment] });
    const user = userEvent.setup();
    renderAt('/cinema/t/1');
    await user.click(await screen.findByRole('tab', { name: 'Comments' }));
    expect(await screen.findByText('Great opening.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /^Like/ }));
    await waitFor(() =>
      expect(calls.writes).toContainEqual({
        method: 'PUT',
        url: '/cinema/comments/c1/like',
        body: null,
      }),
    );
    await user.type(screen.getByLabelText('Add a comment'), 'Agreed');
    await user.click(screen.getByRole('checkbox', { name: 'Contains spoilers' }));
    await user.click(screen.getByRole('button', { name: 'Post comment' }));
    await waitFor(() =>
      expect(calls.writes).toContainEqual({
        method: 'POST',
        url: '/cinema/titles/1/comments',
        body: { body: 'Agreed', spoiler: true },
      }),
    );
  });

  it('shows Resume when the title is started, and a friendly message for a missing title', async () => {
    useCinemaApi();
    renderAt('/cinema/t/5');
    expect(await screen.findByRole('button', { name: /Resume 42:10/ })).toBeInTheDocument();

    renderAt('/cinema/t/missing');
    expect(await screen.findByText('We could not find that title')).toBeInTheDocument();
  });

  it('playing an age-restricted title asks first', async () => {
    useCinemaApi({ detail: detail(RESTRICTED) });
    server.use(http.post('*/api/v1/sr/age-gate', () => new HttpResponse(null, { status: 204 })));
    const user = userEvent.setup();
    const { router } = renderAt('/cinema/t/4');
    await user.click(await screen.findByRole('button', { name: /Play/ }));
    await user.click(await screen.findByRole('button', { name: 'Yes' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/cinema/watch/4'));
  });
});
