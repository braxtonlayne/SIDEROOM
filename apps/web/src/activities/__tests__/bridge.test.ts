// ARC-SBX-001: the validated postMessage bridge of the hardened sandbox. The frame has an opaque origin, so
// these tests pin what the host believes (origin "null" only), what it refuses before any command runs and what
// the storage and saveFile commands enforce.
import { describe, expect, it, vi } from 'vitest';
import { RPC_ERRORS, type Frame } from '@sideroom/embedded-app-sdk';
import { createActivityHost, OPAQUE_ORIGIN, requestProblem, type HostEnv } from '../host';
import { createBridgeStorage, storageKeyFor } from '../bridgeStorage';
import type { ActivityInstance } from '../types';
import { ACTIVITY_SANDBOX } from '../ActivityFrame';

const ORIGIN = 'https://7.activities.chat.example.org';
const base: ActivityInstance = {
  id: '9',
  application_id: '7',
  application_name: 'Chess',
  guild_id: '2',
  channel_id: '3',
  channel_type: 'voice',
  development: false,
  started_by: '10',
  created_at_ms: 0,
  participants: [{ user_id: '10', joined_at_ms: 1 }],
};

function rig(capabilities: string[] = ['storage'], over: Partial<HostEnv> = {}) {
  const posted: Frame[] = [];
  const kv: Record<string, string> = { a: '1' };
  const saved: { name: string; mime: string; size: number }[] = [];
  const confirm = vi.fn<HostEnv['confirm']>(async () => true);
  const clock = { t: 1_000 };
  const env: HostEnv = {
    origin: ORIGIN,
    senderOrigin: OPAQUE_ORIGIN,
    applicationId: '7',
    instance: () => ({ ...base, capabilities }),
    channel: () => ({ id: '3', guild_id: '2', name: 'Lobby' }),
    users: () => [],
    oauth: {
      authorizeCode: async () => 'CODE',
      tokenInfo: async () => {
        throw new Error('unused');
      },
    },
    confirm,
    openLink: () => undefined,
    post: (f) => posted.push(f),
    setPresence: () => undefined,
    assets: async () => [],
    storage: {
      all: () => ({ ...kv }),
      set: (k, v) => {
        kv[k] = v;
      },
      remove: (k) => {
        delete kv[k];
      },
    },
    saveFile: (name, mime, bytes) => saved.push({ name, mime, size: bytes.length }),
    now: () => clock.t,
    ...over,
  };
  const host = createActivityHost(env);
  const send = (data: unknown, origin = OPAQUE_ORIGIN) => host.handle(data, origin);
  const req = (nonce: string, cmd: string, args?: unknown) =>
    send({ sr: 1, kind: 'request', nonce, cmd, args });
  const last = () => posted[posted.length - 1] as Extract<Frame, { kind: 'response' }>;
  return { host, posted, send, req, last, kv, saved, confirm, clock };
}

const handshake = { sr: 1, kind: 'handshake', application_id: '7' };

describe('the sandbox attribute', () => {
  it('is scripts and pointer lock, never the Activity host origin', () => {
    expect(ACTIVITY_SANDBOX.split(' ').sort()).toEqual(['allow-pointer-lock', 'allow-scripts']);
  });
});

describe('who the bridge believes', () => {
  it('answers the opaque origin of the sandboxed frame and nothing else', async () => {
    const r = rig();
    await r.send(handshake, ORIGIN);
    await r.send(handshake, 'https://evil.example');
    await r.send(handshake, '');
    expect(r.posted).toHaveLength(0);
    await r.send(handshake, 'null');
    expect(r.posted).toHaveLength(1);
    expect(r.posted[0]).toMatchObject({ kind: 'ready' });
  });

  it('keeps the old behaviour for a frame that has its origin (no senderOrigin)', async () => {
    const r = rig(['storage'], { senderOrigin: undefined });
    await r.send(handshake, 'null');
    expect(r.posted).toHaveLength(0);
    await r.send(handshake, ORIGIN);
    expect(r.posted).toHaveLength(1);
  });

  it('drops a handshake for another application and malformed frames', async () => {
    const r = rig();
    await r.send({ sr: 1, kind: 'handshake', application_id: '8' });
    await r.send({ sr: 2, kind: 'handshake', application_id: '7' });
    await r.send('handshake');
    await r.send(null);
    await r.send({ sr: 1, kind: 'response', nonce: 'x', ok: true });
    expect(r.posted).toHaveLength(0);
  });
});

describe('request validation', () => {
  it('refuses oversized envelopes and arguments before any command runs', async () => {
    const r = rig();
    await r.send(handshake);
    await r.req('n'.repeat(65), 'getChannel');
    expect(r.last()).toMatchObject({ ok: false, error: { code: RPC_ERRORS.invalidArguments } });
    await r.req('a', 'c'.repeat(65));
    expect(r.last()).toMatchObject({ ok: false, error: { code: RPC_ERRORS.invalidArguments } });
    await r.req('b', 'getChannel', { x: 'y'.repeat(13 * 1024 * 1024) });
    expect(r.last()).toMatchObject({ ok: false, error: { code: RPC_ERRORS.invalidArguments } });
    await r.req('c', 'getChannel');
    expect(r.last()).toMatchObject({ ok: true });
  });

  it('requestProblem is strict about plain data', () => {
    expect(requestProblem({ nonce: 'a', cmd: 'x' })).toBeNull();
    expect(requestProblem({ nonce: '', cmd: 'x' })).not.toBeNull();
    expect(requestProblem({ nonce: 'a', cmd: '' })).not.toBeNull();
    expect(requestProblem({ nonce: 'a', cmd: 'x', args: { f: () => 1 } })).not.toBeNull();
    expect(
      requestProblem({ nonce: 'a', cmd: 'x', args: JSON.parse('{"__proto__": {"admin": true}}') }),
    ).not.toBeNull();
    expect(requestProblem({ nonce: 'a', cmd: 'x', args: { deep: [[[['ok']]]] } })).toBeNull();
    expect(
      requestProblem({ nonce: 'a', cmd: 'x', args: Array.from({ length: 30_000 }, () => 1) }),
    ).not.toBeNull();
  });

  it('answers a flood with RATE_LIMITED and recovers after the window', async () => {
    const r = rig();
    await r.send(handshake);
    for (let i = 0; i < 4000; i++) await r.req(`n${i}`, 'getChannel');
    expect(r.last()).toMatchObject({ ok: true });
    await r.req('over', 'getChannel');
    expect(r.last()).toMatchObject({ ok: false, error: { code: RPC_ERRORS.rateLimited } });
    r.clock.t += 11_000;
    await r.req('later', 'getChannel');
    expect(r.last()).toMatchObject({ ok: true });
  });

  it('refuses unknown commands', async () => {
    const r = rig();
    await r.send(handshake);
    await r.req('a', 'fetch', { url: 'https://evil.example' });
    expect(r.last()).toMatchObject({ ok: false, error: { code: RPC_ERRORS.unknownCommand } });
  });
});

describe('storage over the bridge', () => {
  it('sends the stored values in ready only with the capability', async () => {
    const r = rig(['storage']);
    await r.send(handshake);
    expect(r.posted[0]).toMatchObject({ kind: 'ready', config: { storage: { a: '1' } } });
    const none = rig([]);
    await none.send(handshake);
    expect((none.posted[0] as { config: Record<string, unknown> }).config.storage).toBeUndefined();
  });

  it('stores, replaces and removes keys', async () => {
    const r = rig();
    await r.send(handshake);
    await r.req('1', 'storageSet', { key: 'prefs', value: '{"a":1}' });
    expect(r.last()).toMatchObject({ ok: true });
    expect(r.kv.prefs).toBe('{"a":1}');
    await r.req('2', 'storageRemove', { key: 'prefs' });
    expect(r.kv.prefs).toBeUndefined();
  });

  it('refuses without the capability, with bad keys and over the quota', async () => {
    const off = rig([]);
    await off.send(handshake);
    await off.req('1', 'storageSet', { key: 'k', value: 'v' });
    expect(off.last()).toMatchObject({ ok: false, error: { code: RPC_ERRORS.notAuthorized } });

    const r = rig();
    await r.send(handshake);
    for (const key of ['', 'has space', 'a/b', 'x'.repeat(129), 5]) {
      await r.req('k', 'storageSet', { key, value: 'v' });
      expect(r.last()).toMatchObject({ ok: false, error: { code: RPC_ERRORS.invalidArguments } });
    }
    await r.req('v', 'storageSet', { key: 'big', value: 'x'.repeat(32 * 1024 + 1) });
    expect(r.last()).toMatchObject({ ok: false, error: { code: RPC_ERRORS.invalidArguments } });
    // 4 values of 32 KiB fill the 128 KiB total; a fifth is over.
    for (let i = 0; i < 4; i++) {
      await r.req(`f${i}`, 'storageSet', { key: `f${i}`, value: 'x'.repeat(32 * 1024 - (i === 0 ? 1 : 0)) });
      expect(r.last()).toMatchObject({ ok: true });
    }
    await r.req('over', 'storageSet', { key: 'over', value: 'xx' });
    expect(r.last()).toMatchObject({ ok: false, error: { code: RPC_ERRORS.failed } });
  });

  it('caps the number of keys', async () => {
    const r = rig();
    await r.send(handshake);
    for (let i = 0; i < 63; i++) await r.req(`s${i}`, 'storageSet', { key: `k${i}`, value: '1' });
    expect(r.last()).toMatchObject({ ok: true });
    await r.req('x', 'storageSet', { key: 'extra', value: '1' });
    expect(r.last()).toMatchObject({ ok: false });
    await r.req('y', 'storageSet', { key: 'k1', value: '2' });
    expect(r.last()).toMatchObject({ ok: true });
  });
});

describe('saveFile over the bridge', () => {
  const png = btoa('PNGDATA');
  it('asks the person, then hands the bytes to the page', async () => {
    const r = rig();
    await r.send(handshake);
    await r.req('1', 'saveFile', { name: 'board.png', mime: 'image/png', b: png });
    expect(r.confirm).toHaveBeenCalledWith({ kind: 'save', name: 'board.png', mime: 'image/png', size: 7 });
    expect(r.last()).toMatchObject({ ok: true, data: { saved: true } });
    expect(r.saved).toEqual([{ name: 'board.png', mime: 'image/png', size: 7 }]);
  });

  it('saves nothing when the person declines', async () => {
    const r = rig();
    r.confirm.mockResolvedValueOnce(false);
    await r.send(handshake);
    await r.req('1', 'saveFile', { name: 'board.png', mime: 'image/png', b: png });
    expect(r.last()).toMatchObject({ ok: true, data: { saved: false } });
    expect(r.saved).toHaveLength(0);
  });

  it('refuses scripts, odd names and bad base64', async () => {
    const r = rig();
    await r.send(handshake);
    for (const args of [
      { name: 'x.html', mime: 'text/html', b: png },
      { name: 'x.png', mime: 'image/svg+xml', b: png },
      { name: '../x.png', mime: 'image/png', b: png },
      { name: 'a/b.png', mime: 'image/png', b: png },
      { name: '.hidden', mime: 'image/png', b: png },
      { name: 'x.png', mime: 'image/png', b: '***' },
      { name: 'x.png', mime: 'image/png' },
    ]) {
      await r.req('n', 'saveFile', args);
      expect(r.last()).toMatchObject({ ok: false, error: { code: RPC_ERRORS.invalidArguments } });
    }
    expect(r.saved).toHaveLength(0);
  });
});

describe('createBridgeStorage', () => {
  const mem = () => {
    const m = new Map<string, string>();
    return {
      m,
      s: {
        getItem: (k: string) => m.get(k) ?? null,
        setItem: (k: string, v: string) => void m.set(k, v),
        removeItem: (k: string) => void m.delete(k),
      },
    };
  };

  it('keeps one record per member and application', () => {
    const { m, s } = mem();
    const a = createBridgeStorage('10', '7', s);
    a.set('k', 'v');
    expect(m.get(storageKeyFor('10', '7'))).toBe('{"k":"v"}');
    expect(createBridgeStorage('10', '7', s).all()).toEqual({ k: 'v' });
    expect(createBridgeStorage('11', '7', s).all()).toEqual({});
    expect(createBridgeStorage('10', '8', s).all()).toEqual({});
    a.remove('k');
    expect(m.has(storageKeyFor('10', '7'))).toBe(false);
  });

  it('survives broken or throwing storage', () => {
    const broken = { getItem: () => '{not json', setItem: () => undefined, removeItem: () => undefined };
    expect(createBridgeStorage('1', '1', broken).all()).toEqual({});
    const throws = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('full');
      },
      removeItem: () => undefined,
    };
    const st = createBridgeStorage('1', '1', throws);
    st.set('a', 'b');
    expect(st.all()).toEqual({ a: 'b' });
    expect(createBridgeStorage('1', '1', null).all()).toEqual({});
  });
});
