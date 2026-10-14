import { vi } from 'vitest';
import { LEVELS } from './fakes';

type Handler = (event: string, data: unknown) => void;

// A recording stand-in for hls.js: the real library needs MSE, which jsdom does not have.
const hlsInstances: FakeHls[] = [];
class FakeHls {
  config: Record<string, unknown>;
  handlers = new Map<string, Handler[]>();
  autoLevelCapping = -1;
  nextLevel = -2;
  currentLevel = -2;
  audioTrack = 0;
  audioTracks = [
    { name: 'English', lang: 'en' },
    { name: 'Japanese', lang: 'ja' },
  ];
  loaded: string[] = [];
  attached: unknown = null;
  destroyed = false;
  recovered = 0;
  restarted = 0;
  constructor(config: Record<string, unknown>) {
    this.config = config;
    hlsInstances.push(this);
  }
  on(e: string, h: Handler) {
    this.handlers.set(e, [...(this.handlers.get(e) ?? []), h]);
  }
  emit(e: string, data: unknown) {
    for (const h of this.handlers.get(e) ?? []) h(e, data);
  }
  attachMedia(v: unknown) {
    this.attached = v;
  }
  loadSource(u: string) {
    this.loaded.push(u);
  }
  recoverMediaError() {
    this.recovered++;
  }
  startLoad() {
    this.restarted++;
  }
  destroy() {
    this.destroyed = true;
  }
}
vi.mock('hls.js', () => ({
  default: FakeHls,
  Events: {
    MANIFEST_PARSED: 'manifest',
    LEVEL_SWITCHED: 'level',
    AUDIO_TRACKS_UPDATED: 'audioTracks',
    AUDIO_TRACK_SWITCHED: 'audioSwitched',
    ERROR: 'error',
  },
  ErrorTypes: { NETWORK_ERROR: 'networkError', MEDIA_ERROR: 'mediaError' },
}));

import {
  DATA_SAVER_MAX_HEIGHT,
  capLevelIndex,
  createEngine,
  createHlsEngine,
  createNativeEngine,
  type EngineEvent,
} from '../engine';

beforeAll(() => {
  HTMLMediaElement.prototype.load = () => undefined; // jsdom has no media pipeline
});
const video = () => document.createElement('video');

describe('capLevelIndex (data saver)', () => {
  it('picks the best rung at or below the cap', () => {
    expect(DATA_SAVER_MAX_HEIGHT).toBe(480);
    expect(capLevelIndex(LEVELS, 480)).toBe(0);
    expect(capLevelIndex(LEVELS, 720)).toBe(1);
    expect(capLevelIndex(LEVELS, null)).toBe(-1);
    expect(capLevelIndex([], 480)).toBe(-1);
  });
  it('falls back to the lowest rung when every rung is above the cap', () => {
    expect(capLevelIndex([LEVELS[2]!, LEVELS[1]!], 360)).toBe(1);
  });
});

describe('engine choice', () => {
  it('uses the browser HLS where it exists (Safari, iOS)', async () => {
    const v = video();
    v.canPlayType = () => 'maybe';
    expect((await createEngine(v)).kind).toBe('native');
  });
  it('uses hls.js elsewhere and loads it lazily', async () => {
    const v = video();
    v.canPlayType = () => '';
    const e = await createEngine(v);
    expect(e.kind).toBe('hls.js');
    expect(hlsInstances.at(-1)!.attached).toBe(v);
  });
});

describe('hls.js engine (CIN-PLY-001)', () => {
  async function make() {
    const v = video();
    const e = await createHlsEngine(v);
    const hls = hlsInstances.at(-1)!;
    const events: EngineEvent[] = [];
    e.subscribe((x) => events.push(x));
    return { e, hls, events, v };
  }

  it('reports the ladder and the playing level', async () => {
    const { hls, events } = await make();
    hls.emit('manifest', {
      levels: [
        { height: 480, bitrate: 1_500_000 },
        { height: 1080, bitrate: 6_000_000 },
      ],
    });
    hls.emit('level', { level: 1 });
    expect(events).toEqual([
      {
        type: 'levels',
        levels: [
          { index: 0, height: 480, bitrate: 1_500_000 },
          { index: 1, height: 1080, bitrate: 6_000_000 },
        ],
      },
      { type: 'level', index: 1 },
    ]);
  });

  it('starts from a position and switches quality without flushing', async () => {
    const { e, hls } = await make();
    e.load('https://cdn.test/m.m3u8', 90);
    expect(hls.config.startPosition).toBe(90);
    expect(hls.loaded).toEqual(['https://cdn.test/m.m3u8']);
    hls.emit('manifest', { levels: LEVELS.map((l) => ({ height: l.height, bitrate: l.bitrate })) });
    e.setQuality(1);
    expect(hls.nextLevel).toBe(1);
    e.setQuality('auto');
    expect(hls.currentLevel).toBe(-1);
  });

  it('caps ABR for the data saver but lets a manual choice win', async () => {
    const { e, hls } = await make();
    hls.emit('manifest', { levels: LEVELS.map((l) => ({ height: l.height, bitrate: l.bitrate })) });
    e.setMaxHeight(480);
    expect(hls.autoLevelCapping).toBe(0);
    e.setQuality(2);
    expect(hls.autoLevelCapping).toBe(-1);
    e.setQuality('auto');
    expect(hls.autoLevelCapping).toBe(0);
    e.setMaxHeight(null);
    expect(hls.autoLevelCapping).toBe(-1);
  });

  it('lists and switches audio tracks', async () => {
    const { e, hls, events } = await make();
    hls.emit('audioTracks', {});
    expect(events.at(-1)).toEqual({
      type: 'audio',
      tracks: [
        { id: '0', label: 'English', language: 'en' },
        { id: '1', label: 'Japanese', language: 'ja' },
      ],
      active: '0',
    });
    e.setAudioTrack('1');
    expect(hls.audioTrack).toBe(1);
  });

  it('re-signs later requests after a refresh, and only then', async () => {
    const { e, hls } = await make();
    const xhr = { open: vi.fn() };
    const setup = hls.config.xhrSetup as (x: typeof xhr, url: string) => void;
    setup(xhr, 'https://cdn.test/m1/seg1.m4s?sig=old');
    expect(xhr.open).not.toHaveBeenCalled(); // hls.js opens the request itself until a refresh happened
    e.updateSigning('https://cdn.test/m1/master.m3u8?sig=new&exp=9');
    setup(xhr, 'https://cdn.test/m1/seg2.m4s?sig=old');
    expect(xhr.open).toHaveBeenCalledWith(
      'GET',
      'https://cdn.test/m1/seg2.m4s?sig=new&exp=9',
      true,
    );
    const fetchSetup = hls.config.fetchSetup as (c: { url: string }, i?: RequestInit) => Request;
    expect(fetchSetup({ url: 'https://cdn.test/m1/seg3.m4s?sig=old' }).url).toBe(
      'https://cdn.test/m1/seg3.m4s?sig=new&exp=9',
    );
  });

  it('turns a refused request into an auth error and recovers other failures', async () => {
    const { hls, events } = await make();
    hls.emit('error', { type: 'networkError', fatal: false, response: { code: 403 } });
    expect(events.at(-1)).toEqual({ type: 'error', fatal: false, kind: 'auth', status: 403 });
    hls.emit('error', { type: 'mediaError', fatal: true });
    expect(hls.recovered).toBe(1);
    hls.emit('error', { type: 'networkError', fatal: true, response: { code: 500 } });
    expect(hls.restarted).toBe(1);
    expect(events.at(-1)).toMatchObject({ kind: 'network' });
  });

  it('swaps the source at a position and cleans up on destroy', async () => {
    const { e, hls, events } = await make();
    e.swapSource('https://cdn.test/m.m3u8?sig=z', 400);
    expect(hls.config.startPosition).toBe(400);
    expect(hls.loaded.at(-1)).toBe('https://cdn.test/m.m3u8?sig=z');
    e.destroy();
    expect(hls.destroyed).toBe(true);
    hls.emit('level', { level: 2 });
    expect(events.filter((x) => x.type === 'level')).toHaveLength(0);
  });
});

describe('native engine', () => {
  it('plays the playlist itself, resuming at a position', () => {
    const v = video();
    const e = createNativeEngine(v);
    e.load('https://cdn.test/m.m3u8', 30);
    expect(v.getAttribute('src')).toBe('https://cdn.test/m.m3u8');
    e.setQuality(1); // no quality control on native HLS: a no-op
    e.destroy();
  });

  it('reads and switches audio tracks when the browser exposes them', () => {
    const v = video();
    const list = [
      { id: 'a', label: 'English', language: 'en', enabled: true },
      { id: 'b', label: 'Japanese', language: 'ja', enabled: false },
    ];
    Object.defineProperty(v, 'audioTracks', { value: Object.assign(list, {}) });
    const e = createNativeEngine(v);
    const events: EngineEvent[] = [];
    e.subscribe((x) => events.push(x));
    v.dispatchEvent(new Event('loadedmetadata'));
    expect(events.at(-1)).toMatchObject({ type: 'audio', active: '0' });
    e.setAudioTrack('1');
    expect(list[1]!.enabled).toBe(true);
    expect(list[0]!.enabled).toBe(false);
  });

  it('treats a network error as a lapsed signature', () => {
    const v = video();
    const e = createNativeEngine(v);
    const events: EngineEvent[] = [];
    e.subscribe((x) => events.push(x));
    Object.defineProperty(v, 'error', { value: { code: 2 }, configurable: true });
    v.dispatchEvent(new Event('error'));
    expect(events.at(-1)).toMatchObject({ type: 'error', kind: 'auth' });
  });
});
