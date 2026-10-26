// RDY-DEV-001: the parts of the voice transport that need no SFU. The SFU itself is exercised by
// voice.live.test.ts (SR_VOICE_LIVE=1, the `live` job of e2e.yml).
import { describe, expect, it } from 'vitest';
import {
  TransportError,
  VoiceServerUpdate,
  VoiceSession,
  closeIsRetryable,
  voiceStateBody,
} from './voiceTransport';

describe('voice server update', () => {
  const d = {
    token: 'secret-token',
    endpoint: 'wss://v.example/voice/ws',
    channel_id: '5',
    guild_id: '9',
    ice_servers: [],
  };

  it('parses and defaults to encrypted', () => {
    const u = VoiceServerUpdate.fromDispatch(d);
    expect(u?.channelId).toBe('5');
    expect(u?.guildId).toBe('9');
    expect(u?.e2ee, 'a missing sr_e2ee counts as encrypted').toBe(true);
    expect(VoiceServerUpdate.fromDispatch({ ...d, sr_e2ee: false })?.e2ee).toBe(false);
    expect(VoiceServerUpdate.fromDispatch({ token: 't' })).toBeNull();
  });

  it('keeps the join token out of logs', () => {
    const u = VoiceServerUpdate.fromDispatch(d);
    expect(JSON.stringify(u)).not.toContain('secret-token');
  });
});

describe('join body', () => {
  it('declares E2EE and leaves with a null channel', () => {
    const j = voiceStateBody('9', '5', false, false, true);
    expect(j.sr_e2ee).toBe(true);
    expect(j.guild_id).toBe('9');
    const l = voiceStateBody(null, null, false, false, true);
    expect(l.channel_id).toBeNull();
    expect('guild_id' in l).toBe(false);
  });
});

describe('reconnect rules', () => {
  it('retries losses, not removals or refusals', () => {
    for (const c of [0, 4006, 4008, 4009, 4015]) expect(closeIsRetryable(c), String(c)).toBe(true);
    // 4004 refused token, 4014 removed, 4016 replaced, 4017 full, 4002 protocol error.
    for (const c of [4004, 4014, 4016, 4017, 4002]) expect(closeIsRetryable(c), String(c)).toBe(false);
  });

  it('refuses a non-websocket endpoint before any I/O', async () => {
    const u = new VoiceServerUpdate('t', 'https://example.test/voice', null, '1', true);
    await expect(VoiceSession.connect(u, { bindIp: '127.0.0.1' })).rejects.toMatchObject({
      kind: 'bad-endpoint',
    });
    await expect(VoiceSession.connect(u, { bindIp: '127.0.0.1' })).rejects.toBeInstanceOf(
      TransportError,
    );
  });
});
