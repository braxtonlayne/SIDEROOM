// RDY-DEV-001: the parts of the voice transport that need no SFU. The SFU itself is exercised by
// voice.live.test.ts (SR_VOICE_LIVE=1, the `live` job of e2e.yml).
import { describe, expect, it } from 'vitest';
import {
  MONO_OPUS_PT,
  STEREO_OPUS_PT,
  createVoicePeer,
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
    for (const c of [4004, 4014, 4016, 4017, 4002])
      expect(closeIsRetryable(c), String(c)).toBe(false);
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

describe('bot DJ (MUS-JB-001)', () => {
  it('has a music option and its own error kind', async () => {
    const e = new TransportError('no-music-line', 'no music line');
    expect(e.kind).toBe('no-music-line');
    // The option type-checks, and a bad endpoint is still refused before any I/O.
    const u = new VoiceServerUpdate('t', 'https://example.test/voice', null, '1', true);
    await expect(
      VoiceSession.connect(u, { bindIp: '127.0.0.1', music: true }),
    ).rejects.toMatchObject({ kind: 'bad-endpoint' });
  });
});

describe('bot offer: one Opus flavour per line (MUS-JB-001)', () => {
  it('lists mono Opus on the microphone line and stereo Opus alone on the music line', async () => {
    const { pc } = createVoicePeer('127.0.0.1', true);
    const offer = await pc.createOffer();
    const audio = (offer.sdp ?? '').split(/\r?\nm=/).filter((s) => s.startsWith('audio'));
    expect(audio).toHaveLength(2);
    const [mic, song] = audio as [string, string];
    expect(mic).toMatch(new RegExp(`a=rtpmap:${MONO_OPUS_PT} opus/48000/2`, 'i'));
    expect(mic).not.toContain(`:${STEREO_OPUS_PT} `);
    expect(mic).not.toContain('stereo');
    expect(song).toMatch(new RegExp(`a=rtpmap:${STEREO_OPUS_PT} opus/48000/2`, 'i'));
    expect(song).not.toContain(`:${MONO_OPUS_PT} `);
    expect(song).toMatch(/a=fmtp:112 [^\r\n]*stereo=1/);
    expect(song).toContain('sprop-stereo=1');
    await pc.close();
  });

  it('without a music line the offer has one mono line and no stereo type', async () => {
    const { pc, musicLine } = createVoicePeer('127.0.0.1', false);
    expect(musicLine).toBeNull();
    const sdp = (await pc.createOffer()).sdp ?? '';
    expect(sdp).not.toContain('stereo');
    expect(sdp).not.toContain(`:${STEREO_OPUS_PT} `);
    await pc.close();
  });
});
