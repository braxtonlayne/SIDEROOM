// Ratings, reviews, lists, rankings, unread counts and mark-read-up-to-here (phase 11: LIB-SOC-001/003/006/008/010/011,
// LIB-BRW-002, LIB-SER-001/002/004/005).
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../test/server';
import { localProgress, setProgressApi } from '../progress';
import type { ListDetail, ReadingList, Review, SeriesRatings } from '../social';
import { A, chapter, DONE, renderAt, useLibraryApi } from './harness';

beforeEach(() => {
  localStorage.clear();
  setProgressApi(localProgress);
});

const ratings = (o: Partial<SeriesRatings> = {}): SeriesRatings => ({
  mine: null,
  global: { average: 8.4, count: 12 },
  friends: { average: 9, count: 2 },
  known: { average: 8.6, count: 5 },
  distribution: [0, 0, 0, 0, 1, 1, 2, 4, 3, 1],
  ...o,
});

const review = (id: string, o: Partial<Review> = {}): Review => ({
  id,
  series_id: '1',
  author_id: `u${id}`,
  author_name: `Reader ${id}`,
  rating: 9,
  body: `Body of review ${id}`,
  spoiler: false,
  helpful_count: 2,
  voted: false,
  mine: false,
  created_at_ms: Date.now() - 3600_000,
  updated_at_ms: Date.now() - 3600_000,
  ...o,
});

const list = (id: string, o: Partial<ReadingList> = {}): ReadingList => ({
  id,
  owner_id: 'me',
  owner_name: 'Me',
  mine: true,
  name: `List ${id}`,
  description: '',
  visibility: 'private',
  featured: false,
  item_count: 0,
  covers: [],
  created_at_ms: 1,
  updated_at_ms: 1,
  ...o,
});

describe('ratings on the series page', () => {
  it('shows the score per scope and the distribution, and saves a rating', async () => {
    useLibraryApi();
    const puts: unknown[] = [];
    server.use(
      http.get('*/api/v1/library/series/1/ratings', () => HttpResponse.json(ratings())),
      http.put('*/api/v1/library/series/1/rating', async ({ request }) => {
        puts.push(await request.json());
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    renderAt('/library/s/1');
    const region = await screen.findByRole('region', { name: 'Ratings' });
    expect(await within(region).findByText('8.4')).toBeInTheDocument();
    expect(within(region).getByText('/ 10 · 12 ratings')).toBeInTheDocument();
    await user.click(within(region).getByRole('radio', { name: 'Friends' }));
    expect(within(region).getByText('9.0')).toBeInTheDocument();
    expect(within(region).getByText('/ 10 · 2 ratings')).toBeInTheDocument();
    await user.click(within(region).getByRole('radio', { name: 'People you know' }));
    expect(within(region).getByText('8.6')).toBeInTheDocument();
    // The distribution is reachable without the mouse.
    await user.click(within(region).getByText('How people rated it'));
    expect(within(region).getByText('8: 4 ratings')).toBeInTheDocument();
    await user.selectOptions(within(region).getByLabelText('Your rating'), '7');
    await waitFor(() => expect(puts).toEqual([{ score: 7 }]));
  });

  it('removes the rating with "Not rated"', async () => {
    useLibraryApi();
    const calls: string[] = [];
    server.use(
      http.get('*/api/v1/library/series/1/ratings', () => HttpResponse.json(ratings({ mine: 6 }))),
      http.delete('*/api/v1/library/series/1/rating', () => {
        calls.push('DELETE');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    renderAt('/library/s/1');
    const select = await screen.findByLabelText('Your rating');
    await waitFor(() => expect(select).toHaveValue('6'));
    await user.selectOptions(select, '');
    await waitFor(() => expect(calls).toEqual(['DELETE']));
  });
});

describe('reviews tab', () => {
  it('lists reviews, hides spoilers, votes helpful and pages', async () => {
    useLibraryApi();
    const seen: string[] = [];
    const votes: string[] = [];
    server.use(
      http.get('*/api/v1/library/series/1/ratings', () => HttpResponse.json(ratings())),
      http.get('*/api/v1/library/series/1/reviews', ({ request }) => {
        const u = new URL(request.url);
        seen.push(u.search);
        if (u.searchParams.get('before'))
          return HttpResponse.json({ items: [review('3')], next_before: null });
        return HttpResponse.json({
          items: [review('1', { spoiler: true }), review('2')],
          next_before: '5.2',
        });
      }),
      http.put('*/api/v1/library/reviews/2/helpful', () => {
        votes.push('PUT');
        return HttpResponse.json({ count: 3 });
      }),
    );
    const user = userEvent.setup();
    renderAt('/library/s/1');
    await user.click(await screen.findByRole('tab', { name: 'Reviews' }));
    expect(await screen.findByText('Body of review 2')).toBeInTheDocument();
    // The spoiler review stays hidden until asked for.
    expect(screen.queryByText('Body of review 1')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Show it anyway' }));
    expect(screen.getByText('Body of review 1')).toBeInTheDocument();
    await user.click(screen.getAllByRole('button', { name: 'Helpful (2)' })[1]!);
    await waitFor(() => expect(votes).toEqual(['PUT']));
    await user.click(screen.getByRole('button', { name: 'Show more reviews' }));
    expect(await screen.findByText('Body of review 3')).toBeInTheDocument();
    expect(seen.some((s) => s.includes('before=5.2'))).toBe(true);
    // Switching the sort asks again.
    await user.click(screen.getByRole('radio', { name: 'Newest' }));
    await waitFor(() => expect(seen.some((s) => s.includes('sort=recent'))).toBe(true));
  });

  it('writes a review with a rating and spoiler flag', async () => {
    useLibraryApi();
    let sent: unknown = null;
    server.use(
      http.get('*/api/v1/library/series/1/ratings', () => HttpResponse.json(ratings())),
      http.get('*/api/v1/library/series/1/reviews', () =>
        HttpResponse.json({ items: [], next_before: null }),
      ),
      http.put('*/api/v1/library/series/1/review', async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json(review('9', { mine: true }));
      }),
    );
    const user = userEvent.setup();
    renderAt('/library/s/1');
    await user.click(await screen.findByRole('tab', { name: 'Reviews' }));
    expect(await screen.findByText('No reviews yet')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Write a review' }));
    const save = screen.getByRole('button', { name: 'Post review' });
    expect(save).toBeDisabled();
    await user.type(screen.getByLabelText('Your review'), 'Loved it');
    await user.selectOptions(screen.getByLabelText('Rating with this review'), '9');
    await user.click(screen.getByRole('checkbox', { name: 'This review has spoilers' }));
    await user.click(save);
    await waitFor(() => expect(sent).toEqual({ body: 'Loved it', rating: 9, spoiler: true }));
  });
});

describe('add to list', () => {
  it('toggles membership and creates a list', async () => {
    useLibraryApi();
    const calls: string[] = [];
    server.use(
      http.get('*/api/v1/library/series/1/ratings', () => HttpResponse.json(ratings())),
      http.get('*/api/v1/library/me/lists', () =>
        HttpResponse.json({
          items: [
            list('10', { contains: true, name: 'Slow Sunday' }),
            list('11', { contains: false }),
          ],
        }),
      ),
      http.delete('*/api/v1/library/lists/10/items/1', () => {
        calls.push('remove 10');
        return HttpResponse.json(list('10'));
      }),
      http.put('*/api/v1/library/lists/11/items/1', () => {
        calls.push('add 11');
        return HttpResponse.json(list('11'));
      }),
      http.post('*/api/v1/library/lists', async ({ request }) => {
        calls.push(`create ${((await request.json()) as { name: string }).name}`);
        return HttpResponse.json(list('12', { name: 'New one' }), { status: 201 });
      }),
      http.put('*/api/v1/library/lists/12/items/1', () => {
        calls.push('add 12');
        return HttpResponse.json(list('12'));
      }),
    );
    const user = userEvent.setup();
    renderAt('/library/s/1');
    await user.click(await screen.findByRole('button', { name: 'Add to list' }));
    const dialog = await screen.findByRole('dialog', { name: 'Add to a reading list' });
    const inList = await within(dialog).findByRole('checkbox', { name: 'Slow Sunday' });
    expect(inList).toBeChecked();
    await user.click(inList);
    await waitFor(() => expect(calls).toContain('remove 10'));
    await user.click(within(dialog).getByRole('checkbox', { name: 'List 11' }));
    await waitFor(() => expect(calls).toContain('add 11'));
    await user.type(within(dialog).getByLabelText('New list'), 'New one');
    await user.click(within(dialog).getByRole('button', { name: 'Create and add' }));
    await waitFor(() => expect(calls).toContain('add 12'));
    expect(calls).toContain('create New one');
  });
});

describe('chapter list', () => {
  it('marks everything up to a chapter read', async () => {
    useLibraryApi({ chapters: [chapter(1), chapter(2), chapter(3)] });
    server.use(http.get('*/api/v1/library/series/1/ratings', () => HttpResponse.json(ratings())));
    let body: unknown = null;
    server.use(
      http.post('*/api/v1/library/chapters/c2/read', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ changed: 2 });
      }),
    );
    const user = userEvent.setup();
    renderAt('/library/s/1');
    await user.click(await screen.findByRole('button', { name: 'Mark read up to chapter 2' }));
    await waitFor(() => expect(body).toEqual({ read: true, up_to: true }));
  });
});

describe('rankings', () => {
  it('asks for the chosen period and kind and shows the ranks', async () => {
    useLibraryApi();
    const seen: string[] = [];
    server.use(
      http.get('*/api/v1/library/rankings', ({ request }) => {
        const u = new URL(request.url);
        seen.push(u.search);
        return HttpResponse.json({
          period: u.searchParams.get('period') ?? 'week',
          items: [
            { rank: 1, score: 9, readers: 3, rating_average: 8, rating_count: 2, series: A },
            { rank: 2, score: 4, readers: 1, rating_average: null, rating_count: 0, series: DONE },
          ],
        });
      }),
    );
    const user = userEvent.setup();
    renderAt('/library/rankings');
    const ranked = await screen.findByRole('list', { name: 'Ranked series' });
    const rows = within(ranked).getAllByRole('listitem');
    expect(within(rows[0]!).getByText('Frieren')).toBeInTheDocument();
    expect(within(rows[0]!).getByText('3 readers')).toBeInTheDocument();
    expect(within(rows[1]!).getByText('Monster')).toBeInTheDocument();
    await user.click(screen.getByRole('radio', { name: 'All time' }));
    await waitFor(() => expect(seen.some((s) => s.includes('period=all'))).toBe(true));
    expect(await screen.findAllByText('3 followers')).not.toHaveLength(0);
    await user.selectOptions(screen.getByLabelText('Kind'), 'manhwa');
    await waitFor(() => expect(seen.some((s) => s.includes('kind=manhwa'))).toBe(true));
  });

  it('says so when nothing is ranked', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/rankings', () => HttpResponse.json({ period: 'week', items: [] })),
    );
    renderAt('/library/rankings');
    expect(await screen.findByText('Nothing ranked yet')).toBeInTheDocument();
  });
});

describe('accessibility', () => {
  it('rankings, lists and the rating block have no axe violations', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/rankings', () =>
        HttpResponse.json({
          period: 'week',
          items: [{ rank: 1, score: 9, readers: 3, rating_average: 8, rating_count: 2, series: A }],
        }),
      ),
      http.get('*/api/v1/library/series/1/ratings', () => HttpResponse.json(ratings())),
      http.get('*/api/v1/library/me/lists', () =>
        HttpResponse.json({ items: [list('10', { name: 'Slow Sunday', item_count: 3 })] }),
      ),
    );
    const rankings = renderAt('/library/rankings');
    await screen.findByRole('list', { name: 'Ranked series' });
    expect(await axeViolations(rankings.container)).toEqual([]);
    rankings.unmount();
    const series = renderAt('/library/s/1');
    await screen.findByRole('region', { name: 'Ratings' });
    await screen.findByText('8.4');
    expect(await axeViolations(series.container)).toEqual([]);
    series.unmount();
    const lists = renderAt('/library/lists');
    await screen.findByText('Slow Sunday');
    expect(await axeViolations(lists.container)).toEqual([]);
  });
});

describe('reading lists', () => {
  it('shows mine and featured lists and creates one', async () => {
    useLibraryApi();
    let created: unknown = null;
    server.use(
      http.get('*/api/v1/library/me/lists', () =>
        HttpResponse.json({ items: [list('10', { name: 'Slow Sunday', item_count: 3 })] }),
      ),
      http.get('*/api/v1/library/lists/featured', () =>
        HttpResponse.json({
          items: [
            list('20', { name: 'Staff picks', mine: false, owner_name: 'Curator', featured: true }),
          ],
        }),
      ),
      http.post('*/api/v1/library/lists', async ({ request }) => {
        created = await request.json();
        return HttpResponse.json(list('30', { name: 'Fresh' }), { status: 201 });
      }),
      http.get('*/api/v1/library/lists/30', () =>
        HttpResponse.json({ list: list('30', { name: 'Fresh' }), items: [] } satisfies ListDetail),
      ),
    );
    const user = userEvent.setup();
    const { router } = renderAt('/library/lists');
    expect(await screen.findByText('Slow Sunday')).toBeInTheDocument();
    expect(screen.getByText('3 series · Only me')).toBeInTheDocument();
    expect(await screen.findByText('Staff picks')).toBeInTheDocument();
    expect(screen.getByText(/by Curator/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'New list' }));
    await user.type(screen.getByLabelText('Name'), 'Fresh');
    await user.selectOptions(screen.getByLabelText('Who can see it'), 'public');
    await user.click(screen.getByRole('button', { name: 'Create list' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/library/lists/30'));
    expect(created).toEqual({ name: 'Fresh', description: '', visibility: 'public' });
  });

  it('lets the owner reorder, remove, rename and delete', async () => {
    useLibraryApi();
    const calls: string[] = [];
    const detail: ListDetail = {
      list: list('10', { name: 'Slow Sunday', item_count: 2 }),
      items: [
        { series: A, position: 1, note: '', added_at_ms: 1 },
        { series: DONE, position: 2, note: 'finished', added_at_ms: 2 },
      ],
    };
    server.use(
      http.get('*/api/v1/library/lists/10', () => HttpResponse.json(detail)),
      http.put('*/api/v1/library/lists/10/order', async ({ request }) => {
        calls.push(`order ${JSON.stringify(await request.json())}`);
        return HttpResponse.json(detail.list);
      }),
      http.delete('*/api/v1/library/lists/10/items/3', () => {
        calls.push('remove 3');
        return HttpResponse.json(detail.list);
      }),
      http.patch('*/api/v1/library/lists/10', async ({ request }) => {
        calls.push(`patch ${JSON.stringify(await request.json())}`);
        return HttpResponse.json(detail.list);
      }),
      http.delete('*/api/v1/library/lists/10', () => {
        calls.push('delete');
        return new HttpResponse(null, { status: 204 });
      }),
      http.get('*/api/v1/library/me/lists', () => HttpResponse.json({ items: [] })),
      http.get('*/api/v1/library/lists/featured', () => HttpResponse.json({ items: [] })),
    );
    const user = userEvent.setup();
    const { router } = renderAt('/library/lists/10');
    expect(await screen.findByRole('heading', { name: 'Slow Sunday' })).toBeInTheDocument();
    expect(screen.getByText(/finished/)).toBeInTheDocument();
    // The first can only go down, the last only up.
    expect(screen.getByRole('button', { name: 'Move Frieren up' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Move Monster down' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Move Frieren down' }));
    await waitFor(() => expect(calls).toContain('order {"series_ids":["3","1"]}'));
    await user.click(screen.getByRole('button', { name: 'Remove Monster from the list' }));
    await waitFor(() => expect(calls).toContain('remove 3'));
    await user.click(screen.getByRole('button', { name: 'Edit list' }));
    const name = screen.getByLabelText('Name');
    await user.clear(name);
    await user.type(name, 'Renamed');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() =>
      expect(calls).toContain('patch {"name":"Renamed","description":"","visibility":"private"}'),
    );
    await user.click(await screen.findByRole('button', { name: 'Delete list' }));
    const confirm = await screen.findByRole('dialog', { name: 'Delete Slow Sunday?' });
    await user.click(within(confirm).getByRole('button', { name: 'Delete list' }));
    await waitFor(() => expect(calls).toContain('delete'));
    await waitFor(() => expect(router.state.location.pathname).toBe('/library/lists'));
  });

  it("shows another member's list read-only and answers a hidden one", async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/lists/20', () =>
        HttpResponse.json({
          list: list('20', {
            mine: false,
            owner_name: 'Curator',
            name: 'Staff picks',
            visibility: 'public',
          }),
          items: [{ series: A, position: 1, note: '', added_at_ms: 1 }],
        } satisfies ListDetail),
      ),
      http.get('*/api/v1/library/lists/21', () =>
        HttpResponse.json({ message: 'Unknown list' }, { status: 404 }),
      ),
    );
    const view = renderAt('/library/lists/20');
    expect(await screen.findByRole('heading', { name: 'Staff picks' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Edit list' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Move Frieren down' })).toBeNull();
    view.unmount();
    renderAt('/library/lists/21');
    expect(await screen.findByText('This list is not available')).toBeInTheDocument();
  });
});

describe('my library and history', () => {
  it('shows unread counts and sorts by them', async () => {
    useLibraryApi();
    server.use(
      http.get('*/api/v1/library/me/follows', () =>
        HttpResponse.json({
          items: [
            { series: A, follow: { shelf: 'reading', notify: true, created_at_ms: 2 } },
            { series: DONE, follow: { shelf: 'reading', notify: true, created_at_ms: 1 } },
          ],
          total: 2,
          limit: 60,
          offset: 0,
        }),
      ),
      http.get('*/api/v1/library/me/unread', () =>
        HttpResponse.json({
          items: [
            { series_id: '1', unread: 1, last_read_at_ms: 10, chapter_count: 3 },
            { series_id: '3', unread: 7, last_read_at_ms: 5, chapter_count: 9 },
          ],
        }),
      ),
    );
    const user = userEvent.setup();
    renderAt('/library/me');
    expect(await screen.findByText('7 unread')).toBeInTheDocument();
    expect(screen.getByText('1 unread')).toBeInTheDocument();
    const titles = () =>
      [...document.querySelectorAll('.lib-grid .lib-card')].map((c) => c.textContent ?? '');
    await user.selectOptions(screen.getByLabelText('Sort by'), 'unread');
    await waitFor(() => expect(titles()[0]).toMatch(/Monster/));
    await user.selectOptions(screen.getByLabelText('Sort by'), 'title');
    await waitFor(() => expect(titles()[0]).toMatch(/Frieren/));
  });

  it('removes an entry from the reading history', async () => {
    useLibraryApi();
    const removed: string[] = [];
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
    const user = userEvent.setup();
    renderAt('/library/history');
    expect(await screen.findByText('Chapter 36 · Page 2 of 6')).toBeInTheDocument();
    const original = localProgress.remove;
    localProgress.remove = (id) => {
      removed.push(id);
      return original(id);
    };
    try {
      await user.click(screen.getByRole('button', { name: 'Remove Frieren from your history' }));
      await waitFor(() => expect(removed).toEqual(['c36']));
      expect(await screen.findByText('No reading yet')).toBeInTheDocument();
    } finally {
      localProgress.remove = original;
    }
  });
});
