// D-389 (wave 46): `shelfObjectUrl` hands an Activity that declares byos_read the per-object URL of ONE shelf object,
// after the member said yes (once per object and frame). Without the capability, with bad arguments, after a "no" or
// when the server refuses, the game gets an error and no URL.
import { describe, expect, it, vi } from 'vitest';
import type { Frame } from '@sideroom/embedded-app-sdk';
import { createActivityHost, type HostEnv } from '../host';
import type { ActivityInstance } from '../types';

const ORIGIN = 'https://7.activities.chat.example.org';
const HASH = 'a'.repeat(64);

function rig(capabilities: string[], answer = true) {
  const inst: ActivityInstance = {
    id: '9',
    application_id: '7',
    application_name: 'Arena 3D',
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
  const confirm = vi.fn<HostEnv['confirm']>(async () => answer);
  const mint = vi.fn(async (shelf: string, hash: string) => {
    if (shelf === '404') throw new Error('not found');
    return {
      url: `https://bucket.example.com/${shelf}/${hash}?sig=x`,
      expires_at: '2026-11-01T00:10:00Z',
    };
  });
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
  };
  const host = createActivityHost(env);
  let n = 0;
  const call = async (args: unknown) => {
    const nonce = `n${++n}`;
    await host.handle({ sr: 1, kind: 'request', nonce, cmd: 'shelfObjectUrl', args }, ORIGIN);
    return posted.find((f) => f.kind === 'response' && f.nonce === nonce) as Extract<
      Frame,
      { kind: 'response' }
    >;
  };
  return { host, posted, confirm, mint, call };
}

const ready = (r: ReturnType<typeof rig>) =>
  r.host.handle({ sr: 1, kind: 'handshake', application_id: '7' }, ORIGIN);

describe('shelfObjectUrl (byos_read)', () => {
  it('asks once per object, then returns the minted URL', async () => {
    const r = rig(['datachannel', 'byos_read']);
    await ready(r);
    const first = await r.call({ shelf_id: '123', object_hash: HASH });
    expect(first.ok && first.data).toEqual({
      url: `https://bucket.example.com/123/${HASH}?sig=x`,
      expires_at: '2026-11-01T00:10:00Z',
    });
    expect(r.confirm).toHaveBeenCalledWith({ kind: 'shelf', shelfId: '123', objectHash: HASH });
    await r.call({ shelf_id: '123', object_hash: HASH });
    expect(r.confirm).toHaveBeenCalledTimes(1);
    expect(r.mint).toHaveBeenCalledTimes(2);
    await r.call({ shelf_id: '124', object_hash: HASH });
    expect(r.confirm).toHaveBeenCalledTimes(2);
  });

  it('is refused without the capability, before asking anyone', async () => {
    const r = rig(['datachannel']);
    await ready(r);
    const res = await r.call({ shelf_id: '123', object_hash: HASH });
    expect(!res.ok && res.error.code).toBe('NOT_AUTHORIZED');
    expect(r.confirm).not.toHaveBeenCalled();
    expect(r.mint).not.toHaveBeenCalled();
  });

  it('refuses malformed references', async () => {
    const r = rig(['byos_read']);
    await ready(r);
    for (const args of [
      { shelf_id: 'abc', object_hash: HASH },
      { shelf_id: '1', object_hash: 'A'.repeat(64) },
      { shelf_id: '1', object_hash: '../x' },
      { shelf_id: '1' },
      null,
    ]) {
      const res = await r.call(args);
      expect(!res.ok && res.error.code).toBe('INVALID_ARGUMENTS');
    }
    expect(r.mint).not.toHaveBeenCalled();
  });

  it('a "no" gives USER_DENIED and no URL; a server refusal gives FAILED', async () => {
    const no = rig(['byos_read'], false);
    await ready(no);
    const res = await no.call({ shelf_id: '123', object_hash: HASH });
    expect(!res.ok && res.error.code).toBe('USER_DENIED');
    expect(no.mint).not.toHaveBeenCalled();
    const r = rig(['byos_read']);
    await ready(r);
    const gone = await r.call({ shelf_id: '404', object_hash: HASH });
    expect(!gone.ok && gone.error.code).toBe('FAILED');
  });
});
