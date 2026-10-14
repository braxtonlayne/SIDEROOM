import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../../router';
import { http, HttpResponse, server } from '../../../test/server';
import type { CinemaList, ContinueItem, HistoryItem, ListEntry, TitleRef } from '../types';

const ref = (id: string, o: Partial<TitleRef> = {}): TitleRef => ({
  id,
  title: `Title ${id}`,
  kind: 'movie',
  year: 1998,
  age_restricted: false,
  poster_key: null,
  ...o,
});
const progress = (title: string, episode: string, o = {}) => ({
  title_id: title,
  episode_id: episode,
  position_secs: 600,
  duration_secs: 1500,
  percent: 40,
  completed: false,
  updated_at_ms: Date.UTC(2026, 9, 1, 12),
  ...o,
});
const episode = (title: string, id: string, number: number) => ({
  id,
  title_id: title,
  season: 1,
  number,
  title: `Ep ${number}`,
  runtime_secs: 1500,
});

interface Fixture {
  saved?: unknown[];
  cont?: ContinueItem[];
  lists?: CinemaList[];
  history?: HistoryItem[];
  paused?: boolean;
}

function serve(f: Fixture = {}) {
  const calls: string[] = [];
  let paused = f.paused ?? false;
  let history = f.history ?? [];
  let saved = f.saved ?? [ref('1'), ref('2', { kind: 'series', year: 2022 })];
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/cinema/me/list', () => HttpResponse.json({ items: saved })),
    http.delete('*/api/v1/cinema/me/list/:id', ({ params }) => {
      calls.push(`DELETE list ${String(params.id)}`);
      saved = saved.filter((t) => (t as TitleRef).id !== params.id);
      return new HttpResponse(null, { status: 204 });
    }),
    http.get('*/api/v1/cinema/me/continue-watching', () =>
      HttpResponse.json({
        items: f.cont ?? [
          {
            reason: 'resume',
            title: ref('3', { kind: 'series', title: 'Frieren' }),
            episode: episode('3', 'e9', 9),
            progress: progress('3', 'e9'),
          },
          {
            reason: 'up_next',
            title: ref('4', { kind: 'anime', title: 'Mushishi' }),
            episode: episode('4', 'e3', 3),
          },
        ],
      }),
    ),
    http.get('*/api/v1/cinema/lists', () =>
      HttpResponse.json({
        lists: f.lists ?? [
          { id: 'saved', name: 'My list', kind: 'saved', visibility: 'private', item_count: 2 },
          { id: 'l1', name: 'Rainy day', kind: 'custom', visibility: 'friends', item_count: 2 },
        ],
      }),
    ),
    http.get('*/api/v1/cinema/lists/:id', () =>
      HttpResponse.json({
        list: { id: 'l1', name: 'Rainy day', kind: 'custom', visibility: 'friends', item_count: 1 },
        items: [
          { title: ref('7', { title: 'Paprika' }), note: null, added_at_ms: 1 } satisfies ListEntry,
        ],
      }),
    ),
    http.get('*/api/v1/cinema/me/history', () => HttpResponse.json({ items: history })),
    http.get('*/api/v1/cinema/me/history/settings', () => HttpResponse.json({ paused })),
    http.put('*/api/v1/cinema/me/history/settings', async ({ request }) => {
      paused = ((await request.json()) as { paused: boolean }).paused;
      calls.push(`PUT history paused=${paused}`);
      return new HttpResponse(null, { status: 204 });
    }),
    http.delete('*/api/v1/cinema/me/history', () => {
      calls.push('DELETE history');
      history = [];
      return new HttpResponse(null, { status: 204 });
    }),
  );
  return calls;
}

function open() {
  const router = createAppRouter(createMemoryHistory({ initialEntries: ['/cinema/my-list'] }));
  return { router, ...render(<RouterProvider router={router} />) };
}

const histItem = (o: Partial<HistoryItem> = {}): HistoryItem => ({
  title: ref('3', { title: 'Frieren', kind: 'series' }),
  episode: episode('3', 'e9', 9),
  progress: progress('3', 'e9', { completed: true, percent: 100 }),
  ...o,
});

describe('My list (CIN-MY-001)', () => {
  it('shows the saved titles with links and passes axe', async () => {
    serve();
    const { container } = open();
    expect(await screen.findByRole('heading', { name: 'My list.' })).toBeInTheDocument();
    const grid = await screen.findByRole('list', { name: 'Saved titles' });
    expect(within(grid).getByRole('link', { name: 'Title 1' })).toHaveAttribute(
      'href',
      '/cinema/t/1',
    );
    expect(within(grid).getByText('2022 · Series')).toBeInTheDocument();
    const found = await axeViolations(container);
    expect(found.map((v) => JSON.stringify(v).slice(0, 500))).toEqual([]);
  });

  it('reads saved items wrapped as {title} too', async () => {
    serve({ saved: [{ title: ref('9', { title: 'Wrapped' }), added_at_ms: 1 }] });
    open();
    expect(await screen.findByRole('link', { name: 'Wrapped' })).toBeInTheDocument();
  });

  it('removes a saved title', async () => {
    const calls = serve();
    open();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Remove Title 1 from My list' }));
    await waitFor(() => expect(screen.queryByText('Title 1')).not.toBeInTheDocument());
    expect(calls).toEqual(['DELETE list 1']);
  });

  it('explains an empty list', async () => {
    serve({ saved: [] });
    open();
    expect(await screen.findByText('Nothing saved yet')).toBeInTheDocument();
  });

  it('continue watching opens the episode in the player; up next has no progress bar', async () => {
    serve();
    open();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Continue watching' }));
    const grid = await screen.findByRole('list', { name: 'Continue watching' });
    expect(within(grid).getByRole('link', { name: 'Frieren' })).toHaveAttribute(
      'href',
      '/cinema/watch/3?episode=e9',
    );
    expect(within(grid).getByText('S1 · E9 · Ep 9')).toBeInTheDocument();
    expect(within(grid).getAllByRole('progressbar')).toHaveLength(1);
    expect(within(grid).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '40');
    expect(within(grid).getByText('Up next · S1 · E3 · Ep 3')).toBeInTheDocument();
    expect(within(grid).getByRole('link', { name: 'Mushishi' })).toHaveAttribute(
      'href',
      '/cinema/watch/4?episode=e3',
    );
  });

  it('shows custom lists only (not the saved list) and expands one', async () => {
    serve();
    open();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Lists' }));
    expect(await screen.findByText('2 titles · Friends')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'My list' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Show titles in Rainy day' }));
    expect(await screen.findByRole('link', { name: 'Paprika' })).toBeInTheDocument();
  });

  it('pauses and clears the watch history after confirmation', async () => {
    const calls = serve({
      history: [
        histItem(),
        histItem({
          title: ref('5', { title: 'Akira' }),
          episode: null,
          progress: progress('5', '0', { completed: false }),
        }),
      ],
    });
    open();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'History' }));
    const list = await screen.findByRole('list', { name: 'Watch history' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(2);
    expect(within(list).getByText('S1 · E9 · Ep 9 · Watched')).toBeInTheDocument();
    expect(within(list).getByText('In progress')).toBeInTheDocument();
    expect(within(list).getByRole('link', { name: 'Akira' })).toHaveAttribute(
      'href',
      '/cinema/watch/5',
    );

    await user.click(screen.getByRole('switch', { name: /Pause watch history/ }));
    await waitFor(() => expect(calls).toContain('PUT history paused=true'));
    await waitFor(() => expect(screen.getByRole('switch', { name: /Pause/ })).toBeChecked());

    await user.click(screen.getByRole('button', { name: 'Clear history' }));
    const dialog = await screen.findByRole('dialog', { name: 'Clear your watch history?' });
    expect(calls).not.toContain('DELETE history'); // nothing happens before the confirmation
    await user.click(within(dialog).getByRole('button', { name: 'Clear history' }));
    await waitFor(() => expect(calls).toContain('DELETE history'));
    expect(await screen.findByText('History is paused')).toBeInTheDocument();
  });

  it('offers a retry when a section fails to load', async () => {
    serve();
    server.use(http.get('*/api/v1/cinema/me/list', () => HttpResponse.json({}, { status: 500 })));
    open();
    expect(await screen.findByText('We could not load your list')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });
});
