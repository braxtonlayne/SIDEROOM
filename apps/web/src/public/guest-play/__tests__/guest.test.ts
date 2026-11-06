import {
  COMMANDS,
  EVENTS,
  PROTOCOL_VERSION,
  RPC_ERRORS,
  b64,
  type Frame,
} from '@sideroom/embedded-app-sdk';
import { requestProblem as memberRequestProblem } from '../../../activities/host';
import { GUEST_COMMANDS, GUEST_EVENTS, guestCapabilities, runnerRefusal } from '../capabilities';
import type { GuestMember } from '../contract';
import { createGuestHost, OPAQUE_ORIGIN, requestProblem, type GuestLane } from '../guestHost';

const APP = '777';
const roster: GuestMember[] = [
  { id: '10', nickname: 'Mellow Otter 42' },
  { id: '99', nickname: 'Host', host: true },
];

type Response = Extract<Frame, { kind: 'response' }>;

function rig(declared: string[] = ['guest_play', 'lobby_channel', 'datachannel', 'storage']) {
  const posted: Frame[] = [];
  const sent: Uint8Array[] = [];
  let push: ((from: string, b: Uint8Array) => void) | null = null;
  const lane: GuestLane = {
    send: async (b) => {
      sent.push(b);
      return true;
    },
    onMessage: (cb) => {
      push = cb;
      return () => {
        push = null;
      };
    },
  };
  const host = createGuestHost({
    applicationId: APP,
    declared,
    self: () => ({ id: '10', callsign: 'Mellow Otter 42' }),
    roster: () => roster,
    lane,
    post: (f) => posted.push(f),
  });
  let n = 0;
  const call = async (cmd: string, args?: unknown): Promise<Response> => {
    const nonce = `n${++n}`;
    await host.handle({ sr: 1, kind: 'request', nonce, cmd, args }, OPAQUE_ORIGIN);
    return posted.find((f) => f.kind === 'response' && f.nonce === nonce) as Response;
  };
  const handshake = () =>
    host.handle({ sr: 1, kind: 'handshake', application_id: APP }, OPAQUE_ORIGIN);
  return { host, posted, sent, call, handshake, push: (f: string, b: Uint8Array) => push?.(f, b) };
}

const errCode = (r: Response) => (r.ok ? null : r.error.code);

describe('the guest capability set is an allow-list', () => {
  it('names exactly four commands and three events', () => {
    expect([...GUEST_COMMANDS].sort()).toEqual([
      'dataSend',
      'getInstanceConnectedParticipants',
      'subscribe',
      'unsubscribe',
    ]);
    expect([...GUEST_EVENTS].sort()).toEqual([
      'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE',
      'DATA_MESSAGE',
      'HOST_CHANGED',
    ]);
  });

  it('is a subset of what the protocol knows, so nothing is invented for guests', () => {
    for (const c of GUEST_COMMANDS) expect(COMMANDS).toContain(c);
    for (const e of GUEST_EVENTS) expect(EVENTS).toContain(e);
  });

  it('refuses EVERY other protocol command with NOT_AUTHORIZED and sends nothing', async () => {
    const r = rig();
    await r.handshake();
    for (const cmd of COMMANDS.filter((c) => !(GUEST_COMMANDS as readonly string[]).includes(c))) {
      const res = await r.call(cmd, {
        scope: ['identify'],
        url: 'https://x.test',
        key: 'k',
        value: 'v',
      });
      expect(res.ok, cmd).toBe(false);
      expect(errCode(res), cmd).toBe(RPC_ERRORS.notAuthorized);
    }
    expect(r.sent).toHaveLength(0);
  });

  it('refuses pickShelfObject (D-466) like the other shelf commands and sends nothing', async () => {
    expect(GUEST_COMMANDS).not.toContain('pickShelfObject');
    const r = rig();
    await r.handshake();
    const res = await r.call('pickShelfObject', { accept: ['.json'], max_size: 1000 });
    expect(errCode(res)).toBe(RPC_ERRORS.notAuthorized);
    expect(r.sent).toHaveLength(0);
  });

  it('answers a command the protocol does not know with UNKNOWN_COMMAND, so a future command stays closed', async () => {
    const r = rig();
    await r.handshake();
    expect(errCode(await r.call('getGuildMembers'))).toBe(RPC_ERRORS.unknownCommand);
  });

  it.each(['VOICE_STATE_UPDATE', 'SPEAKING_START', 'SPEAKING_STOP'])(
    'does not let a game subscribe to %s (the call is not the game)',
    async (event) => {
      const r = rig();
      await r.handshake();
      expect(errCode(await r.call('subscribe', { event }))).toBe(RPC_ERRORS.notAuthorized);
    },
  );
});

describe('the guest bridge', () => {
  it('tells the game a guest id, a callsign and placeholders, nothing about a server', async () => {
    const r = rig();
    await r.handshake();
    const ready = r.posted.find((f) => f.kind === 'ready') as Extract<Frame, { kind: 'ready' }>;
    expect(ready.config).toEqual({
      application_id: APP,
      instance_id: 'lobby',
      channel_id: '0',
      guild_id: '0',
      user_id: '10',
      host_id: '99',
      // the game's declaration cut to the allow-list: no storage, no guest_play flag, no lobby flag
      capabilities: ['datachannel'],
      guest: true,
      callsign: 'Mellow Otter 42',
    });
  });

  it('lists lobby nicknames as the participants and nothing else about them', async () => {
    const r = rig();
    await r.handshake();
    const res = await r.call('getInstanceConnectedParticipants');
    const p = (res.ok ? (res.data as { participants: Record<string, unknown>[] }) : null)!
      .participants;
    expect(p.map((x) => x.global_name)).toEqual(['Mellow Otter 42', 'Host']);
    expect(p.every((x) => x.avatar === null && !('avatar_url' in x))).toBe(true);
  });

  it('sends data through the lane and delivers the lane to a subscribed game with the stamped sender', async () => {
    const r = rig();
    await r.handshake();
    await r.call('subscribe', { event: 'DATA_MESSAGE' });
    const sent = await r.call('dataSend', { b: b64(new Uint8Array([1, 2, 3])) });
    expect(sent.ok && sent.data).toEqual({ sent: true });
    expect(Array.from(r.sent[0]!)).toEqual([1, 2, 3]);
    r.push('99', new Uint8Array([9]));
    const ev = r.posted.find((f) => f.kind === 'event') as Extract<Frame, { kind: 'event' }>;
    expect(ev).toMatchObject({
      event: 'DATA_MESSAGE',
      data: { from: '99', b: b64(new Uint8Array([9])) },
    });
  });

  it('does not deliver events before the game subscribed, and none after dispose', async () => {
    const r = rig();
    await r.handshake();
    r.push('99', new Uint8Array([1]));
    expect(r.posted.some((f) => f.kind === 'event')).toBe(false);
    await r.call('subscribe', { event: 'DATA_MESSAGE' });
    r.host.dispose();
    r.push('99', new Uint8Array([2]));
    expect(r.posted.some((f) => f.kind === 'event')).toBe(false);
  });

  it('refuses dataSend when the game never declared datachannel', async () => {
    const r = rig(['guest_play']);
    await r.handshake();
    const res = await r.call('dataSend', { b: b64(new Uint8Array([1])) });
    expect(errCode(res)).toBe(RPC_ERRORS.notAuthorized);
    expect(r.sent).toHaveLength(0);
  });

  it('rejects oversized and malformed payloads before the lane sees them', async () => {
    const r = rig();
    await r.handshake();
    expect((await r.call('dataSend', { b: b64(new Uint8Array(20_000)) })).ok).toBe(false);
    expect((await r.call('dataSend', { b: '***' })).ok).toBe(false);
    expect((await r.call('dataSend', {})).ok).toBe(false);
    expect(r.sent).toHaveLength(0);
  });

  it('ignores frames from any other origin and requests before ready()', async () => {
    const r = rig();
    await r.host.handle({ sr: 1, kind: 'handshake', application_id: APP }, 'https://evil.test');
    expect(r.posted).toHaveLength(0);
    const early = await r.call('getInstanceConnectedParticipants');
    expect(errCode(early)).toBe(RPC_ERRORS.notAuthorized);
    await r.host.handle({ sr: 1, kind: 'handshake', application_id: 'other' }, OPAQUE_ORIGIN);
    expect(r.posted.some((f) => f.kind === 'ready')).toBe(false);
  });

  it('notifies a subscribed game when the roster changes', async () => {
    const r = rig();
    await r.handshake();
    await r.call('subscribe', { event: 'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE' });
    r.host.rosterChanged();
    expect(
      r.posted.some(
        (f) => f.kind === 'event' && f.event === 'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE',
      ),
    ).toBe(true);
  });

  it('answers a flood with RATE_LIMITED', async () => {
    const r = rig();
    await r.handshake();
    let limited = false;
    for (let i = 0; i < 4100 && !limited; i++) {
      limited =
        errCode(await r.call('getInstanceConnectedParticipants')) === RPC_ERRORS.rateLimited;
    }
    expect(limited).toBe(true);
  });
});

describe('request shape checks agree with the member bridge', () => {
  const cases = [
    { nonce: '', cmd: 'x' },
    { nonce: 'n', cmd: '' },
    { nonce: 'n'.repeat(65), cmd: 'x' },
    { nonce: 'n', cmd: 'x', args: JSON.parse('{"__proto__":1}') },
    { nonce: 'n', cmd: 'x', args: { a: [1, 'b', { c: true }] } },
    { nonce: 'n', cmd: 'x', args: { f: () => 1 } },
  ];
  it.each(cases.map((c, i) => [i, c] as const))('case %i', (_i, c) => {
    expect(requestProblem(c)).toBe(memberRequestProblem(c));
  });
});

describe('what the runner will start', () => {
  it('needs guest_play and refuses ingame_voice (D-399: validates, never runs)', () => {
    expect(runnerRefusal(['guest_play', 'datachannel'])).toBeNull();
    expect(runnerRefusal(['guest_play', 'ingame_voice'])).toBe('ingame_voice');
    expect(runnerRefusal(['ingame_voice'])).toBe('ingame_voice');
    expect(runnerRefusal(['datachannel'])).toBe('not_guest_play');
    expect(runnerRefusal([])).toBe('not_guest_play');
  });

  it('cuts a declaration to the guest allow-list', () => {
    expect(guestCapabilities(['storage', 'datachannel', 'byos_read', 'ingame_voice'])).toEqual([
      'datachannel',
    ]);
    expect(PROTOCOL_VERSION).toBe(1);
  });
});

describe('HOST_CHANGED for a guest game (D-388)', () => {
  it('forwards a new host the member bridge named, to a game that subscribed', async () => {
    const posted: Frame[] = [];
    let move: ((h: string, p: string | null) => void) | null = null;
    const host = createGuestHost({
      applicationId: APP,
      declared: ['guest_play', 'lobby_channel', 'datachannel', 'host_handoff'],
      self: () => ({ id: 'g2', callsign: 'Mellow Otter 42' }),
      roster: () => roster,
      hostId: () => '5550001',
      onHostChange: (cb) => {
        move = cb;
        return () => (move = null);
      },
      lane: { send: async () => true, onMessage: () => () => undefined },
      post: (f) => posted.push(f),
    });
    await host.handle({ sr: 1, kind: 'handshake', application_id: APP }, OPAQUE_ORIGIN);
    const ready = posted[0] as Extract<Frame, { kind: 'ready' }>;
    expect(ready.config.host_id).toBe('5550001');
    // host_handoff is not in the guest manifest set: a guest game is never told it may host.
    expect(ready.config.capabilities).toEqual(['datachannel']);
    await host.handle(
      { sr: 1, kind: 'request', nonce: 'n1', cmd: 'subscribe', args: { event: 'HOST_CHANGED' } },
      OPAQUE_ORIGIN,
    );
    move!('5550002', '5550001');
    move!('5550002', '5550001');
    const ev = posted.filter((f) => f.kind === 'event');
    expect(ev).toEqual([
      {
        sr: 1,
        kind: 'event',
        event: 'HOST_CHANGED',
        data: { host_id: '5550002', previous: '5550001' },
      },
    ]);
    host.dispose();
    expect(move).toBeNull();
  });
});
