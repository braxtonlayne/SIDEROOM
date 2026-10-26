import { GatewayClient, resumeUrlWithQuery, type SocketLike } from './client';
import { Op } from './protocol';

class FakeSocket implements SocketLike {
  static all: FakeSocket[] = [];
  readyState = 1;
  binaryType = '';
  onopen: SocketLike['onopen'] = null;
  onmessage: SocketLike['onmessage'] = null;
  onclose: SocketLike['onclose'] = null;
  onerror: SocketLike['onerror'] = null;
  sent: { op: number; d: unknown }[] = [];
  closedWith: number | undefined;
  constructor(public url: string) {
    FakeSocket.all.push(this);
  }
  send(data: string) {
    this.sent.push(JSON.parse(data) as { op: number; d: unknown });
  }
  close(code?: number) {
    this.closedWith = code;
    this.readyState = 3;
  }
  // test helpers
  recv(p: object) {
    this.onmessage?.({ data: JSON.stringify(p) });
  }
  serverClose(code: number) {
    this.readyState = 3;
    this.onclose?.({ code });
  }
  ops() {
    return this.sent.map((m) => m.op).filter((o) => o !== Op.HEARTBEAT);
  }
}

const last = () => FakeSocket.all[FakeSocket.all.length - 1]!;
const flush = () => vi.advanceTimersByTimeAsync(0);

function setup(extra: Partial<ConstructorParameters<typeof GatewayClient>[0]> = {}) {
  const events: [string, unknown, number][] = [];
  const status: string[] = [];
  const client = new GatewayClient({
    url: 'ws://x/gateway?v=1&encoding=json',
    getToken: () => 'tok',
    random: () => 0.5,
    createSocket: (u) => new FakeSocket(u),
    onDispatch: (e, d, s) => events.push([e, d, s]),
    onStatus: (s) => status.push(s),
    ...extra,
  });
  return { client, events, status };
}

async function handshake(interval = 1000) {
  const ws = last();
  ws.recv({ op: Op.HELLO, d: { heartbeat_interval: interval } });
  await flush();
  return ws;
}

beforeEach(() => {
  FakeSocket.all = [];
  vi.useFakeTimers();
});
afterEach(() => vi.useRealTimers());

describe('GatewayClient', () => {
  it('keeps the static backoff cap', () => {
    expect(GatewayClient.backoffMs(0)).toBe(1000);
    expect(GatewayClient.backoffMs(20)).toBe(30_000);
  });

  it('identifies after HELLO and becomes connected on READY', async () => {
    const { client, events, status } = setup();
    client.connect();
    const ws = await handshake();
    expect(ws.sent[0]).toMatchObject({ op: Op.IDENTIFY, d: { token: 'tok' } });
    ws.recv({ op: 0, t: 'READY', s: 1, d: { session_id: 'abc', resume_gateway_url: 'ws://r/g' } });
    expect(client.session).toBe('abc');
    expect(client.lastSequence).toBe(1);
    expect(status).toEqual(['connecting', 'connected']);
    expect(events[0]?.[0]).toBe('READY');
  });

  it('heartbeats with the sequence and answers server heartbeat requests', async () => {
    const { client } = setup();
    client.connect();
    const ws = await handshake();
    ws.recv({ op: 0, t: 'READY', s: 5, d: { session_id: 'abc' } });
    await vi.advanceTimersByTimeAsync(500); // jitter 0.5 * 1000
    expect(ws.sent.at(-1)).toEqual({ op: Op.HEARTBEAT, d: 5 });
    ws.recv({ op: Op.HEARTBEAT_ACK });
    ws.recv({ op: Op.HEARTBEAT });
    expect(ws.sent.at(-1)).toEqual({ op: Op.HEARTBEAT, d: 5 });
  });

  it('reconnects and resumes when a heartbeat is not acked', async () => {
    const { client } = setup();
    client.connect();
    const ws = await handshake();
    ws.recv({ op: 0, t: 'READY', s: 3, d: { session_id: 'abc', resume_gateway_url: 'ws://r/g' } });
    await vi.advanceTimersByTimeAsync(500); // first beat, never acked
    await vi.advanceTimersByTimeAsync(1000); // interval elapses -> zombie
    expect(ws.closedWith).toBe(4000);
    await vi.advanceTimersByTimeAsync(1);
    const ws2 = last();
    expect(ws2).not.toBe(ws);
    // The bare resume URL gets the client's query: without `v` the server answers 400 (RDY-VOX-002).
    expect(ws2.url).toBe('ws://r/g?v=1&encoding=json');
    ws2.recv({ op: Op.HELLO, d: { heartbeat_interval: 1000 } });
    await flush();
    expect(ws2.sent[0]).toEqual({
      op: Op.RESUME,
      d: { token: 'tok', session_id: 'abc', seq: 3 },
    });
    ws2.recv({ op: 0, t: 'RESUMED', s: 4, d: {} });
    expect(client.lastSequence).toBe(4);
  });

  it('resumes after RECONNECT', async () => {
    const { client } = setup();
    client.connect();
    const ws = await handshake();
    ws.recv({ op: 0, t: 'READY', s: 1, d: { session_id: 's' } });
    ws.recv({ op: Op.RECONNECT });
    await vi.advanceTimersByTimeAsync(1);
    const ws2 = last();
    expect(ws2).not.toBe(ws);
    ws2.recv({ op: Op.HELLO, d: { heartbeat_interval: 1000 } });
    await flush();
    expect(ws2.ops()).toEqual([Op.RESUME]);
  });

  it('re-identifies after a non-resumable INVALID_SESSION and keeps it on resumable', async () => {
    const { client } = setup();
    client.connect();
    const ws = await handshake(60_000);
    ws.recv({ op: 0, t: 'READY', s: 1, d: { session_id: 's' } });
    ws.recv({ op: Op.INVALID_SESSION, d: true });
    await vi.advanceTimersByTimeAsync(3000); // 1000 + 0.5*4000
    expect(ws.ops().at(-1)).toBe(Op.RESUME);
    ws.recv({ op: Op.INVALID_SESSION, d: false });
    await vi.advanceTimersByTimeAsync(3000);
    expect(ws.ops().at(-1)).toBe(Op.IDENTIFY);
    expect(client.session).toBeNull();
  });

  it('backs off with jitter, doubling, and resets after READY', async () => {
    const { client } = setup({ random: () => 0 });
    client.connect();
    // random=0 -> delay is half the exponential value: 500, 1000, 2000
    last().serverClose(1006);
    await vi.advanceTimersByTimeAsync(499);
    expect(FakeSocket.all).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(FakeSocket.all).toHaveLength(2);
    last().serverClose(1006);
    await vi.advanceTimersByTimeAsync(999);
    expect(FakeSocket.all).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(1);
    expect(FakeSocket.all).toHaveLength(3);
    const ws = await handshake();
    ws.recv({ op: 0, t: 'READY', s: 1, d: { session_id: 's' } });
    ws.serverClose(1006);
    await vi.advanceTimersByTimeAsync(500);
    expect(FakeSocket.all).toHaveLength(4);
  });

  it('applies jitter within [exp/2, exp]', () => {
    const lo = setup({ random: () => 0 }).client.delayMs(3);
    const hi = setup({ random: () => 0.999 }).client.delayMs(3);
    expect(lo).toBe(4000);
    expect(hi).toBeGreaterThan(7900);
    expect(hi).toBeLessThanOrEqual(8000);
  });

  it('does not reconnect after a fatal close code', async () => {
    const onFatal = vi.fn();
    const { client, status } = setup({ onFatal });
    client.connect();
    last().serverClose(4004);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(FakeSocket.all).toHaveLength(1);
    expect(onFatal).toHaveBeenCalledWith(4004);
    expect(status.at(-1)).toBe('closed');
  });

  it('drops the session on 4009 and identifies fresh', async () => {
    const { client } = setup({ random: () => 0 });
    client.connect();
    const ws = await handshake();
    ws.recv({ op: 0, t: 'READY', s: 1, d: { session_id: 's' } });
    ws.serverClose(4009);
    await vi.advanceTimersByTimeAsync(500);
    const ws2 = last();
    ws2.recv({ op: Op.HELLO, d: { heartbeat_interval: 1000 } });
    await flush();
    expect(ws2.ops()).toEqual([Op.IDENTIFY]);
  });

  it('ignores duplicate sequence numbers and stops on close()', async () => {
    const { client, events } = setup();
    client.connect();
    const ws = await handshake();
    ws.recv({ op: 0, t: 'READY', s: 1, d: { session_id: 's' } });
    ws.recv({ op: 0, t: 'MESSAGE_CREATE', s: 2, d: {} });
    ws.recv({ op: 0, t: 'MESSAGE_CREATE', s: 2, d: {} });
    expect(events.map((e) => e[2])).toEqual([1, 2]);
    client.close();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(FakeSocket.all).toHaveLength(1);
    expect(client.send(Op.PRESENCE_UPDATE, {})).toBe(false);
  });

  it('decodes binary frames with a supplied decoder and requests compression', async () => {
    const decompress = vi.fn(() => JSON.stringify({ op: 0, t: 'X', s: 1, d: { ok: 1 } }));
    const { client, events } = setup({ decompress });
    client.connect();
    expect(last().url).toContain('compress=zstd-stream');
    last().onmessage?.({ data: new ArrayBuffer(4) });
    await flush();
    expect(events[0]).toEqual(['X', { ok: 1 }, 1]);
  });

  it('closes with a decode error on binary frames without a decoder', async () => {
    const { client } = setup();
    client.connect();
    last().onmessage?.({ data: new ArrayBuffer(4) });
    await flush();
    expect(last().closedWith).toBe(4002);
  });
});

describe('resumeUrlWithQuery (RDY-VOX-002)', () => {
  it('adds the client query to a bare resume URL and leaves a complete one alone', () => {
    const base = 'ws://x/gateway?v=1&encoding=json';
    expect(resumeUrlWithQuery('ws://r/gateway', base)).toBe('ws://r/gateway?v=1&encoding=json');
    expect(resumeUrlWithQuery('ws://r/gateway?region=a', base)).toBe(
      'ws://r/gateway?region=a&v=1&encoding=json',
    );
    expect(resumeUrlWithQuery('ws://r/gateway?v=10', base)).toBe('ws://r/gateway?v=10');
    expect(resumeUrlWithQuery('ws://r/gateway', 'ws://x/gateway')).toBe(
      'ws://r/gateway?v=1&encoding=json',
    );
  });
});
