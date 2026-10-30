import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
} from '@tanstack/react-router';
import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PlayerGate } from '../PlayerGate';
import { isPublicPath } from '../scope';
import { __resetPlayerStore, play, RESUME_KEY, stopAndForget, usePlayerStore } from '../store';

const TRACK = { id: 'a', title: 'Alpha', src: '/a.mp3' };
const PATHS = [
  '/',
  '/login',
  '/invite/abc',
  '/help/whats-new',
  '/u/someone',
  '/channels',
  '/cinema',
  '/settings/user/appearance',
];

beforeEach(() => {
  __resetPlayerStore();
  sessionStorage.clear();
  // jsdom has no media pipeline; the engine only needs these to exist.
  vi.spyOn(HTMLMediaElement.prototype, 'play').mockImplementation(() => Promise.resolve());
  vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => undefined);
  vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => undefined);
});
afterEach(() => {
  vi.restoreAllMocks();
});

/** The gate at the root of a small router, like router.tsx's root layout. */
async function mountAt(path: string) {
  const root = createRootRoute({
    component: () => (
      <>
        <Outlet />
        <PlayerGate />
      </>
    ),
  });
  const router = createRouter({
    routeTree: root.addChildren(
      PATHS.map((p) => createRoute({ getParentRoute: () => root, path: p, component: () => null })),
    ),
    history: createMemoryHistory({ initialEntries: [path] }),
  });
  render(<RouterProvider router={router} />);
  await act(async () => {
    await router.load();
  });
  return router;
}

const go = (router: Awaited<ReturnType<typeof mountAt>>, to: string) =>
  act(async () => {
    await router.navigate({ to });
  });

const press = (key: string) =>
  act(() => {
    document.dispatchEvent(
      new KeyboardEvent('keydown', {
        key,
        altKey: true,
        shiftKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
  });

describe('public paths', () => {
  it('knows the entrance, handbook, profile and consent pages', () => {
    for (const p of [
      '/',
      '/login',
      '/register',
      '/forgot',
      '/reset/x',
      '/join/abc',
      '/invite/abc',
      '/template/x',
      '/help',
      '/help/whats-new',
      '/u/x',
      '/~x',
      '/oauth2/authorize',
      '/house-notes',
    ])
      expect(isPublicPath(p), p).toBe(true);
    for (const p of [
      '/channels',
      '/channels/@me',
      '/cinema',
      '/cinema/t/4',
      '/library',
      '/settings/user/appearance',
      '/admin',
      '/developers',
      '/support',
    ])
      expect(isPublicPath(p), p).toBe(false);
  });
});

describe('PlayerGate', () => {
  it('renders nothing until something is queued, even on an app route', async () => {
    await mountAt('/channels');
    expect(document.querySelectorAll('audio')).toHaveLength(0);
  });

  it('mounts on a signed-in route with the two gapless decks, the default skin and its own theme root', async () => {
    await mountAt('/channels');
    act(() => play([TRACK]));
    expect(await screen.findByRole('region', { name: 'Music player' })).toBeTruthy();
    expect(document.querySelectorAll('audio')).toHaveLength(2);
    expect(screen.getByTestId('player-audio')).toBeTruthy();
    expect(screen.getByTestId('player-audio-next')).toBeTruthy();
    expect(document.querySelector('[data-skin="retro"]')).toBeTruthy();
    expect(document.querySelector('.sr-ply-dock')?.getAttribute('data-dock')).toBe('bottom-end');
    // The dock carries the Lounge tokens itself: it sits outside every section shell.
    const root = document.querySelector('.sr-ply-theme') as HTMLElement;
    expect(root.style.getPropertyValue('--ui-bg')).not.toBe('');
    expect(root.contains(document.querySelector('.sr-ply-dock'))).toBe(true);
  });

  it('never mounts on the landing page, login, an invite, the handbook or a public profile', async () => {
    for (const p of ['/', '/login', '/invite/abc', '/help/whats-new', '/u/someone']) {
      __resetPlayerStore();
      play([TRACK]);
      await mountAt(p);
      expect(screen.queryByRole('region', { name: 'Music player' }), p).toBeNull();
      expect(document.querySelectorAll('audio'), p).toHaveLength(0);
      // Whatever was queued is dropped there.
      expect(usePlayerStore.getState().queue, p).toHaveLength(0);
      cleanup();
    }
  });

  it('stays mounted, with the same audio element, between signed-in routes', async () => {
    const router = await mountAt('/cinema');
    act(() => play([TRACK]));
    await screen.findByRole('region', { name: 'Music player' });
    const audio = document.querySelector('audio');
    await go(router, '/settings/user/appearance');
    await go(router, '/channels');
    expect(document.querySelector('audio')).toBe(audio);
    expect(screen.getByRole('region', { name: 'Music player' })).toBeTruthy();
    expect(usePlayerStore.getState().queue).toHaveLength(1);
  });

  it('stops and forgets the queue, the parked one too, when the member lands on a public page (sign-out)', async () => {
    const router = await mountAt('/channels');
    act(() => play([TRACK]));
    await screen.findByRole('region', { name: 'Music player' });
    sessionStorage.setItem(RESUME_KEY, '{"queue":[],"index":0}');
    await go(router, '/login');
    expect(screen.queryByRole('region', { name: 'Music player' })).toBeNull();
    expect(document.querySelectorAll('audio')).toHaveLength(0);
    expect(usePlayerStore.getState().queue).toHaveLength(0);
    expect(usePlayerStore.getState().status).toBe('idle');
    expect(sessionStorage.getItem(RESUME_KEY)).toBeNull();
    // Coming back in starts fresh.
    await go(router, '/channels');
    expect(screen.queryByRole('region', { name: 'Music player' })).toBeNull();
  });

  it('stopAndForget clears the queue and the parked copy', () => {
    play([TRACK]);
    sessionStorage.setItem(RESUME_KEY, 'x');
    stopAndForget();
    expect(usePlayerStore.getState().queue).toHaveLength(0);
    expect(sessionStorage.getItem(RESUME_KEY)).toBeNull();
  });

  it('wears the chosen skin', async () => {
    usePlayerStore.setState({ skin: 'pill' });
    await mountAt('/channels');
    act(() => play([TRACK]));
    await screen.findByRole('region', { name: 'Music player' });
    expect(document.querySelector('[data-skin="pill"].sr-ply')).toBeTruthy();
    act(() => usePlayerStore.setState({ skin: 'library' }));
    expect(document.querySelector('.sr-ply-library')).toBeTruthy();
  });

  it('handles the player keys on any app route', async () => {
    await mountAt('/cinema');
    act(() => play([TRACK, { id: 'b', title: 'Bravo', src: '/b.mp3' }]));
    await screen.findByRole('region', { name: 'Music player' });
    act(() => usePlayerStore.setState({ status: 'playing' }));
    press('P');
    expect(usePlayerStore.getState().status).toBe('paused');
    press('P');
    expect(usePlayerStore.getState().status).toBe('playing');
    press('ArrowRight');
    expect(usePlayerStore.getState().index).toBe(1);
    press('M');
    expect(usePlayerStore.getState().muted).toBe(true);
  });
});
