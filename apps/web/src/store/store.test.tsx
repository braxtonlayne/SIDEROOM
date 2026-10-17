import { act, renderHook, waitFor } from '@testing-library/react';
import { http, HttpResponse, server } from '../test/server';
import { GatewayClient, type SocketLike } from '../gateway';
import {
  resetStore,
  useChannels,
  useGuild,
  useGuilds,
  useMembers,
  useMessages,
  usePresence,
  useStore,
  useTyping,
} from './index';

const me = { id: '1', username: 'me' };
const bob = { id: '2', username: 'bob' };
const dispatch = (e: string, d: unknown) => act(() => useStore.getState().applyDispatch(e, d));
const msg = (id: string, extra: object = {}) => ({
  id,
  channel_id: 'c1',
  author: bob,
  content: `m${id}`,
  timestamp: '2026-01-01T00:00:00Z',
  ...extra,
});

function boot() {
  dispatch('READY', { user: me, guilds: [{ id: 'g1' }, { id: 'g2' }], private_channels: [] });
  dispatch('GUILD_CREATE', {
    id: 'g1',
    name: 'One',
    channels: [
      { id: 'c2', type: 0, name: 'b', position: 1 },
      { id: 'c1', type: 0, name: 'a', position: 0 },
    ],
    roles: [
      { id: 'r1', name: 'low', color: 0, position: 1, permissions: '0' },
      { id: 'r2', name: 'high', color: 0, position: 5, permissions: '0' },
    ],
    members: [{ user: bob, roles: ['r1', 'r2'] }],
    presences: [{ user: { id: '2' }, status: 'online', activities: [] }],
  });
}

beforeEach(() => resetStore());

describe('store reducers via hooks', () => {
  it('hydrates guilds, channels, roles, members and presence', () => {
    boot();
    const guilds = renderHook(() => useGuilds());
    expect(guilds.result.current.map((g) => [g.id, g.unavailable])).toEqual([
      ['g1', false],
      ['g2', true],
    ]);
    expect(renderHook(() => useChannels('g1')).result.current.map((c) => c.id)).toEqual([
      'c1',
      'c2',
    ]);
    expect(renderHook(() => useGuild('g1')).result.current.roles.map((r) => r.id)).toEqual([
      'r2',
      'r1',
    ]);
    const members = renderHook(() => useMembers('g1')).result.current.members;
    expect(members[0]?.user?.username).toBe('bob');
    expect(members[0]?.roles[0]?.id).toBe('r2');
    expect(renderHook(() => usePresence('2')).result.current.status).toBe('online');
    expect(renderHook(() => usePresence('nobody')).result.current.status).toBe('offline');
  });

  it('applies channel, role, member, presence updates and guild delete', () => {
    boot();
    dispatch('CHANNEL_CREATE', { id: 'c3', type: 0, guild_id: 'g1', position: 2 });
    dispatch('CHANNEL_DELETE', { id: 'c2', guild_id: 'g1' });
    expect(useStore.getState().channelIdsByGuild.g1).toEqual(['c1', 'c3']);
    dispatch('GUILD_ROLE_DELETE', { guild_id: 'g1', role_id: 'r2' });
    expect(useStore.getState().members['g1:2']?.roles).toEqual(['r1']);
    dispatch('PRESENCE_UPDATE', { user: { id: '2' }, status: 'idle', activities: [] });
    expect(useStore.getState().presences['2']?.status).toBe('idle');
    dispatch('GUILD_MEMBER_REMOVE', { guild_id: 'g1', user: bob });
    expect(useStore.getState().memberIdsByGuild.g1).toEqual([]);
    dispatch('GUILD_DELETE', { id: 'g1' });
    expect(useStore.getState().guilds.g1).toBeUndefined();
    expect(useStore.getState().channels.c1).toBeUndefined();
  });
});

describe('message windows', () => {
  it('loads history oldest-first, pages older, and orders live inserts', async () => {
    boot();
    const calls: string[] = [];
    server.use(
      http.get('/api/v1/channels/c1/messages', ({ request }) => {
        const url = new URL(request.url);
        calls.push(url.search);
        const before = url.searchParams.get('before');
        if (!before) {
          // 50 newest-first: ids 150..101
          return HttpResponse.json(Array.from({ length: 50 }, (_, i) => msg(String(150 - i))));
        }
        return HttpResponse.json([msg('100'), msg('99')]);
      }),
    );
    const { result } = renderHook(() => useMessages('c1'));
    await waitFor(() => expect(result.current.messages).toHaveLength(50));
    expect(result.current.messages[0]?.id).toBe('101');
    expect(result.current.hasMoreBefore).toBe(true);

    await act(() => result.current.loadOlder());
    expect(calls[1]).toContain('before=101');
    expect(result.current.messages.map((m) => m.id).slice(0, 3)).toEqual(['99', '100', '101']);
    expect(result.current.hasMoreBefore).toBe(false);

    dispatch('MESSAGE_CREATE', msg('160'));
    dispatch('MESSAGE_CREATE', msg('155'));
    dispatch('MESSAGE_CREATE', msg('160'));
    const ids = result.current.messages.map((m) => m.id);
    expect(ids.slice(-2)).toEqual(['155', '160']);
    expect(ids.filter((i) => i === '160')).toHaveLength(1);

    dispatch('MESSAGE_UPDATE', { id: '160', channel_id: 'c1', content: 'edited' });
    expect(result.current.messages.at(-1)?.content).toBe('edited');
    // A card rewritten without its buttons: the update leaves `components` out, and they go.
    dispatch('MESSAGE_UPDATE', {
      id: '160',
      channel_id: 'c1',
      content: 'card',
      components: [
        { type: 1, components: [{ type: 2, style: 3, label: 'Accept', custom_id: 'sr:x:1' }] },
      ],
    });
    expect(result.current.messages.at(-1)?.components).toHaveLength(1);
    dispatch('MESSAGE_UPDATE', { id: '160', channel_id: 'c1', content: 'card' });
    expect(result.current.messages.at(-1)?.components).toBeUndefined();
    dispatch('MESSAGE_DELETE', { id: '160', channel_id: 'c1' });
    expect(result.current.messages.at(-1)?.id).toBe('155');
    expect(useStore.getState().channels.c1?.last_message_id).toBe('160');
  });

  it('does not append live messages to channels with no window', () => {
    boot();
    dispatch('MESSAGE_CREATE', msg('5'));
    expect(useStore.getState().windows.c1).toBeUndefined();
  });

  it('caps the window size', () => {
    boot();
    useStore.setState({
      windows: {
        c1: {
          ids: [],
          pending: [],
          hasMoreBefore: false,
          atLatest: true,
          loading: false,
          error: null,
        },
      },
    });
    for (let i = 1; i <= 310; i++) dispatch('MESSAGE_CREATE', msg(String(1000 + i)));
    const w = useStore.getState().windows.c1!;
    expect(w.ids).toHaveLength(300);
    expect(w.hasMoreBefore).toBe(true);
    expect(useStore.getState().messages['1001']).toBeUndefined();
  });
});

describe('optimistic sending', () => {
  beforeEach(() => {
    boot();
    server.use(http.get('/api/v1/channels/c1/messages', () => HttpResponse.json([])));
  });

  it('shows a pending message and reconciles when the gateway echo arrives first', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    let sentNonce = '';
    server.use(
      http.post('/api/v1/channels/c1/messages', async ({ request }) => {
        const body = (await request.json()) as { content: string; nonce: string };
        sentNonce = body.nonce;
        await gate;
        return HttpResponse.json(
          msg('200', { author: me, content: body.content, nonce: body.nonce }),
        );
      }),
    );
    const { result } = renderHook(() => useMessages('c1'));
    await waitFor(() => expect(useStore.getState().windows.c1?.loading).toBe(false));
    let p!: Promise<unknown>;
    act(() => {
      p = result.current.send('hello');
    });
    expect(result.current.messages).toHaveLength(1);
    expect(result.current.messages[0]?.local?.status).toBe('sending');
    await waitFor(() => expect(sentNonce).not.toBe(''));

    dispatch('MESSAGE_CREATE', msg('200', { author: me, content: 'hello', nonce: sentNonce }));
    expect(result.current.messages.map((m) => [m.id, m.local])).toEqual([['200', undefined]]);

    release();
    await act(() => p);
    expect(result.current.messages).toHaveLength(1);
    expect(useStore.getState().windows.c1?.pending).toEqual([]);
  });

  it('reconciles from the REST response when the gateway echo never comes', async () => {
    server.use(
      http.post('/api/v1/channels/c1/messages', async ({ request }) => {
        const body = (await request.json()) as { content: string; nonce: string };
        return HttpResponse.json(
          msg('201', { author: me, content: body.content, nonce: body.nonce }),
        );
      }),
    );
    const { result } = renderHook(() => useMessages('c1'));
    await waitFor(() => expect(useStore.getState().windows.c1?.loading).toBe(false));
    await act(() => result.current.send('hi'));
    expect(result.current.messages.map((m) => m.id)).toEqual(['201']);
  });

  it('marks failures, then retries or discards', async () => {
    let fail = true;
    server.use(
      http.post('/api/v1/channels/c1/messages', async ({ request }) => {
        const body = (await request.json()) as { content: string; nonce: string };
        if (fail) return HttpResponse.json({ message: 'nope' }, { status: 500 });
        return HttpResponse.json(
          msg('202', { author: me, content: body.content, nonce: body.nonce }),
        );
      }),
    );
    const { result } = renderHook(() => useMessages('c1'));
    await waitFor(() => expect(useStore.getState().windows.c1?.loading).toBe(false));
    await act(async () => {
      await result.current.send('x').catch(() => undefined);
    });
    const failed = result.current.messages[0]!;
    expect(failed.local?.status).toBe('failed');
    fail = false;
    await act(() => result.current.retry(failed.local!.nonce));
    expect(result.current.messages.map((m) => m.id)).toEqual(['202']);

    fail = true;
    await act(async () => {
      await result.current.send('y').catch(() => undefined);
    });
    act(() => result.current.discard(result.current.messages.at(-1)!.local!.nonce));
    expect(result.current.messages.map((m) => m.id)).toEqual(['202']);
  });
});

describe('typing', () => {
  it('shows typers, expires them, and clears on message', () => {
    vi.useFakeTimers();
    try {
      boot();
      const { result } = renderHook(() => useTyping('c1'));
      dispatch('TYPING_START', { channel_id: 'c1', user_id: '2' });
      dispatch('TYPING_START', { channel_id: 'c1', user_id: '1' }); // self ignored
      expect(result.current.map((u) => u.id)).toEqual(['2']);
      act(() => {
        vi.advanceTimersByTime(10_100);
      });
      expect(result.current).toEqual([]);

      dispatch('TYPING_START', { channel_id: 'c1', user_id: '2' });
      expect(result.current).toHaveLength(1);
      dispatch('MESSAGE_CREATE', msg('9'));
      expect(result.current).toEqual([]);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('gateway -> store integration', () => {
  it('feeds fake-socket dispatches into the store', async () => {
    vi.useFakeTimers();
    try {
      const sock = {
        readyState: 1,
        binaryType: '',
        onopen: null,
        onmessage: null,
        onclose: null,
        onerror: null,
        send: () => undefined,
        close: () => undefined,
      } as SocketLike;

      const client = new GatewayClient({
        url: 'ws://x',
        createSocket: () => sock,
        onDispatch: (e, d) => useStore.getState().applyDispatch(e, d),
        onStatus: (s) => useStore.getState().setConnection(s),
      });
      client.connect();
      const recv = (p: object) => sock.onmessage?.({ data: JSON.stringify(p) });
      recv({ op: 10, d: { heartbeat_interval: 1000 } });
      await vi.advanceTimersByTimeAsync(0);
      recv({ op: 0, t: 'READY', s: 1, d: { user: me, guilds: [{ id: 'g9' }] } });
      expect(useStore.getState().connection).toBe('connected');
      expect(useStore.getState().currentUserId).toBe('1');
      expect(useStore.getState().guildOrder).toEqual(['g9']);
      client.close();
    } finally {
      vi.useRealTimers();
    }
  });
});
