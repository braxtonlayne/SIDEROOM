import { describe, expect, it } from 'vitest';
import { emptyData, reduceDispatch, type StoreData } from './reducer';

const apply = (s: StoreData, event: string, d: unknown): StoreData => ({
  ...s,
  ...reduceDispatch(s, event, d),
});

describe('voice state reducer', () => {
  it('tracks who is in which channel per guild', () => {
    let s = emptyData();
    s = apply(s, 'VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c1',
      user_id: 'u1',
      self_mute: true,
    });
    s = apply(s, 'VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: 'c2', user_id: 'u2' });
    expect(Object.keys(s.voiceStates.g1!)).toEqual(['u1', 'u2']);
    expect(s.voiceStates.g1!.u1).toMatchObject({
      channel_id: 'c1',
      self_mute: true,
      guild_id: 'g1',
    });
  });

  it('moving channel updates the entry; channel_id null removes it', () => {
    let s = emptyData();
    s = apply(s, 'VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: 'c1', user_id: 'u1' });
    s = apply(s, 'VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c2',
      user_id: 'u1',
      self_deaf: true,
    });
    expect(s.voiceStates.g1!.u1).toMatchObject({ channel_id: 'c2', self_deaf: true });
    s = apply(s, 'VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: null, user_id: 'u1' });
    expect(s.voiceStates.g1!.u1).toBeUndefined();
  });

  it('a user appears in one guild at a time', () => {
    let s = emptyData();
    s = apply(s, 'VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: 'c1', user_id: 'u1' });
    s = apply(s, 'VOICE_STATE_UPDATE', { guild_id: 'g2', channel_id: 'c9', user_id: 'u1' });
    expect(s.voiceStates.g1!.u1).toBeUndefined();
    expect(s.voiceStates.g2!.u1).toBeDefined();
  });

  it('DM calls (no guild id) are stored under @me', () => {
    const s = apply(emptyData(), 'VOICE_STATE_UPDATE', { channel_id: 'dm1', user_id: 'u1' });
    expect(s.voiceStates['@me']!.u1).toBeDefined();
  });

  it('ignores updates without a user', () => {
    expect(
      reduceDispatch(emptyData(), 'VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: 'c1' }),
    ).toEqual({});
  });

  it('GUILD_CREATE seeds voice states and GUILD_DELETE drops them', () => {
    let s = emptyData();
    s = apply(s, 'GUILD_CREATE', {
      id: 'g1',
      name: 'G',
      channels: [],
      voice_states: [
        { channel_id: 'c1', user_id: 'u1' },
        { channel_id: null, user_id: 'u2' },
      ],
    });
    expect(Object.keys(s.voiceStates.g1!)).toEqual(['u1']);
    s = apply(s, 'GUILD_DELETE', { id: 'g1' });
    expect(s.voiceStates.g1).toBeUndefined();
  });

  it('READY clears stale voice states', () => {
    let s = apply(emptyData(), 'VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c1',
      user_id: 'u1',
    });
    s = apply(s, 'READY', { user: { id: 'me', username: 'me' }, guilds: [] });
    expect(s.voiceStates).toEqual({});
  });

  it('stores VOICE_SERVER_UPDATE with endpoint, token and ICE servers', () => {
    const s = apply(emptyData(), 'VOICE_SERVER_UPDATE', {
      guild_id: 'g1',
      endpoint: 'voice.test:8443',
      token: 'abc',
      ice_servers: [{ urls: ['stun:a'], username: 'u', credential: 'c' }],
    });
    expect(s.voiceServer).toEqual({
      guild_id: 'g1',
      endpoint: 'voice.test:8443',
      token: 'abc',
      ice_servers: [{ urls: ['stun:a'], username: 'u', credential: 'c' }],
    });
  });
});
