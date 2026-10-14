import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../../router';
import { http, HttpResponse, server } from '../../../test/server';
import type { CinemaList, ContinueItem, HistoryItem, TitleCard } from '../types';

const card = (id: string, o: Partial<TitleCard> = {}): TitleCard => ({
  id,
  title: `Title ${id}`,
  kind: 'movie',
  year: 1998,
  poster_url: null,
  ...o,
});

interface Fixture {
  saved?: TitleCard[];
  cont?: ContinueItem[];
  lists?: CinemaList[];
  history?: HistoryItem[];
  paused?: boolean;
}

function serve(f: Fixture = {}) {
  const calls: string[] = [];
  let paused = f.paused ?? false;
  let history = f.history ?? [];
  let saved = f.saved ?? [card('1'), card('2', { kind: 'series', year: 2022 })];
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/cinema/me/list', () => HttpResponse.json({ items: saved })),
    http.delete('*/api/v1/cinema/me/list/:id', ({ params }) => {
      calls.push(`DELETE list ${String(params.id)}`);
      saved = saved.filter((t) => t.id !== params.id);
      return new HttpResponse(null, { status: 204 });
    }),
    http.get('*/api/v1/cinema/me/continue', () =>
      HttpResponse.json({
        items: f.cont ?? [
          {
            title: card('3', { kind: 'series', title: 'Frieren' }),
            episode_id: 'e9',
            episode_label: 'S1 · E9',
            position_s: 600,
            duration_s: 1500,
            updated_at_ms: 1,
          },
        ],
      }),
    ),
    http.get('*/api/v1/cinema/me/lists', () =>
      HttpResponse.json({
        items: f.lists ?? [{ id: 'l1', name: 'Rainy day', visibility: 'friends', item_count: 2 }],
      }),
    ),
    http.get('*/api/v1/cinema/me/lists/:id', () =>
      HttpResponse.json({
        list: { id: 'l1', name: 'Rainy day', visibility: 'friends', item_count: 1 },
        items: [card('7', { title: 'Paprika' })],
      }),
    ),
    http.get('*/api/v1/cinema/me/history', () => HttpResponse.json({ items: history, paused })),
    http.put('*/api/v1/cinema/me/history', async ({ request }) => {
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

const histItem = (id: string, o: Partial<HistoryItem> = {}): HistoryItem => ({
  id,
  title: card('3', { title: 'Frieren', kind: 'series' }),
  episode_id: 'e9',
  episode_label: 'S1 · E9',
  watched_at_ms: Date.UTC(2026, 9, 1),
  completed: true,
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

  it('continue watching links straight into the player at the episode, with progress', async () => {
    serve();
    open();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Continue watching' }));
    const grid = await screen.findByRole('list', { name: 'Continue watching' });
    expect(within(grid).getByRole('link', { name: 'Frieren' })).toHaveAttribute(
      'href',
      '/cinema/watch/3?ep=e9',
    );
    expect(within(grid).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '40');
  });

  it('shows custom lists and expands one', async () => {
    serve();
    open();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Lists' }));
    expect(await screen.findByText('2 titles · Friends')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Show titles in Rainy day' }));
    expect(await screen.findByRole('link', { name: 'Paprika' })).toBeInTheDocument();
  });

  it('pauses and clears the watch history after confirmation', async () => {
    const calls = serve({ history: [histItem('h1'), histItem('h2', { completed: false })] });
    open();
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'History' }));
    const list = await screen.findByRole('list', { name: 'Watch history' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(2);
    expect(within(list).getByText('S1 · E9 · In progress')).toBeInTheDocument();

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
