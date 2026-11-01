import { describe, expect, it } from 'vitest';
import { SdkError, SideRoomSDK, type SdkEnv } from './sdk';
import { parseFrame, type Frame } from './protocol';

const HOST = 'https://chat.example.org';

/** A fake page: `sent` collects what the Activity posts; `deliver` plays a message from `source`. */
function rig() {
  const sent: { msg: Frame; origin: string }[] = [];
  const listeners = new Set<(e: MessageEvent) => void>();
  const hostWindow = { name: 'host' };
  const env: SdkEnv = {
    parent: { postMessage: (m, o) => sent.push({ msg: m as Frame, origin: o }) },
    hostOrigin: HOST,
    hostWindow,
    addEventListener: (_t, l) => listeners.add(l),
    removeEventListener: (_t, l) => listeners.delete(l),
  };
  const deliver = (data: unknown, source: unknown = hostWindow, origin = HOST) =>
    listeners.forEach((l) => l({ data, source, origin } as MessageEvent));
  return { env, sent, deliver, listeners };
}

const cfg = { application_id: '7', instance_id: '9', channel_id: '3', guild_id: '2' };

describe('parseFrame', () => {
  it('accepts our frames and nothing else', () => {
    expect(parseFrame({ sr: 1, kind: 'handshake', application_id: '1' })).not.toBeNull();
    expect(parseFrame({ sr: 2, kind: 'handshake', application_id: '1' })).toBeNull();
    expect(parseFrame({ sr: 1, kind: 'request', nonce: 1, cmd: 'x' })).toBeNull();
    expect(parseFrame({ sr: 1, kind: 'bogus' })).toBeNull();
    expect(parseFrame('hello')).toBeNull();
    expect(parseFrame(null)).toBeNull();
  });
});

describe('SideRoomSDK', () => {
  it('handshakes, posting only to the host origin, and keeps the config', async () => {
    const r = rig();
    const sdk = new SideRoomSDK('7', { env: r.env });
    const p = sdk.ready();
    expect(r.sent[0]).toEqual({
      msg: { sr: 1, kind: 'handshake', application_id: '7' },
      origin: HOST,
    });
    r.deliver({ sr: 1, kind: 'ready', config: cfg });
    await expect(p).resolves.toEqual(cfg);
    expect(sdk.config).toEqual(cfg);
    await expect(sdk.ready()).resolves.toEqual(cfg);
  });

  it('matches responses to requests by nonce and surfaces errors', async () => {
    const r = rig();
    const sdk = new SideRoomSDK('7', { env: r.env });
    const a = sdk.commands.getChannel();
    const b = sdk.commands.authorize({ scope: ['identify'] });
    const [reqA, reqB] = r.sent.map((s) => s.msg as Extract<Frame, { kind: 'request' }>);
    expect(reqA?.cmd).toBe('getChannel');
    expect(reqB?.args).toEqual({ scope: ['identify'] });
    // Answers arrive out of order.
    r.deliver({
      sr: 1,
      kind: 'response',
      nonce: reqB?.nonce,
      ok: false,
      error: { code: 'USER_DENIED', message: 'No.' },
    });
    r.deliver({
      sr: 1,
      kind: 'response',
      nonce: reqA?.nonce,
      ok: true,
      data: { id: '3', guild_id: '2', name: 'Lobby' },
    });
    await expect(a).resolves.toMatchObject({ name: 'Lobby' });
    const err = await b.catch((e: unknown) => e);
    expect(err).toBeInstanceOf(SdkError);
    expect((err as SdkError).code).toBe('USER_DENIED');
  });

  it('ignores messages from other windows or origins', async () => {
    const r = rig();
    const sdk = new SideRoomSDK('7', { env: r.env, timeoutMs: 20 });
    const p = sdk.ready();
    r.deliver({ sr: 1, kind: 'ready', config: cfg }, { name: 'other frame' });
    r.deliver({ sr: 1, kind: 'ready', config: cfg }, r.env.hostWindow, 'https://evil.example');
    await expect(p).rejects.toMatchObject({ code: 'TIMEOUT' });
    expect(sdk.config).toBeNull();
  });

  it('subscribes once per event and unsubscribes with the last listener', async () => {
    const r = rig();
    const sdk = new SideRoomSDK('7', { env: r.env });
    const got: unknown[] = [];
    const off1 = sdk.subscribe('ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE', (d) => got.push(d));
    const off2 = sdk.subscribe('ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE', (d) => got.push(['2', d]));
    expect(r.sent.filter((s) => (s.msg as { cmd?: string }).cmd === 'subscribe')).toHaveLength(1);
    r.deliver({
      sr: 1,
      kind: 'event',
      event: 'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE',
      data: { participants: [] },
    });
    expect(got).toHaveLength(2);
    off1();
    expect(r.sent.some((s) => (s.msg as { cmd?: string }).cmd === 'unsubscribe')).toBe(false);
    off2();
    expect(r.sent.some((s) => (s.msg as { cmd?: string }).cmd === 'unsubscribe')).toBe(true);
  });

  it('sends setActivity with the activity, and null to clear', async () => {
    const r = rig();
    const sdk = new SideRoomSDK('7', { env: r.env });
    const p = sdk.commands.setActivity({
      activity: { details: 'Round 2', state: 'In a match', timestamps: { start: 5 } },
    });
    const req = r.sent[0]?.msg as Extract<Frame, { kind: 'request' }>;
    expect(req.cmd).toBe('setActivity');
    expect(req.args).toEqual({
      activity: { details: 'Round 2', state: 'In a match', timestamps: { start: 5 } },
    });
    r.deliver({
      sr: 1,
      kind: 'response',
      nonce: req.nonce,
      ok: true,
      data: { details: 'Round 2' },
    });
    await expect(p).resolves.toEqual({ details: 'Round 2' });
    const c = sdk.commands.setActivity({ activity: null });
    const clear = r.sent[1]?.msg as Extract<Frame, { kind: 'request' }>;
    expect(clear.args).toEqual({ activity: null });
    r.deliver({ sr: 1, kind: 'response', nonce: clear.nonce, ok: true, data: null });
    await expect(c).resolves.toBeNull();
  });

  it('subscribes to voice events for a channel and delivers them', () => {
    const r = rig();
    const sdk = new SideRoomSDK('7', { env: r.env });
    const got: unknown[] = [];
    sdk.subscribe('SPEAKING_START', (d) => got.push(d), { channel_id: '3' });
    const sub = r.sent[0]?.msg as Extract<Frame, { kind: 'request' }>;
    expect(sub.cmd).toBe('subscribe');
    expect(sub.args).toEqual({ event: 'SPEAKING_START', args: { channel_id: '3' } });
    r.deliver({
      sr: 1,
      kind: 'event',
      event: 'SPEAKING_START',
      data: { channel_id: '3', user_id: '9' },
    });
    r.deliver({
      sr: 1,
      kind: 'event',
      event: 'SPEAKING_STOP',
      data: { channel_id: '3', user_id: '9' },
    });
    expect(got).toEqual([{ channel_id: '3', user_id: '9' }]);
  });

  it('fails clearly outside a host and after close', async () => {
    const r = rig();
    const lone = new SideRoomSDK('7', { env: { ...r.env, parent: null } });
    await expect(lone.ready()).rejects.toMatchObject({ code: 'NO_HOST' });
    await expect(lone.commands.getChannel()).rejects.toMatchObject({ code: 'NO_HOST' });
    const sdk = new SideRoomSDK('7', { env: r.env });
    const p = sdk.commands.getChannel();
    sdk.close();
    await expect(p).rejects.toMatchObject({ code: 'CLOSED' });
    expect(r.listeners.size).toBe(1); // only `lone` still listens
  });
});

describe('SideRoomSDK storage (ARC-SBX-001)', () => {
  async function ready(capabilities: string[], storage: Record<string, string> = {}) {
    const r = rig();
    const sdk = new SideRoomSDK('7', { env: r.env });
    const p = sdk.ready();
    r.deliver({ sr: 1, kind: 'ready', config: { ...cfg, capabilities, storage } });
    await p;
    return { r, sdk };
  }

  it('reads what the host sent and writes through the bridge', async () => {
    const { r, sdk } = await ready(['storage'], { a: '1' });
    expect(sdk.storage.getItem('a')).toBe('1');
    expect(sdk.storage.getItem('nope')).toBeNull();
    sdk.storage.setItem('b', '2');
    expect(sdk.storage.getItem('b')).toBe('2');
    expect(sdk.storage.length).toBe(2);
    const sets = r.sent.map((s) => s.msg).filter((m) => m.kind === 'request');
    expect(sets.at(-1)).toMatchObject({ cmd: 'storageSet', args: { key: 'b', value: '2' } });
    sdk.storage.removeItem('a');
    expect(sdk.storage.getItem('a')).toBeNull();
    expect(r.sent.at(-1)?.msg).toMatchObject({ cmd: 'storageRemove', args: { key: 'a' } });
  });

  it('throws without the capability and over the quota', async () => {
    const off = await ready([]);
    expect(() => off.sdk.storage.setItem('a', 'b')).toThrow(/storage capability/);
    const { sdk } = await ready(['storage']);
    expect(() => sdk.storage.setItem('a', 'x'.repeat(32 * 1024 + 1))).toThrow(/quota/);
    expect(() => sdk.storage.setItem('', 'x')).toThrow(/quota/);
  });

  it('can stand in for localStorage', async () => {
    const { sdk } = await ready(['storage'], { a: '1' });
    const target: { localStorage?: Storage } = {};
    sdk.storage.install(target);
    expect(target.localStorage?.getItem('a')).toBe('1');
  });

  it('saveFile sends base64 and the type', async () => {
    const { r, sdk } = await ready([]);
    void sdk.commands.saveFile({ name: 'a.png', mime: 'image/png', bytes: new Uint8Array([1, 2]) });
    expect(r.sent.at(-1)?.msg).toMatchObject({
      cmd: 'saveFile',
      args: { name: 'a.png', mime: 'image/png', b: 'AQI=' },
    });
  });
});

describe('SideRoomSDK mappedFetch (D-399)', () => {
  it('sends the proxy token from ready, without credentials', async () => {
    const r = rig();
    const calls: { url: string; init?: RequestInit }[] = [];
    const sdk = new SideRoomSDK('7', {
      env: {
        ...r.env,
        fetch: async (url, init) => {
          calls.push({ url, init });
          return new Response('{"ok":true}', { status: 201 });
        },
      },
    });
    const p = sdk.ready();
    r.deliver({ sr: 1, kind: 'ready', config: { ...cfg, proxy_token: 'ab'.repeat(32) } });
    await p;
    const res = await sdk.mappedFetch('/api/scores', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: '{"score":7}',
    });
    expect(res.status).toBe(201);
    expect(calls[0]?.url).toBe('/api/scores');
    const headers = new Headers(calls[0]?.init?.headers);
    expect(headers.get('x-sr-activity-token')).toBe('ab'.repeat(32));
    expect(headers.get('content-type')).toBe('application/json');
    expect(calls[0]?.init?.credentials).toBe('omit');
  });

  it('refuses a full URL and sends no token it does not have', async () => {
    const r = rig();
    const calls: RequestInit[] = [];
    const sdk = new SideRoomSDK('7', {
      env: {
        ...r.env,
        fetch: async (_u, init) => {
          calls.push(init ?? {});
          return new Response(null, { status: 204 });
        },
      },
    });
    await expect(sdk.mappedFetch('https://evil.example/x')).rejects.toMatchObject({
      code: 'INVALID_ARGUMENTS',
    });
    await expect(sdk.mappedFetch('//evil.example/x')).rejects.toBeInstanceOf(SdkError);
    await sdk.mappedFetch('/api/x');
    expect(new Headers(calls[0]?.headers).has('x-sr-activity-token')).toBe(false);
  });
});
