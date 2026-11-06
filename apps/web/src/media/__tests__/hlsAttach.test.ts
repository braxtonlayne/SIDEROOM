// D-518: hls.js is attached only for a listed host, with credentials omitted, and disposed cleanly.
import { vi } from 'vitest';

const h = vi.hoisted(() => {
  const instances: { config: Record<string, unknown>; loaded: string[]; destroyed: boolean }[] = [];
  let supported = true;
  class FakeHls {
    static isSupported() {
      return supported;
    }
    config: Record<string, unknown>;
    loaded: string[] = [];
    destroyed = false;
    constructor(config: Record<string, unknown>) {
      this.config = config;
      instances.push(this);
    }
    on() {}
    attachMedia() {}
    loadSource(u: string) {
      this.loaded.push(u);
    }
    destroy() {
      this.destroyed = true;
    }
  }
  return {
    instances,
    FakeHls,
    setSupported: (v: boolean) => {
      supported = v;
    },
  };
});

vi.mock('hls.js', () => ({
  default: h.FakeHls,
  Events: {
    MANIFEST_PARSED: 'manifest',
    LEVEL_SWITCHED: 'level',
    AUDIO_TRACKS_UPDATED: 'audioTracks',
    AUDIO_TRACK_SWITCHED: 'audioSwitched',
    ERROR: 'error',
  },
  ErrorTypes: { NETWORK_ERROR: 'networkError', MEDIA_ERROR: 'mediaError' },
}));

import { HlsUnsupportedError, attachHls, canPlayListedHls, listedMediaHosts } from '../hlsAttach';

const HOSTS = ['tube.example.org', 'cdn.example.net'];
const LISTED = 'https://cdn.example.net/hls/a/master.m3u8';

afterEach(() => {
  h.instances.length = 0;
  h.setSupported(true);
});

describe('canPlayListedHls', () => {
  it('accepts an https playlist on a listed host only', () => {
    expect(canPlayListedHls(LISTED, HOSTS)).toBe(true);
    expect(canPlayListedHls('https://TUBE.example.org:443/hls/x.m3u8', HOSTS)).toBe(true);
    for (const url of [
      'https://elsewhere.example/hls/a.m3u8',
      'http://cdn.example.net/hls/a.m3u8',
      'https://cdn.example.net:8443/hls/a.m3u8',
      'https://u:p@cdn.example.net/hls/a.m3u8',
      'https://cdn.example.net.evil.example/a.m3u8',
      'not a url',
      '',
      null,
      undefined,
    ]) {
      expect(canPlayListedHls(url, HOSTS)).toBe(false);
    }
    expect(canPlayListedHls(LISTED, [])).toBe(false);
  });

  it('lists nothing when the build names no host', () => {
    // The test build is made without SR_PEERTUBE_HOSTS / SR_MEDIA_HOSTS.
    expect(listedMediaHosts()).toEqual([]);
    expect(canPlayListedHls(LISTED)).toBe(false);
  });
});

describe('attachHls', () => {
  it('refuses an unlisted host before loading anything', async () => {
    const v = document.createElement('video');
    await expect(
      attachHls(v, 'https://elsewhere.example/a.m3u8', { hosts: HOSTS }),
    ).rejects.toThrow();
    expect(h.instances).toHaveLength(0);
  });

  it('loads a listed playlist with credentials omitted and disposes the engine', async () => {
    const v = document.createElement('video');
    const dispose = await attachHls(v, LISTED, { hosts: HOSTS });
    expect(h.instances).toHaveLength(1);
    const hls = h.instances[0]!;
    expect(hls.loaded).toEqual([LISTED]);
    // Both loader hooks drop credentials.
    const fetchSetup = hls.config.fetchSetup as (c: { url: string }, i: RequestInit) => Request;
    expect(fetchSetup({ url: LISTED }, { method: 'GET' }).credentials).toBe('omit');
    const xhrSetup = hls.config.xhrSetup as (x: XMLHttpRequest, u: string) => void;
    const xhr = { withCredentials: true, open: vi.fn() } as unknown as XMLHttpRequest;
    xhrSetup(xhr, LISTED);
    expect(xhr.withCredentials).toBe(false);
    dispose();
    expect(hls.destroyed).toBe(true);
  });

  it('rejects where hls.js cannot run (no MSE)', async () => {
    h.setSupported(false);
    const v = document.createElement('video');
    await expect(attachHls(v, LISTED, { hosts: HOSTS })).rejects.toBeInstanceOf(
      HlsUnsupportedError,
    );
    expect(h.instances).toHaveLength(0);
  });
});
