import { renderHook, waitFor, act } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useNotifSettings } from '../shell/notifSettings';
import { useLoadNotifSettings } from '../shell/useGuildMenu';

/** RDY-SCALE-002: settings load with a constant number of requests, however many servers. */
const ids = (n: number) => Array.from({ length: n }, (_, i) => String(1000 + i));

let calls: string[] = [];
function stubFetch(opts: { fail?: boolean; limit?: number } = {}) {
  const limit = opts.limit ?? Infinity;
  vi.stubGlobal(
    'fetch',
    vi.fn((url: string) => {
      calls.push(String(url));
      if (opts.fail || calls.length > limit)
        return Promise.resolve(
          new Response(JSON.stringify({ message: 'x' }), {
            status: opts.fail ? 500 : 429,
            headers: { 'retry-after': '1' },
          }),
        );
      const body = String(url).endsWith('/users/@me/guild-settings')
        ? {
            settings: [
              {
                guild_id: '1003',
                muted: true,
                mute_config: null,
                message_notifications: 1,
                channel_overrides: [{ channel_id: '9', muted: true }],
              },
            ],
          }
        : { muted: false };
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
    }),
  );
}

describe('server settings batch (RDY-SCALE-002)', () => {
  beforeEach(() => {
    calls = [];
    useNotifSettings.getState().reset();
  });
  afterEach(() => vi.unstubAllGlobals());

  it('a session in 100 servers sends one settings request at load', async () => {
    stubFetch();
    renderHook(() => useLoadNotifSettings(ids(100)));
    await waitFor(() => expect(Object.keys(useNotifSettings.getState().byGuild)).toHaveLength(100));
    const settings = calls.filter((u) => u.includes('settings'));
    expect(settings).toEqual(['/api/v1/users/@me/guild-settings']);
    const s = useNotifSettings.getState().byGuild;
    expect(s['1003']?.muted).toBe(true);
    expect(s['1003']?.channels['9']?.muted).toBe(true);
    expect(s['1004']?.muted).toBe(false);
  });

  it('60 servers stay under a 429 limit that one request per server would trip', async () => {
    stubFetch({ limit: 50 });
    renderHook(() => useLoadNotifSettings(ids(60)));
    await waitFor(() => expect(Object.keys(useNotifSettings.getState().byGuild)).toHaveLength(60));
    expect(calls.length).toBe(1);
  });

  it('a server joined later is fetched alone, the rest are not asked again', async () => {
    stubFetch();
    const { rerender } = renderHook(({ g }) => useLoadNotifSettings(g), {
      initialProps: { g: ids(10) },
    });
    await waitFor(() => expect(Object.keys(useNotifSettings.getState().byGuild)).toHaveLength(10));
    await act(async () => rerender({ g: [...ids(10), '5555'] }));
    await waitFor(() => expect(useNotifSettings.getState().byGuild['5555']).toBeDefined());
    expect(calls).toEqual([
      '/api/v1/users/@me/guild-settings',
      '/api/v1/users/@me/guilds/5555/settings',
    ]);
  });

  it('a failed batch does not fall back to a request per server', async () => {
    stubFetch({ fail: true });
    renderHook(() => useLoadNotifSettings(ids(100)));
    await waitFor(() => expect(calls.length).toBeGreaterThan(0));
    await act(async () => {
      await new Promise((r) => setTimeout(r, 30));
    });
    expect(calls).toHaveLength(1);
    expect(useNotifSettings.getState().byGuild).toEqual({});
  });
});
