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
