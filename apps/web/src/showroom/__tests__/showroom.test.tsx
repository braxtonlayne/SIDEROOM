// UX-SHOW-001: the Showroom (populated preview). The contract: it is offered only when the deployment switch is on, it
// never writes the real stores, and no request leaves the client while it is showing.
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { setClientFlags, useClientFlags } from '../../moderation/reports';
import { createAppRouter } from '../../router';
import { useStore } from '../../store/store';
import { http, HttpResponse, server } from '../../test/server';
import { FIXTURE_GAMES, FIXTURE_SERVERS, FIXTURE_SHELVES } from '../fixtures';
import { SHOWROOM_FLAG, SHOWROOM_STORAGE_KEY } from '../flag';
import { ShowroomBanner } from '../ShowroomBanner';
import { ShowroomRailSlot } from '../ShowroomRailSlot';
import { previewHint, setShowroom, useShowroom, useShowroomActive } from '../state';
import { rowsFor, shelvesFor } from '../studyModel';

/** Every request the client sends, recorded from the moment `watch()` is called. */
function watch(): { method: string; url: string }[] {
  const seen: { method: string; url: string }[] = [];
  server.events.on('request:start', ({ request }) => {
    seen.push({ method: request.method, url: request.url });
  });
  return seen;
}

function Probe() {
  return <span data-testid="probe">{String(useShowroomActive())}</span>;
}

beforeEach(() => {
  localStorage.clear();
  setClientFlags({});
  useShowroom.setState({ on: false, hint: null });
  useStore.setState({
    currentUserId: '1',
    users: { '1': { id: '1', username: 'me' } },
    guilds: { '70': { id: '70', name: 'Club' } },
  } as never);
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/shelves/visible', () => HttpResponse.json({ shelves: [] })),
  );
});

afterEach(() => {
  server.events.removeAllListeners();
  setClientFlags({});
  useShowroom.setState({ on: false, hint: null });
});

describe('the switch and the per-device choice', () => {
  it('is off when the deployment switch is absent, even if the device chose it', () => {
    useShowroom.setState({ on: true });
    render(<Probe />);
    expect(screen.getByTestId('probe')).toHaveTextContent('false');
  });

  it('is on only when the switch is on AND the device chose it, and follows the switch live', () => {
    render(<Probe />);
    act(() => setClientFlags({ [SHOWROOM_FLAG]: true }));
    expect(screen.getByTestId('probe')).toHaveTextContent('false');
    act(() => setShowroom(true));
    expect(screen.getByTestId('probe')).toHaveTextContent('true');
    act(() => setClientFlags({ [SHOWROOM_FLAG]: false }));
    expect(screen.getByTestId('probe')).toHaveTextContent('false');
  });

  it('persists the choice per device and survives blocked storage', () => {
    setShowroom(true);
    expect(localStorage.getItem(SHOWROOM_STORAGE_KEY)).toBe('1');
    setShowroom(false);
    expect(localStorage.getItem(SHOWROOM_STORAGE_KEY)).toBeNull();
    const spy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(() => setShowroom(true)).not.toThrow();
    expect(useShowroom.getState().on).toBe(true);
    spy.mockRestore();
  });
});

describe('the fixtures', () => {
  it('hold ten servers, several friend and server shelves, and both kinds of game card', () => {
    expect(FIXTURE_SERVERS).toHaveLength(10);
    expect(FIXTURE_SERVERS.some((s) => s.voice > 0)).toBe(true);
    expect(FIXTURE_SERVERS.some((s) => s.unread)).toBe(true);
    expect(shelvesFor(FIXTURE_SHELVES, 'friend').length).toBeGreaterThanOrEqual(3);
    expect(shelvesFor(FIXTURE_SHELVES, 'server').length).toBeGreaterThanOrEqual(3);
    expect(FIXTURE_GAMES.some((g) => g.status === 'live')).toBe(true);
    expect(FIXTURE_GAMES.some((g) => g.status === 'preview')).toBe(true);
  });

  it('filter by source, by shelf and by kind', () => {
    const friends = rowsFor(FIXTURE_SHELVES, 'friend', null, 'all');
    expect(friends.length).toBeGreaterThan(0);
    expect(friends.every((r) => r.shelf.source === 'friend')).toBe(true);
    const one = rowsFor(FIXTURE_SHELVES, 'all', 'fx-sh-f2', 'all');
    expect(new Set(one.map((r) => r.shelf.id))).toEqual(new Set(['fx-sh-f2']));
    const films = rowsFor(FIXTURE_SHELVES, 'all', null, 'film');
    expect(films.every((r) => r.item.kind === 'film')).toBe(true);
    // A shelf outside the chosen source filter yields nothing.
    expect(rowsFor(FIXTURE_SHELVES, 'mine', 'fx-sh-f2', 'all')).toEqual([]);
  });
});

describe('the rail and the banner', () => {
  it('adds ten sample servers under the rail only while active, with voice and unread cues', async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <ul>
        <ShowroomRailSlot />
      </ul>,
    );
    expect(screen.queryByText(/preview sample/)).not.toBeInTheDocument();
    act(() => {
      setClientFlags({ [SHOWROOM_FLAG]: true });
      setShowroom(true);
    });
    rerender(
      <ul>
        <ShowroomRailSlot />
      </ul>,
    );
    const buttons = await screen.findAllByRole('button', { name: /preview sample/ });
    expect(buttons).toHaveLength(10);
    expect(buttons.some((b) => /in voice/.test(b.getAttribute('aria-label') ?? ''))).toBe(true);
    expect(buttons.some((b) => /unread|mentions/.test(b.getAttribute('aria-label') ?? ''))).toBe(
      true,
    );
    render(<ShowroomBanner />);
    await user.click(buttons[0]!);
    expect(screen.getByTestId('showroom-hint')).toHaveTextContent(/preview sample/);
  });

  it('shows a Preview banner only while active and can turn itself off', async () => {
    const user = userEvent.setup();
    render(<ShowroomBanner />);
    expect(screen.queryByTestId('showroom-banner')).not.toBeInTheDocument();
    act(() => {
      setClientFlags({ [SHOWROOM_FLAG]: true });
      setShowroom(true);
    });
    const banner = await screen.findByTestId('showroom-banner');
    expect(banner).toHaveTextContent(/Preview/);
    expect(banner).toHaveTextContent(/made-up sample data/);
    act(() => previewHint('Preview only: nothing happened.'));
    expect(within(banner).getByTestId('showroom-hint')).toHaveTextContent('nothing happened');
    await user.click(within(banner).getByRole('button', { name: 'Turn preview off' }));
    expect(screen.queryByTestId('showroom-banner')).not.toBeInTheDocument();
    expect(useShowroom.getState().on).toBe(false);
  });
});

describe('toggling in the real pages', () => {
  const open = (path: string) => {
    const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
    // The banner is mounted at the app root in production (`app/main.tsx`); the router alone does not carry it.
    render(
      <>
        <RouterProvider router={router} />
        <ShowroomBanner />
      </>,
    );
    return router;
  };

  it('The Study: the toggle is hidden without the switch, and on/off leaves the real store untouched and sends nothing', async () => {
    const user = userEvent.setup();
    open('/study');
    await screen.findByRole('navigation', { name: 'Shelves by source' });
    expect(screen.queryByTestId('showroom-toggle')).not.toBeInTheDocument();

    act(() => setClientFlags({ [SHOWROOM_FLAG]: true }));
    const storeBefore = useStore.getState();
    const flagsBefore = useClientFlags.getState().flags;
    const toggle = await screen.findByTestId('showroom-toggle');
    expect(toggle).toHaveAttribute('aria-pressed', 'false');

    const seen = watch();
    await user.click(toggle);
    const preview = await screen.findByTestId('showroom-study');
    expect(screen.getByTestId('showroom-banner')).toBeInTheDocument();
    // Several friends and several servers are on the shelves, and the source filter narrows them.
    expect(within(preview).getByText('demo_juniper')).toBeInTheDocument();
    expect(within(preview).getByText('demo_wren')).toBeInTheDocument();
    expect(within(preview).getByText('Paper Boat Cinema')).toBeInTheDocument();
    const all = within(preview).getAllByRole('listitem').length;
    await user.click(within(preview).getByRole('button', { name: 'Friends' }));
    const friendsOnly = within(preview).getByRole('list', { name: 'Sample items' });
    expect(within(friendsOnly).getAllByRole('button').length).toBe(6);
    expect(within(friendsOnly).queryByText(/Server Paper Boat Cinema/)).not.toBeInTheDocument();
    expect(all).toBeGreaterThan(0);

    // The local metadata view, and the mutation attempts that only hint.
    await user.click(within(friendsOnly).getByRole('button', { name: /Robots of Tuesday/ }));
    const detail = await screen.findByRole('complementary', { name: 'Item details' });
    expect(detail).toHaveTextContent('kept on this device only');
    await user.click(within(detail).getByRole('button', { name: 'Pin' }));
    expect(screen.getByTestId('showroom-hint')).toHaveTextContent('nothing was opened, pinned');
    await user.click(within(detail).getByRole('button', { name: 'Edit note' }));

    // Nothing left the client and the real store was not touched.
    expect(seen).toEqual([]);
    expect(useStore.getState()).toBe(storeBefore);
    expect(useClientFlags.getState().flags).toBe(flagsBefore);

    // Off again: the real page is back, and everything it sends is a read.
    await user.click(screen.getByTestId('showroom-toggle'));
    await screen.findByRole('navigation', { name: 'Shelves by source' });
    expect(screen.queryByTestId('showroom-banner')).not.toBeInTheDocument();
    expect(screen.queryByTestId('showroom-study')).not.toBeInTheDocument();
    expect(seen.length, 'the recorder sees the real page requests').toBeGreaterThan(0);
    expect(seen.every((r) => r.method === 'GET')).toBe(true);
    expect(useStore.getState()).toBe(storeBefore);
  });

  it('The Arcade: playable builds and showcase cards are separate, and no request is made', async () => {
    const user = userEvent.setup();
    act(() => {
      setClientFlags({ [SHOWROOM_FLAG]: true });
      setShowroom(true);
    });
    const storeBefore = useStore.getState();
    const seen = watch();
    open('/games');
    const page = await screen.findByTestId('showroom-arcade');
    const live = within(page).getByRole('region', { name: 'PLAYABLE NOW' });
    const soon = within(page).getByRole('region', { name: 'PREVIEW / IN DEVELOPMENT' });
    expect(within(live).getAllByRole('article').length).toBeGreaterThanOrEqual(3);
    expect(within(soon).getAllByRole('article').length).toBeGreaterThanOrEqual(3);
    // A live card never sits in the showcase section and the other way round.
    expect(within(soon).queryByText('Doodle Relay (sample)')).not.toBeInTheDocument();
    expect(within(live).queryByText('Lantern Run (sample)')).not.toBeInTheDocument();

    await user.click(within(live).getAllByRole('button', { name: 'Play' })[0]!);
    expect(screen.getByTestId('showroom-hint')).toHaveTextContent('no game was started');
    await user.click(within(soon).getAllByRole('button', { name: 'Notify me' })[0]!);
    expect(screen.getByTestId('showroom-hint')).toHaveTextContent('no reminder was saved');

    await waitFor(() => expect(screen.getByTestId('showroom-banner')).toBeInTheDocument());
    expect(seen.filter((r) => r.url.includes('/arcade'))).toEqual([]);
    expect(seen.filter((r) => r.method !== 'GET')).toEqual([]);
    expect(useStore.getState()).toBe(storeBefore);
  });
});

describe('D-385: the fixtures are never in the app shell or the entrance', () => {
  const src = join(__dirname, '..', '..');
  const files: string[] = [];
  const walk = (dir: string) => {
    for (const name of readdirSync(dir)) {
      const p = join(dir, name);
      if (statSync(p).isDirectory()) {
        if (name === 'node_modules' || name === '__tests__') continue;
        walk(p);
      } else if (/\.(ts|tsx)$/.test(name) && !/\.test\./.test(name)) files.push(p);
    }
  };
  walk(src);

  it('are imported by the lazy preview chunks only', () => {
    const importers = files
      .filter((f) => /from '(?:\.\/|\.\.\/showroom\/)fixtures'/.test(readFileSync(f, 'utf8')))
      .map((f) => relative(src, f).split(sep).join('/'))
      .sort();
    expect(importers).toEqual([
      'showroom/PreviewArcade.tsx',
      'showroom/PreviewRailItems.tsx',
      'showroom/PreviewStudy.tsx',
      'showroom/studyModel.ts',
    ]);
  });

  it('are reached from the rest of the app only through dynamic imports', () => {
    for (const f of files) {
      const rel = relative(src, f).split(sep).join('/');
      if (rel.startsWith('showroom/')) continue;
      const text = readFileSync(f, 'utf8');
      const staticHits = [
        ...text.matchAll(/^import[^;]*from '(?:\.\.\/)+showroom\/([A-Za-z]+)';/gm),
      ];
      for (const m of staticHits)
        expect(['state', 'PreviewToggle', 'ShowroomRailSlot', 'ShowroomBanner'], rel).toContain(
          m[1],
        );
    }
  });

  it('the entrance (public surfaces) does not import the showroom at all', () => {
    for (const f of files) {
      const rel = relative(src, f).split(sep).join('/');
      if (/^(routes\/(landing|auth|invite)|components|entrance)/.test(rel))
        expect(readFileSync(f, 'utf8'), rel).not.toMatch(/showroom/);
    }
  });
});
