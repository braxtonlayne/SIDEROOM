import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
} from '@tanstack/react-router';
import { act, cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PlayerGate } from '../PlayerGate';
import { closePlayerLibrary, isPlayerLibraryOpen, openPlayerLibrary } from '../presence';
import { __resetPlayerStore, play, usePlayerStore } from '../store';

const PATHS = ['/', '/login', '/invite/abc', '/help/whats-new', '/u/someone', '/channels'];

beforeEach(() => {
  __resetPlayerStore();
  closePlayerLibrary();
  sessionStorage.clear();
  vi.spyOn(HTMLMediaElement.prototype, 'play').mockImplementation(() => Promise.resolve());
  vi.spyOn(HTMLMediaElement.prototype, 'pause').mockImplementation(() => undefined);
  vi.spyOn(HTMLMediaElement.prototype, 'load').mockImplementation(() => undefined);
});
afterEach(() => {
  cleanup();
  closePlayerLibrary();
  vi.restoreAllMocks();
});

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

describe('MUS-PLY-002 music button opens the library with an empty queue', () => {
  it('shows the player on the Library tab although nothing is queued', async () => {
    await mountAt('/channels');
    expect(screen.queryByRole('region', { name: 'Music player' })).toBeNull();
    act(() => openPlayerLibrary());
    expect(await screen.findByRole('region', { name: 'Music player' })).toBeTruthy();
    expect(usePlayerStore.getState().queue).toHaveLength(0);
    expect(usePlayerStore.getState().skin).toBe('library');
    expect(screen.getByRole('tab', { name: 'Library' }).getAttribute('aria-selected')).toBe('true');
    expect(document.querySelectorAll('audio')).toHaveLength(2);
  });

  it('switches an open retro player to the library skin and tab, and Close ends the view', async () => {
    const user = userEvent.setup();
    await mountAt('/channels');
    act(() => play([{ id: 'a', title: 'Alpha', src: '/a.mp3' }]));
    await screen.findByRole('region', { name: 'Music player' });
    expect(document.querySelector('[data-skin="retro"]')).toBeTruthy();
    act(() => openPlayerLibrary());
    expect(await screen.findByRole('tab', { name: 'Library' })).toBeTruthy();
    await user.click(screen.getByRole('tab', { name: 'Queue' }));
    expect(screen.getByRole('tab', { name: 'Queue' }).getAttribute('aria-selected')).toBe('true');
    act(() => openPlayerLibrary());
    expect(screen.getByRole('tab', { name: 'Library' }).getAttribute('aria-selected')).toBe('true');
    await user.click(screen.getByRole('button', { name: 'Close player' }));
    expect(isPlayerLibraryOpen()).toBe(false);
    expect(screen.queryByRole('region', { name: 'Music player' })).toBeNull();
  });

  it('never mounts on public paths, even when the library was asked for', async () => {
    for (const p of ['/', '/login', '/invite/abc', '/help/whats-new', '/u/someone']) {
      openPlayerLibrary();
      await mountAt(p);
      expect(screen.queryByRole('region', { name: 'Music player' }), p).toBeNull();
      expect(document.querySelectorAll('audio'), p).toHaveLength(0);
      // Landing on a public page also withdraws the request.
      expect(isPlayerLibraryOpen(), p).toBe(false);
      cleanup();
    }
  });

  it('is withdrawn when the member moves onto a public page', async () => {
    const router = await mountAt('/channels');
    act(() => openPlayerLibrary());
    await screen.findByRole('region', { name: 'Music player' });
    await act(async () => {
      // /login is a separate document now (D-385); a page the gate also treats as public is in the app router.
      await router.navigate({ to: '/help/whats-new' });
    });
    expect(screen.queryByRole('region', { name: 'Music player' })).toBeNull();
    expect(isPlayerLibraryOpen()).toBe(false);
  });
});
