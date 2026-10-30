import { beforeEach, describe, expect, it } from 'vitest';
import {
  __resetPlayerStore,
  clear,
  currentTrack,
  cycleRepeat,
  enqueue,
  engineReport,
  next,
  nextIndex,
  pause,
  play,
  prev,
  registerLibraryPanel,
  removeAt,
  sanitizePersisted,
  seek,
  setCrossfade,
  setDock,
  setEq,
  setSkin,
  setVolume,
  toggle,
  toggleMute,
  usePlayerStore,
} from '../store';
import type { PlayerTrack } from '../types';

const track = (n: number): PlayerTrack => ({ id: `t${n}`, title: `Track ${n}`, src: `/t${n}.mp3` });
const state = () => usePlayerStore.getState();

beforeEach(() => {
  __resetPlayerStore();
});

describe('nextIndex', () => {
  it('stops at the end unless repeat is on', () => {
    expect(nextIndex(2, 3, 'off', true)).toBeNull();
    expect(nextIndex(2, 3, 'off', false)).toBeNull();
    expect(nextIndex(2, 3, 'all', true)).toBe(0);
    expect(nextIndex(2, 3, 'all', false)).toBe(0);
    expect(nextIndex(0, 3, 'off', true)).toBe(1);
  });
  it('repeat-one replays only when a track ends by itself', () => {
    expect(nextIndex(1, 3, 'one', true)).toBe(1);
    expect(nextIndex(1, 3, 'one', false)).toBe(2);
    expect(nextIndex(0, 0, 'all', true)).toBeNull();
  });
});

describe('play, pause and toggle', () => {
  it('replaces the queue and starts loading the chosen track', () => {
    play([track(1), track(2), track(3)], 1);
    expect(state().queue).toHaveLength(3);
    expect(state().index).toBe(1);
    expect(state().status).toBe('loading');
    expect(currentTrack()?.id).toBe('t2');
  });
  it('takes one track, a number and nothing', () => {
    play(track(1));
    expect(state().queue).toHaveLength(1);
    enqueue([track(2), track(3)]);
    play(2);
    expect(currentTrack()?.id).toBe('t3');
    engineReport.status('playing');
    pause();
    expect(state().status).toBe('paused');
    play();
    expect(state().status).toBe('playing');
  });
  it('ignores an empty list and clamps the start index', () => {
    play([]);
    expect(state().queue).toHaveLength(0);
    play([track(1), track(2)], 99);
    expect(state().index).toBe(1);
  });
  it('play() with nothing queued does nothing; toggle flips', () => {
    play();
    expect(state().status).toBe('idle');
    play(track(1));
    engineReport.status('playing');
    toggle();
    expect(state().status).toBe('paused');
    toggle();
    expect(state().status).toBe('playing');
  });
  it('restarts a finished queue from the top', () => {
    play([track(1), track(2)]);
    next(true);
    next(true);
    expect(state().status).toBe('ended');
    play();
    expect(state().index).toBe(0);
    expect(state().status).toBe('loading');
  });
});

describe('queue', () => {
  it('enqueues at the end or right after the current track', () => {
    play([track(1), track(2)]);
    enqueue(track(3));
    enqueue(track(9), { next: true });
    expect(state().queue.map((t) => t.id)).toEqual(['t1', 't9', 't2', 't3']);
    expect(state().index).toBe(0);
  });
  it('enqueue starts nothing by itself', () => {
    enqueue(track(1));
    expect(state().status).toBe('idle');
    expect(state().index).toBe(-1);
  });
  it('removeAt keeps the current track current', () => {
    play([track(1), track(2), track(3)], 1);
    removeAt(0);
    expect(currentTrack()?.id).toBe('t2');
    expect(state().index).toBe(0);
    removeAt(1);
    expect(currentTrack()?.id).toBe('t2');
  });
  it('removing the current track starts its successor, and the last one empties the player', () => {
    play([track(1), track(2)], 0);
    removeAt(0);
    expect(currentTrack()?.id).toBe('t2');
    expect(state().status).toBe('loading');
    removeAt(0);
    expect(state().queue).toHaveLength(0);
    expect(state().status).toBe('idle');
  });
  it('clear stops everything', () => {
    play([track(1)]);
    clear();
    expect(state().index).toBe(-1);
    expect(state().status).toBe('idle');
  });
});

describe('next and prev', () => {
  it('next walks the queue and ends at the last track', () => {
    play([track(1), track(2)]);
    next();
    expect(state().index).toBe(1);
    next(true);
    expect(state().status).toBe('ended');
  });
  it('repeat all wraps; repeat one replays on auto-advance', () => {
    play([track(1), track(2)], 1);
    cycleRepeat();
    expect(state().repeat).toBe('all');
    next(true);
    expect(state().index).toBe(0);
    cycleRepeat();
    expect(state().repeat).toBe('one');
    next(true);
    expect(state().index).toBe(0);
    expect(state().status).toBe('loading');
    cycleRepeat();
    expect(state().repeat).toBe('off');
  });
  it('prev restarts after three seconds, otherwise goes back', () => {
    play([track(1), track(2)], 1);
    engineReport.time(10, 100);
    prev();
    expect(state().index).toBe(1);
    expect(state().seekRequest).toBe(0);
    engineReport.time(1, 100);
    prev();
    expect(state().index).toBe(0);
  });
});

describe('seek, volume and the member choices', () => {
  it('seek clamps to the duration and asks the engine', () => {
    play(track(1));
    engineReport.time(0, 200);
    seek(500);
    expect(state().seekRequest).toBe(200);
    seek(-4);
    expect(state().seekRequest).toBe(0);
    engineReport.seekDone();
    expect(state().seekRequest).toBeNull();
  });
  it('seek with nothing loaded does nothing', () => {
    seek(5);
    expect(state().seekRequest).toBeNull();
  });
  it('volume clamps, and moving it unmutes', () => {
    toggleMute();
    expect(state().muted).toBe(true);
    setVolume(3);
    expect(state().volume).toBe(1);
    expect(state().muted).toBe(false);
    setVolume(Number.NaN);
    expect(state().volume).toBe(0);
  });
  it('skin, dock, EQ and crossfade validate their input', () => {
    setSkin('pill');
    expect(state().skin).toBe('pill');
    setSkin('nope' as never);
    expect(state().skin).toBe('pill');
    setDock('top-start');
    expect(state().dock).toBe('top-start');
    setDock('middle' as never);
    expect(state().dock).toBe('top-start');
    setEq([1, 2, 3]);
    expect(state().eq.every((g) => g === 0)).toBe(true);
    setEq([20, -20, 0, 0, 0, 0, 0, 0, 0, 3]);
    expect(state().eq[0]).toBe(12);
    expect(state().eq[1]).toBe(-12);
    setCrossfade(99);
    expect(state().crossfadeSec).toBe(12);
  });
});

describe('persisted choices', () => {
  it('drops anything invalid', () => {
    expect(sanitizePersisted(null)).toEqual({});
    expect(
      sanitizePersisted({
        volume: 7,
        skin: 'bogus',
        dock: 'top-end',
        repeat: 'all',
        eq: [1],
        crossfadeSec: -3,
        muted: 'yes',
      }),
    ).toEqual({ volume: 1, dock: 'top-end', repeat: 'all', crossfadeSec: 0 });
  });
  it('keeps a well-formed EQ', () => {
    const eq = [0, 1, 2, 3, 4, 5, 6, 7, 8, 40];
    expect(sanitizePersisted({ eq }).eq?.[9]).toBe(12);
  });
});

describe('library slot', () => {
  it('registers a panel and removes it again', () => {
    const Panel = () => null;
    const off = registerLibraryPanel(Panel);
    expect(state().libraryPanel).toBe(Panel);
    off();
    expect(state().libraryPanel).toBeNull();
  });
  it('a stale remover does not drop a newer panel', () => {
    const A = () => null;
    const B = () => null;
    const offA = registerLibraryPanel(A);
    registerLibraryPanel(B);
    offA();
    expect(state().libraryPanel).toBe(B);
  });
});
