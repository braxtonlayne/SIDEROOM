import { describe, expect, it, vi } from 'vitest';
import type { AuthenticateResult, Frame } from '@sideroom/embedded-app-sdk';
import { createActivityHost, safeExternalUrl, type HostEnv } from '../host';
import type { ActivityInstance } from '../types';

const ORIGIN = 'https://7.activities.chat.example.org';
const instance: ActivityInstance = {
  id: '9',
  application_id: '7',
  application_name: 'Chess',
  guild_id: '2',
  channel_id: '3',
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
  };
  const host = createActivityHost(env);
  const send = (data: unknown, origin = ORIGIN) => host.handle(data, origin);
  const req = (nonce: string, cmd: string, args?: unknown) =>
    send({ sr: 1, kind: 'request', nonce, cmd, args });
  const last = () => posted[posted.length - 1] as Extract<Frame, { kind: 'response' }>;
  return { host, posted, send, req, last, authorizeCode, tokenInfo, confirm, openLink };
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
    await r.req('1', 'setActivity', {});
    expect(r.last()).toMatchObject({ ok: false, error: { code: 'UNKNOWN_COMMAND' } });
  });
});
