// D-646 (SEC-AUD-169): the web side of the gateway resync. The server re-sends state after it lost events; the
// store marks what it cannot be sure of stale and nothing is shown to the member.
import { act } from '@testing-library/react';
import { http, HttpResponse, server } from '../test/server';
import { emptyWindow } from './reducer';
import { resetStore, useStore } from './index';

const me = { id: '1', username: 'me' };
const bob = { id: '2', username: 'bob' };
const cat = { id: '3', username: 'cat' };
const dispatch = (e: string, d: unknown) => act(() => useStore.getState().applyDispatch(e, d));
const msg = (id: string, channel = 'c1') => ({
  id,
  channel_id: channel,
  author: bob,
  content: `m${id}`,
  timestamp: '2026-01-01T00:00:00Z',
});

function boot() {
  dispatch('READY', {
    user: me,
    guilds: [{ id: 'g1' }, { id: 'g2' }],
    private_channels: [],
    read_state: { entries: [{ id: 'c1', last_message_id: '5', mention_count: 2 }] },
  });
  dispatch('GUILD_CREATE', {
    id: 'g1',
    name: 'One',
    channels: [{ id: 'c1', type: 0, name: 'a', position: 0 }],
    roles: [],
    members: [{ user: bob, roles: [] }],
  });
  dispatch('GUILD_CREATE', {
    id: 'g2',
    name: 'Two',
    channels: [{ id: 'c9', type: 0, name: 'z', position: 0 }],
    roles: [],
    members: [],
  });
  // Two channels with a loaded window each.
  act(() =>
    useStore.setState((s) => ({
      messages: { ...s.messages, '10': msg('10', 'c1'), '90': msg('90', 'c9') },
      windows: {
        ...s.windows,
        c1: { ...emptyWindow(), ids: ['10'] },
        c9: { ...emptyWindow(), ids: ['90'] },
      },
    })),
  );
}

/** A window with a message in it and an optimistic send waiting. */
function addPending(channel: string, nonce: string) {
  const pid = `pending:${nonce}`;
  act(() =>
    useStore.setState((s) => ({
      messages: { ...s.messages, [pid]: { ...msg(pid, channel), id: pid } },
      windows: {
        ...s.windows,
        [channel]: { ...(s.windows[channel] ?? emptyWindow()), pending: [pid] },
      },
    })),
  );
  return pid;
}

beforeEach(() => resetStore());

describe('SR_RESYNC (guild)', () => {
  it('marks only that guild’s channel windows stale and keeps pending sends', () => {
    boot();
    // Fetched, so not stale yet.
    act(() =>
      useStore.setState((s) => ({
        windows: Object.fromEntries(
          Object.entries(s.windows).map(([k, w]) => [k, { ...w, stale: false }]),
        ),
      })),
    );
    const pid = addPending('c1', 'n1');
    dispatch('SR_RESYNC', { scope: 'guild', guild_id: 'g1' });
    const { windows, messages } = useStore.getState();
    expect(windows.c1?.stale).toBe(true);
    expect(windows.c1?.ids).toEqual(['10']);
    expect(windows.c1?.pending).toEqual([pid]);
    expect(messages[pid]).toBeDefined();
    expect(messages['10']).toBeDefined();
    // The other server's window is untouched.
    expect(windows.c9?.stale).toBe(false);
  });

  it('drops the typing entries of that guild’s channels only', () => {
    boot();
    act(() =>
      useStore.setState({
        typing: {
          c1: [{ user_id: '2', expires_at: Date.now() + 10_000 }],
          c9: [{ user_id: '2', expires_at: Date.now() + 10_000 }],
        },
      }),
    );
    dispatch('SR_RESYNC', { scope: 'guild', guild_id: 'g1' });
    expect(Object.keys(useStore.getState().typing)).toEqual(['c9']);
  });

  it('ignores a marker without a guild, and the platform marker', () => {
    boot();
    const before = useStore.getState();
    dispatch('SR_RESYNC', { scope: 'guild' });
    dispatch('SR_RESYNC', { scope: 'platform' });
    const after = useStore.getState();
    expect(after.windows).toBe(before.windows);
    expect(after.typing).toBe(before.typing);
    expect(after.guilds).toBe(before.guilds);
  });

  it('reads the member list again, as a replacement, when it was loaded', async () => {
    boot();
    expect(useStore.getState().memberIdsByGuild.g1).toEqual(['2']);
    // Bob left while events were lost; cat is the whole roster now.
    server.use(
      http.get('*/api/v1/guilds/g1/members', () => HttpResponse.json([{ user: cat, roles: [] }])),
    );
    dispatch('SR_RESYNC', { scope: 'guild', guild_id: 'g1' });
    await vi.waitFor(() => expect(useStore.getState().memberIdsByGuild.g1).toEqual(['3']));
    expect(useStore.getState().members['g1:2']).toBeUndefined();
  });

  it('does not fetch a member list that was never loaded', async () => {
    dispatch('READY', { user: me, guilds: [{ id: 'g5' }], private_channels: [] });
    let asked = 0;
    server.use(
      http.get('*/api/v1/guilds/g5/members', () => {
        asked += 1;
        return HttpResponse.json([]);
      }),
    );
    dispatch('SR_RESYNC', { scope: 'guild', guild_id: 'g5' });
    await new Promise((r) => setTimeout(r, 30));
    expect(asked).toBe(0);
  });
});

describe('members after a plain fetch', () => {
  it('are still merged, so a page does not drop the others', async () => {
    boot();
    server.use(
      http.get('*/api/v1/guilds/g1/members', () => HttpResponse.json([{ user: cat, roles: [] }])),
    );
    await act(() => useStore.getState().loadMembers('g1'));
    expect(useStore.getState().memberIdsByGuild.g1?.sort()).toEqual(['2', '3']);
  });
});

describe('a second READY on the same session (the user topic resync)', () => {
  it('replaces the read state and keeps pending sends', () => {
    boot();
    const pid = addPending('c1', 'n2');
    dispatch('READY', {
      user: me,
      session_id: 's1',
      guilds: [{ id: 'g1' }, { id: 'g2' }],
      private_channels: [],
      read_state: { entries: [{ id: 'c9', last_message_id: '7', mention_count: 0 }] },
    });
    const s = useStore.getState();
    expect(Object.keys(s.readStates)).toEqual(['c9']);
    expect(s.windows.c1?.pending).toEqual([pid]);
    expect(s.windows.c1?.stale).toBe(true);
    expect(s.messages[pid]).toBeDefined();
  });
});

describe('READY and servers that are gone', () => {
  it('removes a server that is not in READY, with its channels and members', () => {
    boot();
    dispatch('READY', { user: me, guilds: [{ id: 'g1' }], private_channels: [] });
    const s = useStore.getState();
    expect(Object.keys(s.guilds)).toEqual(['g1']);
    expect(s.guildOrder).toEqual(['g1']);
    expect(s.channels.c9).toBeUndefined();
    expect(s.windows.c9).toBeUndefined();
    expect(s.channelIdsByGuild.g2).toBeUndefined();
    // The server that stays keeps what it held until its GUILD_CREATE replaces it.
    expect(s.channels.c1).toBeDefined();
    expect(s.windows.c1?.stale).toBe(true);
  });
});
