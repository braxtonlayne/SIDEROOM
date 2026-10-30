import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PlayerGate } from '../PlayerGate';
import { __resetPlayerStore, play, usePlayerStore } from '../store';

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

describe('PlayerGate and host', () => {
  it('renders nothing until something is queued', () => {
    const { container } = render(<PlayerGate />);
    expect(container.innerHTML).toBe('');
  });

  it('mounts the host on the first queued track, with one audio element and the default skin', async () => {
    render(<PlayerGate />);
    act(() => play([{ id: 'a', title: 'Alpha', src: '/a.mp3' }]));
    expect(await screen.findByRole('region', { name: 'Music player' })).toBeTruthy();
    expect(document.querySelectorAll('audio')).toHaveLength(1);
    expect(document.querySelector('[data-skin="retro"]')).toBeTruthy();
    expect(document.querySelector('.sr-ply-dock')?.getAttribute('data-dock')).toBe('bottom-end');
  });

  it('keeps the audio element after the queue is cleared (it outlives the skins)', async () => {
    render(<PlayerGate />);
    act(() => play([{ id: 'a', title: 'Alpha', src: '/a.mp3' }]));
    await screen.findByRole('region', { name: 'Music player' });
    act(() => usePlayerStore.setState({ queue: [], index: -1, status: 'idle' }));
    expect(screen.queryByRole('region', { name: 'Music player' })).toBeNull();
    expect(document.querySelectorAll('audio')).toHaveLength(1);
  });

  it('wears the chosen skin', async () => {
    usePlayerStore.setState({ skin: 'pill' });
    render(<PlayerGate />);
    act(() => play([{ id: 'a', title: 'Alpha', src: '/a.mp3' }]));
    await screen.findByRole('region', { name: 'Music player' });
    expect(document.querySelector('[data-skin="pill"]')).toBeTruthy();
    act(() => usePlayerStore.setState({ skin: 'library' }));
    expect(document.querySelector('.sr-ply-library')).toBeTruthy();
  });

  it('handles the player keys on any route', async () => {
    render(<PlayerGate />);
    act(() =>
      play([
        { id: 'a', title: 'Alpha', src: '/a.mp3' },
        { id: 'b', title: 'Bravo', src: '/b.mp3' },
      ]),
    );
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
