import { describe, expect, it, vi } from 'vitest';
import type { AuthenticateResult, Frame } from '@sideroom/embedded-app-sdk';
import { createActivityHost, safeExternalUrl, type HostEnv, type VoicePerson } from '../host';
import type { ActivityInstance } from '../types';

const ORIGIN = 'https://7.activities.chat.example.org';
const instance: ActivityInstance = {
  id: '9',
  application_id: '7',
  application_name: 'Chess',
  guild_id: '2',
  channel_id: '3',
  channel_type: 'voice',
  development: false,
  started_by: '10',
  created_at_ms: 0,
  participants: [
    { user_id: '10', joined_at_ms: 1 },
    { user_id: '11', joined_at_ms: 2 },
  ],
};

function rig() {
  const posted: Frame[] = [];
  const authorizeCode = vi.fn<(a: string, s: string[], r: string) => Promise<string>>(
    async () => 'CODE',
  );
  const tokenInfo = vi.fn(async (t: string): Promise<AuthenticateResult> => ({
    access_token: t,
    user: { id: '10', username: 'ann', global_name: 'Ann', avatar: null },
    scopes: ['identify'],
    expires: '2030-01-01T00:00:00Z',
    application: { id: '7', name: 'Chess' },
  }));
  const confirm = vi.fn<HostEnv['confirm']>(async () => true);
  const openLink = vi.fn();
  const setPresence = vi.fn<HostEnv['setPresence']>();
  const clock = { t: 1_000 };
  const env: HostEnv = {
    origin: ORIGIN,
    applicationId: '7',
    instance: () => instance,
    channel: () => ({ id: '3', guild_id: '2', name: 'Lobby' }),
    users: (ids) =>
      ids.flatMap((id) =>
        id === '10' ? [{ id, username: 'ann', global_name: 'Ann', avatar: null }] : [],
      ),
    oauth: { authorizeCode, tokenInfo },
    confirm,
    openLink,
    post: (f) => posted.push(f),
    setPresence,
    assets: async () => [
      { id: '900', name: 'logo' },
      { id: '901', name: 'icon' },
    ],
    now: () => clock.t,
  };
  const host = createActivityHost(env);
  const send = (data: unknown, origin = ORIGIN) => host.handle(data, origin);
  const req = (nonce: string, cmd: string, args?: unknown) =>
    send({ sr: 1, kind: 'request', nonce, cmd, args });
  const last = () => posted[posted.length - 1] as Extract<Frame, { kind: 'response' }>;
  return {
    host,
    posted,
    send,
    req,
    last,
    authorizeCode,
    tokenInfo,
    confirm,
    openLink,
    setPresence,
    clock,
  };
}

const handshake = { sr: 1, kind: 'handshake', application_id: '7' };

describe('activity host', () => {
  it('answers the handshake with the instance and refuses requests before it', async () => {
    const r = rig();
    await r.req('a', 'getChannel');
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'NOT_AUTHORIZED' } });
    await r.send(handshake);
    expect(r.posted.at(-1)).toEqual({
      sr: 1,
      kind: 'ready',
      config: { application_id: '7', instance_id: '9', channel_id: '3', guild_id: '2' },
    });
    await r.req('b', 'getChannel');
    expect(r.last()).toMatchObject({ nonce: 'b', ok: true, data: { name: 'Lobby' } });
  });

  it('ignores other origins and a handshake for another application', async () => {
    const r = rig();
    await r.send(handshake, 'https://evil.example');
    await r.send({ sr: 1, kind: 'handshake', application_id: '8' });
    await r.send('hello');
    expect(r.posted).toHaveLength(0);
  });

  it('lists the participants the user store knows', async () => {
    const r = rig();
    await r.send(handshake);
    await r.req('p', 'getInstanceConnectedParticipants');
    expect(r.last()).toMatchObject({
      ok: true,
      data: { participants: [{ id: '10', username: 'ann' }] },
    });
  });

  it('authorize asks first, limits scopes, and returns the code', async () => {
    const r = rig();
    await r.send(handshake);
    await r.req('1', 'authorize', { scope: ['identify', 'identify'] });
    expect(r.confirm).toHaveBeenCalledWith({ kind: 'authorize', scopes: ['identify'] });
    expect(r.authorizeCode).toHaveBeenCalledWith('7', ['identify'], `${ORIGIN}/`);
    expect(r.last()).toMatchObject({ ok: true, data: { code: 'CODE' } });

    await r.req('2', 'authorize', { scope: ['email'] });
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'INVALID_ARGUMENTS' } });
    await r.req('3', 'authorize', { scope: [] });
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'INVALID_ARGUMENTS' } });
    expect(r.authorizeCode).toHaveBeenCalledTimes(1);

    r.confirm.mockResolvedValueOnce(false);
    await r.req('4', 'authorize', { scope: ['identify'] });
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'USER_DENIED' } });
    expect(r.authorizeCode).toHaveBeenCalledTimes(1);
  });

  it('authenticate accepts only tokens of this application', async () => {
    const r = rig();
    await r.send(handshake);
    await r.req('1', 'authenticate', { access_token: 'tok' });
    expect(r.last()).toMatchObject({ ok: true, data: { user: { id: '10' } } });
    await r.req('2', 'authenticate', {});
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'INVALID_ARGUMENTS' } });
    r.tokenInfo.mockResolvedValueOnce({
      access_token: 'x',
      user: null,
      scopes: [],
      expires: '',
      application: { id: '99', name: 'Other' },
    });
    await r.req('3', 'authenticate', { access_token: 'x' });
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'NOT_AUTHORIZED' } });
  });

  it('a failing server call is a generic failure, not a leak', async () => {
    const r = rig();
    r.authorizeCode.mockRejectedValueOnce(new Error('secret detail'));
    await r.send(handshake);
    await r.req('1', 'authorize', { scope: ['identify'] });
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'FAILED' } });
    expect(JSON.stringify(r.last())).not.toContain('secret detail');
  });

  it('opens http(s) links only after confirmation', async () => {
    const r = rig();
    await r.send(handshake);
    await r.req('1', 'openExternalLink', { url: 'https://example.com/a' });
    expect(r.openLink).toHaveBeenCalledWith('https://example.com/a');
    expect(r.last()).toMatchObject({ ok: true, data: { opened: true } });
    for (const bad of ['javascript:alert(1)', 'data:text/html,hi', 'file:///etc/passwd', 'nope']) {
      await r.req('x', 'openExternalLink', { url: bad });
      expect(r.last()).toMatchObject({ ok: false, error: { code: 'INVALID_ARGUMENTS' } });
    }
    r.confirm.mockResolvedValueOnce(false);
    await r.req('2', 'openExternalLink', { url: 'https://example.com/b' });
    expect(r.last()).toMatchObject({ ok: true, data: { opened: false } });
    expect(r.openLink).toHaveBeenCalledTimes(1);
    expect(safeExternalUrl(5)).toBeNull();
  });

  it('pushes participant updates only to subscribers', async () => {
    const r = rig();
    await r.send(handshake);
    r.host.participantsChanged();
    expect(r.posted.filter((f) => f.kind === 'event')).toHaveLength(0);
    await r.req('1', 'subscribe', { event: 'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE' });
    r.host.participantsChanged();
    expect(r.posted.at(-1)).toMatchObject({
      kind: 'event',
      event: 'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE',
      data: { participants: [{ id: '10' }] },
    });
    await r.req('2', 'unsubscribe', { event: 'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE' });
    const n = r.posted.length;
    r.host.participantsChanged();
    expect(r.posted).toHaveLength(n);
    await r.req('3', 'subscribe', { event: 'NOPE' });
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'INVALID_ARGUMENTS' } });
  });

  it('rejects unknown commands', async () => {
    const r = rig();
    await r.send(handshake);
    await r.req('1', 'nope', {});
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'UNKNOWN_COMMAND' } });
  });

  it('setActivity writes a Playing presence for this application and clears it with null', async () => {
    const r = rig();
    await r.send(handshake);
    await r.req('1', 'setActivity', {
      activity: {
        details: ' Round 2 ',
        state: 'In a match',
        timestamps: { start: 5, end: 9 },
        party: { id: 'room-1' },
        assets: { large_image: 'logo', large_text: 'Chess', small_image: 'nope', ignored: 1 },
        extra: 'dropped',
      },
    });
    expect(r.setPresence).toHaveBeenCalledWith({
      type: 0,
      name: 'Chess',
      application_id: '7',
      platform: 'web',
      details: 'Round 2',
      state: 'In a match',
      timestamps: { start: 5, end: 9 },
      party: { id: 'room-1' },
      // The name resolves to the asset's id; a name nobody uploaded is dropped.
      assets: { large_image: '900', large_text: 'Chess' },
    });
    expect(r.last()).toMatchObject({
      ok: true,
      data: { details: 'Round 2', assets: { large_image: '900', large_text: 'Chess' } },
    });
    r.clock.t += 1;
    await r.req('2', 'setActivity', { activity: null });
    expect(r.setPresence).toHaveBeenLastCalledWith(null);
    expect(r.last()).toMatchObject({ ok: true, data: null });
  });

  it('setActivity validates its fields and is rate limited', async () => {
    const r = rig();
    await r.send(handshake);
    for (const activity of [
      { details: 5 },
      { details: 'x'.repeat(129) },
      { state: 'bad\u0007' },
      { timestamps: { start: -1 } },
      { timestamps: { start: 1.5 } },
      { party: 'x' },
      'text',
    ]) {
      await r.req('v', 'setActivity', { activity });
      expect(r.last(), JSON.stringify(activity)).toMatchObject({
        ok: false,
        error: { code: 'INVALID_ARGUMENTS' },
      });
    }
    expect(r.setPresence).not.toHaveBeenCalled();
    // Five calls in the window, the sixth is refused; later it is allowed again.
    for (let i = 0; i < 5; i++) {
      await r.req(`r${i}`, 'setActivity', { activity: { details: `n${i}` } });
      expect(r.last()).toMatchObject({ ok: true });
    }
    await r.req('r5', 'setActivity', { activity: { details: 'n5' } });
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'RATE_LIMITED' } });
    expect(r.setPresence).toHaveBeenCalledTimes(5);
    r.clock.t += 21_000;
    await r.req('r6', 'setActivity', { activity: { details: 'n6' } });
    expect(r.last()).toMatchObject({ ok: true });
  });

  it('dispose clears the presence the Activity set, and only then', async () => {
    const r = rig();
    await r.send(handshake);
    r.host.dispose();
    expect(r.setPresence).not.toHaveBeenCalled();
    const s = rig();
    await s.send(handshake);
    await s.req('1', 'setActivity', { activity: { details: 'x' } });
    s.host.dispose();
    expect(s.setPresence).toHaveBeenLastCalledWith(null);
    s.host.dispose();
    expect(s.setPresence).toHaveBeenCalledTimes(2);
  });

  const person = (userId: string, over: Partial<VoicePerson> = {}): VoicePerson => ({
    userId,
    speaking: false,
    muted: false,
    deafened: false,
    serverMuted: false,
    serverDeafened: false,
    suppressed: false,
    ...over,
  });

  it('forwards voice-state and speaking events of the instance channel to subscribers', async () => {
    const r = rig();
    await r.send(handshake);
    const events = () =>
      r.posted.filter((f): f is Extract<Frame, { kind: 'event' }> => f.kind === 'event');
    // Baseline: the first snapshot is not news, and nothing is sent before a subscription.
    r.host.voiceChanged([person('10')]);
    r.host.voiceChanged([person('10', { speaking: true })]);
    expect(events()).toHaveLength(0);

    await r.req('1', 'subscribe', { event: 'SPEAKING_START', args: { channel_id: '3' } });
    await r.req('2', 'subscribe', { event: 'SPEAKING_STOP' });
    await r.req('3', 'subscribe', { event: 'VOICE_STATE_UPDATE', args: { channel_id: '3' } });
    r.host.voiceChanged([person('10', { speaking: false })]);
    expect(events().map((e) => e.event)).toEqual(['SPEAKING_STOP']);
    expect(events()[0]?.data).toEqual({ channel_id: '3', user_id: '10' });
    r.host.voiceChanged([person('10', { speaking: true })]);
    expect(events().at(-1)).toMatchObject({
      event: 'SPEAKING_START',
      data: { channel_id: '3', user_id: '10' },
    });

    // A mute is a voice-state update with Discord's fields; a new member is one too.
    r.host.voiceChanged([person('10', { speaking: true, muted: true }), person('11')]);
    const states = events().filter((e) => e.event === 'VOICE_STATE_UPDATE');
    expect(states).toHaveLength(2);
    expect(states[0]?.data).toMatchObject({
      voice_state: {
        mute: false,
        deaf: false,
        self_mute: true,
        self_deaf: false,
        suppress: false,
      },
      user: { id: '10', username: 'ann' },
      mute: true,
      volume: 100,
      pan: { left: 100, right: 100 },
    });
    expect(states[1]?.data).toMatchObject({ user: { id: '11' } });

    // Server mute is `mute`, not `self_mute`. A speaker who leaves has stopped.
    r.host.voiceChanged([person('10', { speaking: true, muted: true, serverMuted: true })]);
    expect(events().at(-1)).toMatchObject({
      event: 'VOICE_STATE_UPDATE',
      data: { voice_state: { mute: true, self_mute: false } },
    });
    r.host.voiceChanged([]);
    expect(events().at(-1)).toMatchObject({
      event: 'SPEAKING_STOP',
      data: { channel_id: '3', user_id: '10' },
    });

    // Unsubscribed events stop.
    await r.req('4', 'unsubscribe', { event: 'VOICE_STATE_UPDATE' });
    const n = events().length;
    r.host.voiceChanged([person('12', { muted: true })]);
    expect(events()).toHaveLength(n);
  });

  it('refuses to subscribe to the events of another channel', async () => {
    const r = rig();
    await r.send(handshake);
    await r.req('1', 'subscribe', { event: 'SPEAKING_START', args: { channel_id: '99' } });
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'NOT_AUTHORIZED' } });
    r.host.voiceChanged([person('10')]);
    r.host.voiceChanged([person('10', { speaking: true })]);
    expect(r.posted.filter((f) => f.kind === 'event')).toHaveLength(0);
  });
});
