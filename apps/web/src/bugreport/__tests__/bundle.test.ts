import { describe, expect, it } from 'vitest';
import { buildBundle, type BundleSources } from '../bundle';

const O = 'https://chat.example.org';
const NOW = 1_794_500_000_000;
const HEX64 = 'a'.repeat(64);

const good = (): BundleSources => ({
  commit: () => '1234abc',
  route: () => 'lounge.:guildId.:channelId',
  viewport: () => ({ w: 1280, h: 800, dpr: 2 }),
  caps: () => ({ webgpu: true, wasm: false, touch: false }),
  engine: () => 'chromium',
  locale: () => 'en',
  theme: () => 'lounge-dark',
  online: () => true,
  gateway: () => ({ status: 'connected' }),
  sw: () => 'active',
  voiceConnected: () => false,
  resources: () => [
    {
      name: `${O}/api/v1/channels/1794500000123456789/messages?x=1`,
      responseStatus: 500,
      duration: 312.4,
      startTime: 1000,
      initiatorType: 'fetch',
    },
    { name: `${O}/api/v1/ok`, responseStatus: 200, duration: 3, startTime: 1100 },
  ],
  perfNow: () => 15_000,
  origin: () => O,
  crashes: () => [HEX64],
});

describe('buildBundle', () => {
  it('has exactly the contract keys', () => {
    const b = buildBundle(good(), NOW);
    expect(Object.keys(b).sort()).toEqual(
      [
        'at',
        'caps',
        'commit',
        'crashes',
        'failed_requests',
        'gateway',
        'locale',
        'online',
        'route',
        'sw',
        'theme',
        'viewport',
        'voice',
      ].sort(),
    );
    expect(b.at).toBe(NOW);
    expect(b.theme).toBe('lounge-dark');
    expect(b.caps).toEqual({ engine: 'chromium', webgpu: true, wasm: false, touch: false });
    expect(b.gateway).toEqual({ status: 'connected' });
    expect(b.failed_requests).toEqual([
      { method: '?', path: '/api/v1/channels/:id/messages', status: 500, ms: 312, ago_s: 14 },
    ]);
    expect(b.crashes).toEqual([HEX64]);
  });

  it('keeps reconnects and heartbeat when they are numbers', () => {
    const gateway = () => ({ status: 'reconnecting', reconnects: 3, heartbeat_ms: 41 });
    expect(buildBundle({ ...good(), gateway }, NOW).gateway).toEqual({
      status: 'reconnecting',
      reconnects: 3,
      heartbeat_ms: 41,
    });
  });

  it('clamps reconnects and heartbeat to the server limits and leaves absent ones out', () => {
    const gw = (g: object) => buildBundle({ ...good(), gateway: () => g }, NOW).gateway;
    expect(gw({ status: 'connected', reconnects: 99_999, heartbeat_ms: 9_999_999 })).toEqual({
      status: 'connected',
      reconnects: 10_000,
      heartbeat_ms: 600_000,
    });
    expect(gw({ status: 'connected', reconnects: -4, heartbeat_ms: 40.6 })).toEqual({
      status: 'connected',
      reconnects: 0,
      heartbeat_ms: 41,
    });
    expect(gw({ status: 'connected', reconnects: 0 })).toEqual({
      status: 'connected',
      reconnects: 0,
    });
  });

  it('maps themes', () => {
    const th = (id: unknown) => buildBundle({ ...good(), theme: () => id }, NOW).theme;
    expect(th('lounge-editorial')).toBe('editorial');
    expect(th('lounge-light')).toBe('lounge-light');
    expect(th('lounge-onyx')).toBe('other');
    expect(th({})).toBe('other');
  });

  it('yields a clean bundle from junk', () => {
    const junk: BundleSources = {
      commit: () => 12345,
      route: () => 'lounge.123456789012345678',
      viewport: () => ({ w: '1280', h: Infinity, dpr: 'big', secret: 'SECRET' }),
      caps: () => ({ webgpu: 'yes', wasm: true, secret: true, token: 'SECRET' }),
      engine: () => 'Mozilla/5.0 SECRET',
      locale: () => 'x'.repeat(5000),
      theme: () => 'SECRET',
      online: () => 'true',
      gateway: () => ({
        status: 'SECRET',
        reconnects: 'many',
        heartbeat_ms: NaN,
        sessionId: 'SECRET',
      }),
      sw: () => 'SECRET',
      voiceConnected: () => 1,
      resources: () => [
        {
          name: `${O}/api/v1/x?token=SECRET`,
          responseStatus: 404,
          duration: 'slow',
          startTime: 'now',
        },
        { name: 'https://evil.example.net/api/v1/y', responseStatus: 500 },
        { name: `${O}/api/v1/z`, responseStatus: '500' },
        null,
        'SECRET',
      ],
      perfNow: () => 'later',
      origin: () => O,
      crashes: () => ['SECRET', HEX64.toUpperCase(), 7, HEX64],
    };
    const b = buildBundle(junk, NOW);
    expect(JSON.stringify(b)).not.toContain('SECRET');
    expect(b.commit).toBe('0000000');
    expect(b.route).toBe('unknown');
    expect(b.viewport).toEqual({ w: 0, h: 0, dpr: 1 });
    expect(b.caps).toEqual({ engine: 'other', wasm: true });
    expect(b.locale).toBe('und');
    expect(b.online).toBe(false);
    expect(b.gateway).toEqual({ status: 'closed' });
    expect(b.sw).toBe('none');
    expect(b.voice.connected).toBe(false);
    expect(b.failed_requests).toEqual([
      { method: '?', path: '/api/v1/x', status: 404, ms: 0, ago_s: 0 },
    ]);
    expect(b.crashes).toEqual([HEX64]);
  });

  it('survives a throwing reader, keeps the newest 20 failures and the last 5 crashes', () => {
    const many = Array.from({ length: 30 }, (_, i) => ({
      name: `${O}/api/v1/r${i}`,
      responseStatus: 500,
      duration: 1,
      startTime: i * 10,
    }));
    const s: BundleSources = {
      ...good(),
      route: () => {
        throw new Error('SECRET');
      },
      resources: () => many,
      crashes: () => Array.from({ length: 8 }, (_, i) => String(i).repeat(64)),
    };
    const b = buildBundle(s, NOW);
    expect(b.route).toBe('unknown');
    expect(b.failed_requests).toHaveLength(20);
    expect(b.failed_requests[0]!.path).toBe('/api/v1/r29');
    expect(b.crashes).toHaveLength(5);
    expect(b.crashes[4]).toBe('7'.repeat(64));
  });
});
