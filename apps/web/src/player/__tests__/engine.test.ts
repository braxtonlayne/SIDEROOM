import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { __resetPwaState, applyPendingUpdate, setUpdateReady } from '../../pwa/state';
import { createPlayerEngine, restoreResume, RESUME_KEY, type PlayerEngine } from '../engine';
import { __resetPlayerStore, pause, play, seek, setVolume, usePlayerStore } from '../store';
import type { PlayerTrack } from '../types';

/** A stand-in for HTMLAudioElement: the pieces the engine touches, and a way to fire its events. */
class FakeAudio extends EventTarget {
  src = '';
  currentTime = 0;
  duration = Number.NaN;
  paused = true;
  volume = 1;
  preload = '';
  error: { message: string } | null = null;
  playCalls = 0;
  play = vi.fn(() => {
    this.playCalls++;
    this.paused = false;
    queueMicrotask(() => this.dispatchEvent(new Event('playing')));
    return Promise.resolve();
  });
  pause = vi.fn(() => {
    this.paused = true;
  });
  load = vi.fn();
  removeAttribute = vi.fn((name: string) => {
    if (name === 'src') this.src = '';
  });
  fire(type: string) {
    this.dispatchEvent(new Event(type));
  }
}

const track = (n: number, extra: Partial<PlayerTrack> = {}): PlayerTrack => ({
  id: `t${n}`,
  title: `Track ${n}`,
  src: `/t${n}.mp3`,
  ...extra,
});
const flush = () => new Promise((r) => setTimeout(r, 0));
const state = () => usePlayerStore.getState();

let audio: FakeAudio;
let engine: PlayerEngine;

beforeEach(() => {
  sessionStorage.clear();
  __resetPwaState();
  __resetPlayerStore();
  audio = new FakeAudio();
  // No Web Audio in these tests: the element alone plays (the fallback path).
  engine = createPlayerEngine(audio as unknown as HTMLAudioElement, { contextFactory: () => null });
});
afterEach(() => {
  engine.destroy();
});

describe('engine', () => {
  it('loads and plays the queued track, and reports playing', async () => {
    play([track(1), track(2)]);
    await flush();
    expect(audio.src).toBe('/t1.mp3');
    expect(audio.playCalls).toBe(1);
    expect(state().status).toBe('playing');
  });

  it('picks up a track queued before the host mounted', async () => {
    engine.destroy();
    play(track(5));
    audio = new FakeAudio();
    engine = createPlayerEngine(audio as unknown as HTMLAudioElement, {
      contextFactory: () => null,
    });
    await flush();
    expect(audio.src).toBe('/t5.mp3');
    expect(audio.playCalls).toBe(1);
  });

  it('pauses and resumes the element with the store', async () => {
    play(track(1));
    await flush();
    pause();
    expect(audio.pause).toHaveBeenCalled();
    play();
    await flush();
    expect(audio.playCalls).toBe(2);
    expect(audio.src).toBe('/t1.mp3');
  });

  it('applies seeks and the volume', async () => {
    play(track(1));
    await flush();
    audio.duration = 120;
    audio.fire('loadedmetadata');
    expect(state().duration).toBe(120);
    seek(42);
    expect(audio.currentTime).toBe(42);
    expect(state().seekRequest).toBeNull();
    setVolume(0.25);
    expect(audio.volume).toBe(0.25);
  });

  it('mirrors the element clock into the store', async () => {
    play(track(1));
    await flush();
    audio.currentTime = 7.5;
    audio.duration = 90;
    audio.fire('timeupdate');
    expect(state().position).toBe(7.5);
    expect(state().duration).toBe(90);
  });

  it('moves to the next track when one ends, and stops after the last', async () => {
    play([track(1), track(2)]);
    await flush();
    audio.fire('ended');
    await flush();
    expect(state().index).toBe(1);
    expect(audio.src).toBe('/t2.mp3');
    audio.fire('ended');
    expect(state().status).toBe('ended');
  });

  it('asks resolveSrc for a fresh URL right before playing', async () => {
    const resolveSrc = vi.fn(() => Promise.resolve('/fresh.mp3'));
    play(track(1, { resolveSrc }));
    await flush();
    expect(resolveSrc).toHaveBeenCalledOnce();
    expect(audio.src).toBe('/fresh.mp3');
  });

  it('reports an error when the element fails', async () => {
    play(track(1));
    await flush();
    audio.error = { message: 'decode' };
    audio.fire('error');
    expect(state().status).toBe('error');
    expect(state().error).toBe('decode');
  });

  it('stays paused when the browser refuses to start sound without a gesture', async () => {
    audio.play = vi.fn(() =>
      Promise.reject(Object.assign(new Error('x'), { name: 'NotAllowedError' })),
    );
    play(track(1));
    await flush();
    expect(state().status).toBe('paused');
  });

  it('empties the element when the queue is cleared', async () => {
    play(track(1));
    await flush();
    usePlayerStore.setState({ queue: [], index: -1, status: 'idle' });
    expect(audio.pause).toHaveBeenCalled();
    expect(audio.src).toBe('');
  });
});

describe('app update while music plays (RDY-PWA-001 risk)', () => {
  const armUpdate = () => {
    const apply = vi.fn();
    setUpdateReady(apply);
    return apply;
  };

  it('does not reload under a playing track, and reloads at the track boundary', async () => {
    const apply = armUpdate();
    play([track(1), track(2)]);
    await flush();
    applyPendingUpdate();
    expect(apply).not.toHaveBeenCalled();
    audio.fire('ended');
    expect(apply).toHaveBeenCalledOnce();
    // The rest of the queue is parked so the reloaded page can offer it paused.
    const parked = JSON.parse(sessionStorage.getItem(RESUME_KEY) ?? 'null');
    expect(parked.index).toBe(1);
    expect(parked.queue).toHaveLength(2);
    // The next track was not started under the reload.
    expect(audio.src).toBe('/t1.mp3');
    expect(state().status).toBe('paused');
  });

  it('reloads at once when nothing is playing', async () => {
    const apply = armUpdate();
    applyPendingUpdate();
    expect(apply).toHaveBeenCalledOnce();
  });

  it('reloads when the member pauses', async () => {
    const apply = armUpdate();
    play(track(1));
    await flush();
    applyPendingUpdate();
    expect(apply).not.toHaveBeenCalled();
    pause();
    expect(apply).toHaveBeenCalledOnce();
  });

  it('a normal track change does not touch the update', async () => {
    const apply = armUpdate();
    play([track(1), track(2)]);
    await flush();
    audio.fire('ended');
    await flush();
    expect(apply).not.toHaveBeenCalled();
  });

  it('restoreResume puts the parked queue back paused, once', () => {
    sessionStorage.setItem(RESUME_KEY, JSON.stringify({ queue: [track(1), track(2)], index: 1 }));
    expect(restoreResume()).toBe(true);
    expect(state().queue).toHaveLength(2);
    expect(state().index).toBe(1);
    expect(state().status).toBe('paused');
    expect(restoreResume()).toBe(false);
  });

  it('restoreResume ignores junk', () => {
    sessionStorage.setItem(RESUME_KEY, '{"queue":[{"x":1}],"index":0}');
    expect(restoreResume()).toBe(false);
    sessionStorage.setItem(RESUME_KEY, 'not json');
    expect(restoreResume()).toBe(false);
  });
});
