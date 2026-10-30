import { describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../../api/client';
import { JukeboxController, TICK_MS, type AudioLike } from '../controller';
import type { JukeboxApi, JukeboxItem, JukeboxState } from '../types';

class FakeAudio implements AudioLike {
  src = '';
  currentTime = 0;
  playbackRate = 1;
  paused = true;
  readyState = 4;
  playError: Error | null = null;
  async play() {
    if (this.playError) throw this.playError;
    this.paused = false;
  }
  pause() {
    this.paused = true;
  }
  addEventListener() {}
  removeEventListener() {}
}

const item = (over: Partial<JukeboxItem> = {}): JukeboxItem => ({
  id: '1',
  state: 'playing',
  playable: true,
  title: 'Song',
  duration_ms: 180_000,
  ...over,
});

function state(over: Partial<JukeboxState> = {}, nowMs = 1_000_000): JukeboxState {
  return {
    channel_id: 'c',
    active: true,
    mode: 'synced',
    version: 1,
    server_now_ms: nowMs,
    position_ms: 5_000,
    started_at_ms: nowMs - 5_000,
    paused: false,
    current: item(),
    queue: [],
    votes: 0,
    votes_needed: 1,
    my_vote: false,
    listeners: 1,
    i_am_dj: false,
    can_queue: true,
    can_control: false,
    ...over,
  };
}

function rig(first: JukeboxState, offset = 0) {
  let local = first.server_now_ms - offset;
  const audio = new FakeAudio();
  const ended = vi.fn(async (): Promise<JukeboxState> => first);
  const api = {
    listen: vi.fn(async () => first),
    state: vi.fn(async () => first),
    presign: vi.fn(async () => ({ url: 'https://bucket.test/a.flac' })),
    ended,
    leave: vi.fn(async () => first),
  } as unknown as JukeboxApi;
  const timers: (() => void)[] = [];
  const c = new JukeboxController({
    api,
    audio,
    now: () => local,
    setInterval: (fn) => timers.push(fn),
    clearInterval: () => undefined,
    subscribe: () => () => undefined,
  });
  return {
    c,
    audio,
    api,
    ended,
    advance(ms: number) {
      local += ms;
    },
    tick() {
      timers[0]?.();
    },
  };
}

describe('JukeboxController', () => {
  it('starts the presigned track at the scheduled position, using the server clock offset', async () => {
    // The local clock runs 30 s behind the server: the position must still come out right.
    const r = rig(state(), 30_000);
    await r.c.start('c');
    await vi.waitFor(() => expect(r.c.getSnapshot().status).toBe('playing'));
    expect(r.audio.src).toBe('https://bucket.test/a.flac');
    expect(r.audio.currentTime).toBeCloseTo(5, 1);
  });

  it('leaves small drift alone, nudges the rate for a moderate one and seeks for a large one', async () => {
    const r = rig(state());
    await r.c.start('c');
    await vi.waitFor(() => expect(r.c.getSnapshot().status).toBe('playing'));
    r.advance(2_000); // past the settle window; the schedule is at 7 s
    r.audio.currentTime = 7.05;
    r.tick();
    expect(r.audio.playbackRate).toBe(1);
    r.audio.currentTime = 7.5; // 500 ms ahead
    r.tick();
    expect(r.audio.playbackRate).toBeLessThan(1);
    r.audio.currentTime = 12; // 5 s ahead
    r.tick();
    expect(r.audio.currentTime).toBeCloseTo(7, 0);
    expect(r.audio.playbackRate).toBe(1);
  });

  it('stays silent and says so when the server refuses to sign the track (404)', async () => {
    const r = rig(state());
    (r.api.presign as ReturnType<typeof vi.fn>).mockRejectedValue(new ApiError(404, 'no', null));
    await r.c.start('c');
    await vi.waitFor(() => expect(r.c.getSnapshot().status).toBe('notShared'));
    expect(r.audio.src).toBe('');
    expect(r.audio.paused).toBe(true);
  });

  it('does not even ask for a signature when the state marks the track unplayable', async () => {
    const r = rig(state({ current: item({ playable: false }) }));
    await r.c.start('c');
    expect(r.c.getSnapshot().status).toBe('notShared');
    expect(r.api.presign).not.toHaveBeenCalled();
  });

  it('reports blocked when the browser refuses sound, then resumes on a gesture', async () => {
    const r = rig(state());
    r.audio.playError = new DOMException('no', 'NotAllowedError');
    await r.c.start('c');
    await vi.waitFor(() => expect(r.c.getSnapshot().status).toBe('blocked'));
    r.audio.playError = null;
    await r.c.resume();
    expect(r.c.getSnapshot().status).toBe('playing');
  });

  it('plays nothing for listeners in stream mode and pauses with the server', async () => {
    const r = rig(state({ mode: 'stream' }));
    await r.c.start('c');
    expect(r.c.getSnapshot().status).toBe('streamed');
    expect(r.api.presign).not.toHaveBeenCalled();
    const p = rig(state({ paused: true }));
    await p.c.start('c');
    expect(p.c.getSnapshot().status).toBe('paused');
  });

  it('asks the server once when the schedule says the track is over', async () => {
    const r = rig(state({ current: item({ duration_ms: 6_000 }) }));
    await r.c.start('c');
    await vi.waitFor(() => expect(r.c.getSnapshot().status).toBe('playing'));
    r.advance(TICK_MS + 3_000);
    r.tick();
    r.tick();
    await vi.waitFor(() => expect(r.ended).toHaveBeenCalledTimes(1));
    expect(r.ended).toHaveBeenCalledWith('c', '1');
  });
});
