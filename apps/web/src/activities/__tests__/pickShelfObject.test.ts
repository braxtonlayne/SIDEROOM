// D-466 (wave 49): `pickShelfObject` opens the app's own shelf picker for a member and returns the ONE object they chose.
// The pick is their yes for that object (a following shelfObjectBytes shows no second dialog); cancel is null; the
// capability is required; `accept` and `max_size` are enforced again by the host; one picker at a time.
import { describe, expect, it, vi } from 'vitest';
import type { Frame, ShelfObjectPick } from '@sideroom/embedded-app-sdk';
import { createActivityHost, type HostEnv } from '../host';
import { acceptMatches, contentTypeOf, fileName, pickable } from '../shelfPick';
import type { ActivityInstance } from '../types';

const ORIGIN = 'https://7.activities.chat.example.org';
const HASH = 'a'.repeat(64);
type Res = Extract<Frame, { kind: 'response' }>;

const PICK: ShelfObjectPick = {
  shelf_id: '123',
  object_hash: HASH,
  name: 'pack.json',
  size: 900,
  content_type: 'application/json',
};

function rig(
  capabilities: string[],
  picker?: HostEnv['pickShelfObject'] | 'none',
  fetchShelfObject?: HostEnv['fetchShelfObject'],
) {
  const inst: ActivityInstance = {
    id: '9',
    application_id: '7',
    application_name: 'Sketch & Slander',
    guild_id: '2',
    channel_id: '3',
    channel_type: 'voice',
    development: false,
    started_by: '10',
    created_at_ms: 0,
    participants: [{ user_id: '10', joined_at_ms: 1 }],
    capabilities,
  };
  const posted: Frame[] = [];
  const confirm = vi.fn<HostEnv['confirm']>(async () => true);
  const mint = vi.fn(async (shelf: string, hash: string) => ({
    url: `https://bucket.example.com/${shelf}/${hash}?sig=x`,
    expires_at: '2026-11-01T00:10:00Z',
  }));
  const pick = vi.fn<NonNullable<HostEnv['pickShelfObject']>>(async () => PICK);
  const env: HostEnv = {
    origin: ORIGIN,
    applicationId: '7',
    instance: () => inst,
    channel: () => ({ id: '3', guild_id: '2', name: 'Lobby' }),
    users: () => [],
    oauth: { authorizeCode: async () => 'x', tokenInfo: async () => ({}) as never },
    confirm,
    openLink: () => undefined,
    post: (f) => posted.push(f),
    setPresence: () => undefined,
    assets: async () => [],
    shelfObjectUrl: mint,
    ...(fetchShelfObject ? { fetchShelfObject } : {}),
    ...(picker === 'none' ? {} : { pickShelfObject: picker ?? pick }),
  };
  const host = createActivityHost(env);
  let n = 0;
  const call = async (cmd: string, args: unknown): Promise<Res> => {
    const nonce = `n${++n}`;
    await host.handle({ sr: 1, kind: 'request', nonce, cmd, args }, ORIGIN);
    return posted.find((f) => f.kind === 'response' && f.nonce === nonce) as Res;
  };
  const ready = () => host.handle({ sr: 1, kind: 'handshake', application_id: '7' }, ORIGIN);
  return { host, confirm, mint, pick, call, ready };
}

const code = (r: Res) => (r.ok ? null : r.error.code);
const tick = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};

describe('pickShelfObject (D-466)', () => {
  it('hands the game the one picked object and records the member yes', async () => {
    const r = rig(['byos_read']);
    await r.ready();
    const res = await r.call('pickShelfObject', { accept: ['.json'], max_size: 1000 });
    expect(res.ok && res.data).toEqual(PICK);
    expect(r.pick).toHaveBeenCalledWith(
      { accept: ['.json'], maxSize: 1000 },
      expect.any(AbortSignal),
    );
    expect(r.confirm).not.toHaveBeenCalled();
  });

  it('a following shelfObjectBytes for that object shows no second dialog; another object still asks', async () => {
    const r = rig(['byos_read'], undefined, async () => new Uint8Array([1, 2, 3]));
    await r.ready();
    await r.call('pickShelfObject', {});
    const bytes = await r.call('shelfObjectBytes', { shelf_id: '123', object_hash: HASH });
    expect(bytes.ok && bytes.data).toEqual({ b: 'AQID', size: 3 });
    const url = await r.call('shelfObjectUrl', { shelf_id: '123', object_hash: HASH });
    expect(url.ok).toBe(true);
    expect(r.confirm).not.toHaveBeenCalled();
    await r.call('shelfObjectBytes', { shelf_id: '123', object_hash: 'b'.repeat(64) });
    expect(r.confirm).toHaveBeenCalledTimes(1);
  });

  it('cancel answers null and grants nothing', async () => {
    const r = rig(
      ['byos_read'],
      async () => null,
      async () => new Uint8Array([1]),
    );
    await r.ready();
    const res = await r.call('pickShelfObject', {});
    expect(res.ok && res.data).toBeNull();
    await r.call('shelfObjectBytes', { shelf_id: '123', object_hash: HASH });
    expect(r.confirm).toHaveBeenCalledTimes(1);
  });

  it('needs the byos_read capability and a picker, before anything opens', async () => {
    const no = rig(['datachannel']);
    await no.ready();
    expect(code(await no.call('pickShelfObject', {}))).toBe('NOT_AUTHORIZED');
    expect(no.pick).not.toHaveBeenCalled();
    const none = rig(['byos_read'], 'none');
    await none.ready();
    expect(code(await none.call('pickShelfObject', {}))).toBe('NOT_AUTHORIZED');
  });

  it('refuses malformed arguments without opening the picker', async () => {
    const r = rig(['byos_read']);
    await r.ready();
    for (const args of [
      { accept: 'json' },
      { accept: ['a b'] },
      { accept: Array.from({ length: 17 }, () => '.a') },
      { max_size: 0 },
      { max_size: 2.5 },
      { max_size: '9' },
      [],
    ]) {
      expect(code(await r.call('pickShelfObject', args)), JSON.stringify(args)).toBe(
        'INVALID_ARGUMENTS',
      );
    }
    expect(r.pick).not.toHaveBeenCalled();
  });

  it('checks the pick against accept and max_size again (a swapped picker cannot widen them)', async () => {
    const wrongKind = rig(['byos_read'], async () => ({ ...PICK, name: 'cat.png' }));
    await wrongKind.ready();
    expect(code(await wrongKind.call('pickShelfObject', { accept: ['.json'] }))).toBe('FAILED');
    const tooBig = rig(['byos_read'], async () => ({ ...PICK, size: 5000 }));
    await tooBig.ready();
    expect(code(await tooBig.call('pickShelfObject', { max_size: 1000 }))).toBe('FAILED');
    // Neither granted anything: the object still asks.
    await tooBig.call('shelfObjectUrl', { shelf_id: '123', object_hash: HASH });
    expect(tooBig.confirm).toHaveBeenCalledTimes(1);
    const bad = rig(['byos_read'], async () => ({ shelf_id: 'x' }) as never);
    await bad.ready();
    expect(code(await bad.call('pickShelfObject', {}))).toBe('FAILED');
  });

  it('hands over only the five fields of the pick', async () => {
    const r = rig(
      ['byos_read'],
      async () => ({ ...PICK, path: 'secret/dir/pack.json', bucket: 'b' }) as never,
    );
    await r.ready();
    const res = await r.call('pickShelfObject', {});
    expect(res.ok && Object.keys(res.data as object).sort()).toEqual([
      'content_type',
      'name',
      'object_hash',
      'shelf_id',
      'size',
    ]);
  });

  it('refuses a second picker while one is open, and a picker that fails is FAILED', async () => {
    let finish: (p: ShelfObjectPick | null) => void = () => undefined;
    const r = rig(['byos_read'], () => new Promise((res) => (finish = res)));
    await r.ready();
    const first = r.call('pickShelfObject', {});
    await tick();
    const second = await r.call('pickShelfObject', {});
    expect(code(second)).toBe('FAILED');
    finish(PICK);
    expect((await first).ok).toBe(true);
    const again = rig(['byos_read'], async () => {
      throw new Error('boom');
    });
    await again.ready();
    expect(code(await again.call('pickShelfObject', {}))).toBe('FAILED');
  });

  it('closing the frame aborts an open picker and the game gets null', async () => {
    let signal: AbortSignal | undefined;
    const r = rig(['byos_read'], (_req, s) => {
      signal = s;
      return new Promise((res) => s.addEventListener('abort', () => res(null)));
    });
    await r.ready();
    const pending = r.call('pickShelfObject', {});
    await tick();
    r.host.dispose();
    expect(signal?.aborted).toBe(true);
    const res = await pending;
    expect(res.ok && res.data).toBeNull();
  });
});

describe('picker filters', () => {
  it('names the media type from the extension', () => {
    expect(contentTypeOf('Pack.JSON')).toBe('application/json');
    expect(contentTypeOf('a.txt')).toBe('text/plain');
    expect(contentTypeOf('noext')).toBe('application/octet-stream');
    expect(contentTypeOf('.hidden')).toBe('application/octet-stream');
    expect(fileName('a/b/c.json')).toBe('c.json');
  });

  it('matches extensions, exact media types and families; empty passes all', () => {
    expect(acceptMatches('a.json', undefined)).toBe(true);
    expect(acceptMatches('a.json', [])).toBe(true);
    expect(acceptMatches('a.json', ['.json'])).toBe(true);
    expect(acceptMatches('a.JSON', ['json'])).toBe(true);
    expect(acceptMatches('a.json', ['.txt'])).toBe(false);
    expect(acceptMatches('a.txt', ['text/plain'])).toBe(true);
    expect(acceptMatches('a.txt', ['text/*'])).toBe(true);
    expect(acceptMatches('a.png', ['text/*'])).toBe(false);
    expect(acceptMatches('a.png', ['image/*', '.json'])).toBe(true);
    expect(acceptMatches('noext', ['.json'])).toBe(false);
  });

  it('applies the size cap inclusively', () => {
    expect(pickable('a.json', 100, { maxSize: 100 })).toBe(true);
    expect(pickable('a.json', 101, { maxSize: 100 })).toBe(false);
    expect(pickable('a.json', 10 ** 9, { accept: [], maxSize: null })).toBe(true);
  });
});
