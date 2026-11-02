import { b64 } from '@sideroom/embedded-app-sdk';
import { CLOSE, topicOf } from '../contract';
import { encodeDirect, encodeRelayed, encodeSys } from '../envelope';
import { guestLane } from '../lane';
import { generateGameKey } from '../link';
import { importGameKey, open, seal } from '../seal';
import { guestSocketUrl, joinLobby, JoinError, openLobby, type SocketLike } from '../session';

const TOKEN = 'cd34'.repeat(16);

class FakeSocket implements SocketLike {
  readyState = 0;
  onopen: SocketLike['onopen'] = null;
  onmessage: SocketLike['onmessage'] = null;
  onclose: SocketLike['onclose'] = null;
  onerror: SocketLike['onerror'] = null;
  sent: Record<string, unknown>[] = [];
  closed: number | undefined;
  send(d: string) {
    this.sent.push(JSON.parse(d) as Record<string, unknown>);
  }
  close(code?: number) {
    this.closed = code;
    this.readyState = 3;
  }
  // test helpers
  connect() {
    this.readyState = 1;
    this.onopen?.({});
  }
  say(f: unknown) {
    this.onmessage?.({ data: JSON.stringify(f) });
  }
  drop(code: number) {
    this.readyState = 3;
    this.onclose?.({ code });
  }
}

async function rig() {
  const keyStr = generateGameKey();
  const key = await importGameKey(keyStr);
  const ws = new FakeSocket();
  const lobby = openLobby({
    url: 'wss://x.test/gateway/guest?v=1',
    hello: { guest_token: 'srg_' + 'a'.repeat(64) },
    lobbyToken: TOKEN,
    key,
    makeSocket: () => ws,
  });
  ws.connect();
  return { ws, lobby, key };
}

const ready = (over: Record<string, unknown> = {}) => ({
  op: 'ready',
  seat: 2,
  role: 'guest',
  callsign: 'Amber Otter 42',
  key_id: 'k',
  epoch: 0,
  scope: { data: true, media: false },
  peers: [{ seat: 1, role: 'member', callsign: 'Host' }],
  ...over,
});

const settle = () => new Promise((r) => setTimeout(r, 20));

describe('joinLobby', () => {
  const answer = (status: number, body?: unknown) =>
    (async () =>
      new Response(body === undefined ? null : JSON.stringify(body), {
        status,
      })) as unknown as typeof fetch;

  it('posts with no body, no cookie and no referrer to the token path', async () => {
    const f = vi.fn(
      answer(200, {
        guest_token: 'srg_x',
        callsign: 'Amber Otter 42',
        expires_at: 'e',
        topic: topicOf(TOKEN),
        socket_path: '/gateway/guest',
        application_id: '7',
        application_name: 'Land.io',
        capabilities: ['datachannel', 'guest_play', 'ingame_voice'],
      }),
    );
    const j = await joinLobby(TOKEN, f as unknown as typeof fetch);
    expect(j.callsign).toBe('Amber Otter 42');
    // The relay's answer carries the manifest flags, reserved ones included, so the runner can refuse them.
    expect(j.capabilities).toEqual(['datachannel', 'guest_play', 'ingame_voice']);
    const [url, init] = f.mock.calls[0]!;
    expect(url).toBe(`/api/v1/play/${TOKEN}/join`);
    expect(init).toMatchObject({
      method: 'POST',
      credentials: 'omit',
      referrerPolicy: 'no-referrer',
    });
    expect(init).not.toHaveProperty('body');
  });

  it.each([
    [429, 'rate_limited'],
    [404, 'gone'],
    [410, 'gone'],
    [403, 'refused'],
    [500, 'refused'],
  ])('maps %i to %s', async (status, reason) => {
    await expect(joinLobby(TOKEN, answer(status))).rejects.toMatchObject({ reason });
  });

  it('maps a network failure and a malformed answer', async () => {
    await expect(
      joinLobby(TOKEN, (async () => {
        throw new TypeError('offline');
      }) as unknown as typeof fetch),
    ).rejects.toBeInstanceOf(JoinError);
    await expect(joinLobby(TOKEN, answer(200, { nope: 1 }))).rejects.toMatchObject({
      reason: 'refused',
    });
    // An answer without the manifest flags cannot be checked for `ingame_voice`: refused, never assumed.
    await expect(
      joinLobby(TOKEN, answer(200, { guest_token: 'srg_x', callsign: 'c', application_id: '7' })),
    ).rejects.toMatchObject({ reason: 'refused' });
  });
});

describe('the lobby socket', () => {
  it('says hello with the guest token, then subscribes to its one topic', async () => {
    const { ws, lobby } = await rig();
    expect(ws.sent[0]).toEqual({ op: 'hello', guest_token: 'srg_' + 'a'.repeat(64) });
    ws.say(ready());
    const seated = await lobby.seated;
    expect(seated).toMatchObject({ seat: 2, role: 'guest', callsign: 'Amber Otter 42' });
    expect(ws.sent[1]).toEqual({ op: 'subscribe', topic: topicOf(TOKEN) });
  });

  it('can only ever name its own lobby topic', async () => {
    const { ws, lobby } = await rig();
    ws.say(ready());
    await lobby.seated;
    await lobby.publish(new Uint8Array([1]));
    await lobby.publish(new Uint8Array([2]));
    const topics = ws.sent.map((f) => f.topic).filter(Boolean);
    expect(topics.length).toBe(3);
    expect(new Set(topics)).toEqual(new Set([topicOf(TOKEN)]));
  });

  it('seals what it publishes with the game key (the relay sees ciphertext)', async () => {
    const { ws, lobby, key } = await rig();
    ws.say(ready());
    await lobby.seated;
    await lobby.publish(new TextEncoder().encode('secret-move'));
    const pub = ws.sent.find((f) => f.op === 'publish')!;
    const sealed = Uint8Array.from(atob(pub.data as string), (c) => c.charCodeAt(0));
    expect(new TextDecoder().decode(sealed)).not.toContain('secret-move');
    expect(new TextDecoder().decode((await open(key, TOKEN, sealed))!)).toBe('secret-move');
  });

  it('opens data from another seat with the relay-stamped role, and ignores what is not sealed for it', async () => {
    const { ws, lobby, key } = await rig();
    ws.say(ready());
    await lobby.seated;
    const got: { seat: number; role: string; b: number[] }[] = [];
    lobby.onMessage((m) => got.push({ seat: m.seat, role: m.role, b: Array.from(m.bytes) }));
    ws.say({ op: 'data', from: 1, data: b64(await seal(key, TOKEN, new Uint8Array([5, 6]))) });
    // sealed under another key: dropped
    const other = await importGameKey(generateGameKey());
    ws.say({ op: 'data', from: 1, data: b64(await seal(other, TOKEN, new Uint8Array([9]))) });
    // from a seat the relay never announced: dropped
    ws.say({ op: 'data', from: 77, data: b64(await seal(key, TOKEN, new Uint8Array([8]))) });
    // our own seat echoed: dropped
    ws.say({ op: 'data', from: 2, data: b64(await seal(key, TOKEN, new Uint8Array([4]))) });
    await settle();
    expect(got).toEqual([{ seat: 1, role: 'member', b: [5, 6] }]);
  });

  it('tracks peers joining and leaving', async () => {
    const { ws, lobby } = await rig();
    ws.say(ready());
    await lobby.seated;
    ws.say({
      op: 'peer',
      event: 'join',
      peer: { seat: 3, role: 'guest', callsign: 'Brisk Crane 11' },
    });
    expect(
      lobby
        .peers()
        .map((p) => p.seat)
        .sort(),
    ).toEqual([1, 2, 3]);
    ws.say({ op: 'peer', event: 'leave', seat: 3 });
    expect(
      lobby
        .peers()
        .map((p) => p.seat)
        .sort(),
    ).toEqual([1, 2]);
  });

  it('reports the relay dropping it (4403 for a forged topic, 4410 for a revoked link) once', async () => {
    const { ws, lobby } = await rig();
    ws.say(ready());
    await lobby.seated;
    const ends: number[] = [];
    lobby.onEnd((c) => ends.push(c));
    ws.drop(CLOSE.forbidden);
    ws.drop(CLOSE.gone);
    expect(ends).toEqual([CLOSE.forbidden]);
    expect(await lobby.publish(new Uint8Array([1]))).toBe(false);
  });

  it('rejects `seated` when the relay refuses the credential', async () => {
    const { ws, lobby } = await rig();
    ws.drop(CLOSE.unauthorized);
    await expect(lobby.seated).rejects.toThrow('4401');
  });

  it('builds the socket address from the page origin', () => {
    expect(guestSocketUrl({ protocol: 'https:', host: 'chat.example.org' })).toBe(
      'wss://chat.example.org/gateway/guest?v=1',
    );
    expect(guestSocketUrl({ protocol: 'http:', host: 'localhost:5173' })).toBe(
      'ws://localhost:5173/gateway/guest?v=1',
    );
  });
});

describe('the guest lane', () => {
  it('delivers a member seat’s relayed message under the room id and a guest under g<seat>', async () => {
    const { ws, lobby, key } = await rig();
    ws.say(ready());
    await lobby.seated;
    ws.say({
      op: 'peer',
      event: 'join',
      peer: { seat: 3, role: 'guest', callsign: 'Brisk Crane 11' },
    });
    const kit = guestLane(lobby);
    const got: [string, number[]][] = [];
    kit.lane.onMessage((from, b) => got.push([from, Array.from(b)]));
    ws.say({
      op: 'data',
      from: 1,
      data: b64(await seal(key, TOKEN, encodeRelayed('5550001', new Uint8Array([1])))),
    });
    ws.say({
      op: 'data',
      from: 3,
      data: b64(await seal(key, TOKEN, encodeDirect(new Uint8Array([2])))),
    });
    await settle();
    expect(got).toEqual([
      ['5550001', [1]],
      ['g3', [2]],
    ]);
  });

  it('learns the host from a member control message, with a member seat as the fallback', async () => {
    const { ws, lobby, key } = await rig();
    ws.say(ready());
    await lobby.seated;
    const kit = guestLane(lobby);
    expect(kit.hostId()).toBe('m1');
    expect(await kit.hostKnown(10)).toBe(false);
    const known = kit.hostKnown(500);
    ws.say({
      op: 'data',
      from: 1,
      data: b64(await seal(key, TOKEN, encodeSys({ host: '5550001' }))),
    });
    expect(await known).toBe(true);
    expect(kit.hostId()).toBe('5550001');
  });

  it('D-388: tells the runner when a member names a different host, not for the first name or a repeat', async () => {
    const { ws, lobby, key } = await rig();
    ws.say(ready());
    await lobby.seated;
    const kit = guestLane(lobby);
    const moves: [string, string | null][] = [];
    kit.onHost((h, p) => moves.push([h, p]));
    for (const host of ['5550001', '5550001', '5550002']) {
      ws.say({ op: 'data', from: 1, data: b64(await seal(key, TOKEN, encodeSys({ host }))) });
      await settle();
    }
    expect(moves).toEqual([['5550002', '5550001']]);
    expect(kit.hostId()).toBe('5550002');
  });

  it('wraps what the game sends as a direct message', async () => {
    const { ws, lobby, key } = await rig();
    ws.say(ready());
    await lobby.seated;
    const kit = guestLane(lobby);
    await kit.lane.send(new Uint8Array([4, 5]));
    const pub = ws.sent.find((f) => f.op === 'publish')!;
    const plain = await open(
      key,
      TOKEN,
      Uint8Array.from(atob(pub.data as string), (c) => c.charCodeAt(0)),
    );
    expect(Array.from(plain!)).toEqual([0, 4, 5]);
  });

  it('lists callsigns as the roster, members flagged as host', async () => {
    const { ws, lobby } = await rig();
    ws.say(ready());
    await lobby.seated;
    expect(guestLane(lobby).roster()).toEqual([
      { id: 'g2', nickname: 'Amber Otter 42' },
      { id: 'm1', nickname: 'Host', host: true },
    ]);
  });
});
