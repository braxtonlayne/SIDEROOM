// K-22: a session for an account in about 60 servers. Profiles the reducer path of one READY followed by
// one GUILD_CREATE per server (what the gateway sends), so a change in the hot path shows up here. The
// asserts are generous time budgets and structural facts (how many store updates a READY costs); the printed
// timings are the profile.
import { performance } from 'node:perf_hooks';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { http, HttpResponse, server } from '../test/server';
import { BURST_MS, receiveDispatch } from './connect';
import { emptyData, reduceDispatch, type StoreData } from './reducer';
import { resetStore } from './index';
import { PEOPLE_CHUNK, useStore } from './store';

const GUILDS = 60;
const CHANNELS = 25;
const ROLES = 12;
const MEMBERS = 40;

const user = (n: number) => ({ id: String(1_000_000 + n), username: `u${n}`, discriminator: '0' });

function guildCreate(g: number) {
  const id = String(5_000 + g);
  return {
    id,
    name: `Guild ${g}`,
    owner_id: '1',
    channels: Array.from({ length: CHANNELS }, (_, c) => ({
      id: String(100_000 + g * 100 + c),
      guild_id: id,
      type: c % 5 === 0 ? 2 : 0,
      name: `channel-${c}`,
      position: c,
    })),
    roles: Array.from({ length: ROLES }, (_, r) => ({
      id: String(200_000 + g * 100 + r),
      guild_id: id,
      name: `role-${r}`,
      position: r,
      permissions: '0',
    })),
    members: Array.from({ length: MEMBERS }, (_, m) => ({
      user: user(g * MEMBERS + m),
      roles: [],
      nick: null,
      joined_at: '2026-01-01T00:00:00Z',
    })),
    presences: Array.from({ length: MEMBERS }, (_, m) => ({
      user: user(g * MEMBERS + m),
      status: 'online',
      activities: [],
    })),
    voice_states: [],
  };
}

const ready = () => ({
  user: { id: '1', username: 'owner', discriminator: '0' },
  guilds: Array.from({ length: GUILDS }, (_, g) => ({ id: String(5_000 + g), unavailable: true })),
  private_channels: [],
  read_state: { entries: [] },
  presences: [],
  session_id: 's',
});

function profile(label: string, run: () => void): number {
  const t0 = performance.now();
  run();
  const ms = performance.now() - t0;
  console.info(`[K-22] ${label}: ${ms.toFixed(1)} ms`);
  return ms;
}

describe('READY and GUILD_CREATE for an account in 60 servers (K-22)', () => {
  beforeEach(() => resetStore());

  it('reduces READY plus 60 GUILD_CREATEs quickly', () => {
    let s: StoreData = emptyData();
    const creates = Array.from({ length: GUILDS }, (_, g) => guildCreate(g));
    const total = profile('reducer: READY + 60 GUILD_CREATE one by one', () => {
      s = { ...s, ...reduceDispatch(s, 'READY', ready()) };
      for (const c of creates) s = { ...s, ...reduceDispatch(s, 'GUILD_CREATE', c) };
    });
    expect(Object.keys(s.guilds)).toHaveLength(GUILDS);
    expect(Object.keys(s.channels)).toHaveLength(GUILDS * CHANNELS);
    expect(Object.keys(s.members)).toHaveLength(GUILDS * MEMBERS);
    expect(total).toBeLessThan(2_000);
  });

  it('a whole READY burst is one store update, not 61', () => {
    let updates = 0;
    const off = useStore.subscribe(() => {
      updates += 1;
    });
    const burst = [
      ['READY', ready()] as const,
      ...Array.from({ length: GUILDS }, (_, g) => ['GUILD_CREATE', guildCreate(g)] as const),
    ];
    const ms = profile('store: READY + 60 GUILD_CREATE as one batch', () => {
      useStore.getState().applyDispatches(burst.map(([event, data]) => ({ event, data })));
    });
    off();
    expect(updates).toBe(1);
    expect(Object.keys(useStore.getState().guilds)).toHaveLength(GUILDS);
    expect(ms).toBeLessThan(2_000);
  });

  it('a burst can index members and presences a moment after the servers appear', async () => {
    vi.useFakeTimers();
    try {
      const burst = [
        ['READY', ready()] as const,
        ...Array.from({ length: GUILDS }, (_, g) => ['GUILD_CREATE', guildCreate(g)] as const),
      ].map(([event, data]) => ({ event, data }));
      useStore.getState().applyDispatches(burst, { deferPeople: true });
      // First paint: every server, channel and role is there; nobody is indexed yet.
      let st = useStore.getState();
      expect(Object.keys(st.guilds)).toHaveLength(GUILDS);
      expect(Object.keys(st.channels)).toHaveLength(GUILDS * CHANNELS);
      expect(Object.keys(st.roles)).toHaveLength(GUILDS * ROLES);
      expect(Object.keys(st.members)).toHaveLength(0);
      // Then a few servers per tick, and the whole thing ends complete.
      await vi.advanceTimersByTimeAsync(0);
      st = useStore.getState();
      expect(Object.keys(st.memberIdsByGuild).length).toBe(PEOPLE_CHUNK);
      await vi.runAllTimersAsync();
      st = useStore.getState();
      expect(Object.keys(st.members)).toHaveLength(GUILDS * MEMBERS);
      expect(Object.keys(st.presences)).toHaveLength(GUILDS * MEMBERS);
      expect(Object.keys(st.memberIdsByGuild)).toHaveLength(GUILDS);
    } finally {
      vi.useRealTimers();
    }
  });

  it('a server left while its people wait is not brought back', async () => {
    vi.useFakeTimers();
    try {
      useStore.getState().applyDispatches(
        [
          { event: 'READY', data: ready() },
          { event: 'GUILD_CREATE', data: guildCreate(0) },
          { event: 'GUILD_CREATE', data: guildCreate(1) },
        ],
        { deferPeople: true },
      );
      useStore.getState().applyDispatch('GUILD_DELETE', { id: String(5_000) });
      await vi.runAllTimersAsync();
      const st = useStore.getState();
      expect(st.memberIdsByGuild['5000']).toBeUndefined();
      expect(st.memberIdsByGuild['5001']).toHaveLength(MEMBERS);
    } finally {
      vi.useRealTimers();
    }
  });

  it('READY keeps the cached message windows, marked stale, and a fetch replaces them', () => {
    let s: StoreData = emptyData();
    s = { ...s, ...reduceDispatch(s, 'READY', ready()) };
    s = { ...s, ...reduceDispatch(s, 'GUILD_CREATE', guildCreate(0)) };
    const channel = String(100_000);
    s = {
      ...s,
      windows: {
        [channel]: {
          ids: ['9001', '9002'],
          pending: [],
          hasMoreBefore: false,
          atLatest: true,
          loading: false,
          error: null,
        },
        // A window that holds nothing is not worth keeping.
        [String(100_001)]: {
          ids: [],
          pending: [],
          hasMoreBefore: false,
          atLatest: true,
          loading: false,
          error: null,
        },
      },
      messages: {
        '9001': { id: '9001', channel_id: channel, content: 'a' } as never,
        '9002': { id: '9002', channel_id: channel, content: 'b' } as never,
        '9999': { id: '9999', channel_id: String(100_001), content: 'orphan' } as never,
      },
    };
    const fresh = { ...s, ...reduceDispatch(s, 'READY', ready()) };
    expect(fresh.windows[channel]).toMatchObject({ ids: ['9001', '9002'], stale: true });
    expect(fresh.messages['9001']).toBeDefined();
    expect(fresh.windows[String(100_001)]).toBeUndefined();
    expect(fresh.messages['9999']).toBeUndefined();
  });

  it('a stale window is shown as it is and replaced by the newest page', async () => {
    const channel = '77';
    useStore.setState({
      windows: {
        [channel]: {
          ids: ['1', '2', '3'],
          pending: [],
          hasMoreBefore: true,
          atLatest: true,
          loading: false,
          error: null,
          stale: true,
        },
      },
      messages: {
        '1': { id: '1', channel_id: channel, content: 'old' } as never,
        '2': { id: '2', channel_id: channel, content: 'deleted meanwhile' } as never,
        '3': { id: '3', channel_id: channel, content: 'kept' } as never,
      },
    });
    server.use(
      http.get('*/api/v1/channels/77/messages', () =>
        HttpResponse.json([
          { id: '4', channel_id: channel, content: 'new', author: { id: '9', username: 'z' } },
          { id: '3', channel_id: channel, content: 'kept', author: { id: '9', username: 'z' } },
        ]),
      ),
    );
    await useStore.getState().loadInitialMessages(channel);
    const st = useStore.getState();
    expect(st.windows[channel]!.ids).toEqual(['3', '4']);
    expect(st.windows[channel]!.stale).toBe(false);
    expect(st.messages['2']).toBeUndefined();
    // A fresh window is not fetched again.
    let calls = 0;
    server.use(
      http.get('*/api/v1/channels/77/messages', () => {
        calls += 1;
        return HttpResponse.json([]);
      }),
    );
    await useStore.getState().loadInitialMessages(channel);
    expect(calls).toBe(0);
  });
});

describe('the gateway burst (K-22)', () => {
  it('collects READY and the GUILD_CREATEs after it into one update, in order', async () => {
    vi.useFakeTimers();
    try {
      const seen: number[] = [];
      const off = useStore.subscribe((st) => {
        seen.push(st.guilds ? Object.values(st.guilds).filter((g) => !g.unavailable).length : 0);
      });
      receiveDispatch('READY', ready());
      for (let g = 0; g < 5; g++) receiveDispatch('GUILD_CREATE', guildCreate(g));
      expect(seen).toEqual([]);
      await vi.advanceTimersByTimeAsync(BURST_MS + 1);
      // The first update already holds all five servers; the rest are the member indexing steps.
      expect(seen[0]).toBe(5);
      expect(useStore.getState().guildOrder).toHaveLength(GUILDS);
      // Anything else flushes what waits first, so the server's order is kept.
      receiveDispatch('GUILD_CREATE', guildCreate(7));
      receiveDispatch('CHANNEL_DELETE', { id: String(100_000 + 7 * 100), guild_id: '5007' });
      expect(useStore.getState().channels[String(100_000 + 7 * 100)]).toBeUndefined();
      expect(useStore.getState().guilds['5007']?.unavailable).toBe(false);
      off();
    } finally {
      vi.useRealTimers();
    }
  });

  it('takes the PRESENCE_UPDATEs after READY in the same update, and the server lists never undo them', async () => {
    resetStore();
    vi.useFakeTimers();
    try {
      const firsts: { servers: number; me?: string; five?: string }[] = [];
      const off = useStore.subscribe((st) =>
        firsts.push({
          servers: Object.values(st.guilds).filter((g) => !g.unavailable).length,
          me: st.presences['1']?.status,
          five: st.presences[user(5).id]?.status,
        }),
      );
      receiveDispatch('READY', ready());
      for (let g = 0; g < 3; g++) receiveDispatch('GUILD_CREATE', guildCreate(g));
      // The person's own presence, once per server, as the gateway sends it after READY; and a live
      // change for someone whose connect-time presence (online) is still in a deferred list.
      for (let g = 0; g < 3; g++)
        receiveDispatch('PRESENCE_UPDATE', {
          user: { id: '1' },
          guild_id: String(5_000 + g),
          status: 'online',
        });
      receiveDispatch('PRESENCE_UPDATE', { user: user(5), guild_id: '5000', status: 'dnd' });
      await vi.advanceTimersByTimeAsync(BURST_MS + 1);
      // One update carries READY, the servers and every PRESENCE_UPDATE; the later ones index people.
      expect(firsts[0]).toEqual({ servers: 3, me: 'online', five: 'dnd' });
      await vi.runAllTimersAsync();
      const p = useStore.getState().presences;
      expect(p['1']?.status).toBe('online');
      expect(p[user(5).id]?.status).toBe('dnd');
      // People of the deferred lists without a live update are still indexed.
      expect(p[user(6).id]?.status).toBe('online');
      off();
    } finally {
      vi.useRealTimers();
    }
  });
});
