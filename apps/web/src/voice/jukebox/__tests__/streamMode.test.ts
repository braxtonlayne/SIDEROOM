// MUS-JB-001 + MUS-JB-002 (wave-31 integration): the DJ's element feeds the call's music track in stream mode.
import { describe, expect, it, vi } from 'vitest';
import { JukeboxController, type AudioLike, type MusicSink } from '../controller';
import { browserMusicSink } from '../musicSink';
import type { JukeboxApi, JukeboxState } from '../types';

class FakeAudio implements AudioLike {
  src = '';
  crossOrigin: string | null = null;
  currentTime = 0;
  playbackRate = 1;
  paused = true;
  readyState = 4;
  async play() {
    this.paused = false;
  }
  pause() {
    this.paused = true;
  }
  addEventListener() {}
  removeEventListener() {}
}

function state(over: Partial<JukeboxState> = {}): JukeboxState {
  const now = 1_000_000;
  return {
    channel_id: 'c',
    active: true,
    mode: 'stream',
    version: 1,
    server_now_ms: now,
    position_ms: 0,
    started_at_ms: now - 1_000,
    paused: false,
    current: { id: '1', state: 'playing', playable: true, title: 'Song', duration_ms: 180_000 },
    queue: [],
    votes: 0,
    votes_needed: 1,
    my_vote: false,
    listeners: 2,
    i_am_dj: true,
    can_queue: true,
    can_control: true,
    ...over,
  };
}

function rig(first: JukeboxState) {
  let next = first;
  const audio = new FakeAudio();
  const sink: MusicSink = { start: vi.fn(), stop: vi.fn() };
  const api = {
    listen: vi.fn(async () => next),
    state: vi.fn(async () => next),
    presign: vi.fn(async () => ({ url: 'https://bucket.test/a.flac' })),
    ended: vi.fn(async () => next),
    leave: vi.fn(async () => next),
  } as unknown as JukeboxApi;
  const c = new JukeboxController({
    api,
    audio,
    now: () => first.server_now_ms,
    setInterval: () => 0,
    clearInterval: () => undefined,
    subscribe: () => () => undefined,
    music: sink,
  });
  return { c, audio, sink, setNext: (s: JukeboxState) => (next = s) };
}

describe('jukebox stream mode on the DJ side', () => {
  it('publishes the DJ track, keeps the DJ hearing it and asks the bucket for CORS', async () => {
    const r = rig(state());
    await r.c.start('c');
    await vi.waitFor(() => expect(r.c.getSnapshot().status).toBe('playing'));
    expect(r.sink.start).toHaveBeenCalledTimes(1);
    expect(r.c.streamingMusic).toBe(true);
    expect(r.audio.crossOrigin).toBe('anonymous');
  });

  it('unpublishes on a hand-over, a switch to synced and on leaving', async () => {
    const r = rig(state());
    await r.c.start('c');
    r.setNext(state({ i_am_dj: false, version: 2 }));
    await r.c.refresh();
    expect(r.sink.stop).toHaveBeenCalledTimes(1);
    expect(r.c.getSnapshot().status).toBe('streamed');

    const s = rig(state());
    await s.c.start('c');
    s.setNext(state({ mode: 'synced', version: 2 }));
    await s.c.refresh();
    expect(s.sink.stop).toHaveBeenCalledTimes(1);

    const l = rig(state());
    await l.c.start('c');
    await l.c.leave();
    expect(l.sink.stop).toHaveBeenCalledTimes(1);
  });

  it('never publishes for a listener or an unreadable track', async () => {
    const r = rig(state({ i_am_dj: false }));
    await r.c.start('c');
    const u = rig(
      state({
        current: { id: '1', state: 'playing', playable: false, title: '', duration_ms: 1 },
      }),
    );
    await u.c.start('c');
    expect(r.sink.start).not.toHaveBeenCalled();
    expect(u.sink.start).not.toHaveBeenCalled();
  });
});

describe('browserMusicSink', () => {
  function fakeCtx() {
    const source = { connect: vi.fn(), disconnect: vi.fn() };
    const stream = { id: 'tap' } as unknown as MediaStream;
    const ctx = {
      destination: { id: 'speakers' },
      createMediaElementSource: vi.fn(() => source),
      createMediaStreamDestination: vi.fn(() => ({ stream })),
      resume: vi.fn(async () => undefined),
      close: vi.fn(async () => undefined),
    };
    return { ctx, source, stream };
  }

  it('routes the element once, publishes the tap and detaches it on stop', () => {
    const f = fakeCtx();
    const pub = { publishMusic: vi.fn(), unpublishMusic: vi.fn() };
    const audio = {} as HTMLAudioElement;
    const sink = browserMusicSink(audio, pub, () => f.ctx as unknown as AudioContext);
    sink.start();
    expect(f.ctx.createMediaElementSource).toHaveBeenCalledWith(audio);
    expect(f.source.connect).toHaveBeenCalledWith(f.ctx.destination);
    expect(pub.publishMusic).toHaveBeenCalledWith(f.stream);
    sink.stop();
    expect(pub.unpublishMusic).toHaveBeenCalledTimes(1);
    expect(f.source.disconnect).toHaveBeenCalledTimes(1);
    // The next stay reuses the same source (an element can be routed only once).
    sink.start();
    expect(f.ctx.createMediaElementSource).toHaveBeenCalledTimes(1);
    expect(pub.publishMusic).toHaveBeenCalledTimes(2);
    sink.dispose();
    expect(pub.unpublishMusic).toHaveBeenCalledTimes(2);
    expect(f.ctx.close).toHaveBeenCalled();
  });

  it('does nothing without Web Audio', () => {
    const pub = { publishMusic: vi.fn(), unpublishMusic: vi.fn() };
    const sink = browserMusicSink({} as HTMLAudioElement, pub, () => null);
    sink.start();
    sink.stop();
    expect(pub.publishMusic).not.toHaveBeenCalled();
    expect(pub.unpublishMusic).not.toHaveBeenCalled();
  });
});
