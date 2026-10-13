import { describe, expect, it } from 'vitest';
import {
  codeLabel,
  endsOnKeyup,
  keybindFromEvent,
  keybindParts,
  keybindText,
  matchesKeydown,
  sameKeybind,
  sanitizeKeybind,
} from '../keybind';
import { encode, parseFrame, tuneOpus, VoiceOp, voiceUrl } from '../protocol';
import { barsFor, levelFor, qualityFromStats } from '../quality';
import { AUTO_MARGIN_DB, HANG_MS, levelFraction, rmsDb, SILENCE_DB, VadGate } from '../vad';
import { CUES, cueEnabled } from '../sounds';
import {
  DEFAULT_VOICE_SETTINGS,
  effectiveInputMode,
  sanitizeVoiceSettings,
  userVolumeOf,
} from '../settings';

const ev = (
  code: string,
  mods: Partial<Record<'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey', boolean>> = {},
) => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe('voice activity gate', () => {
  it('measures RMS in dBFS', () => {
    expect(rmsDb([])).toBe(SILENCE_DB);
    expect(rmsDb(new Float32Array(64))).toBe(SILENCE_DB);
    expect(rmsDb(new Float32Array(64).fill(1))).toBeCloseTo(0, 5);
    expect(rmsDb(new Float32Array(64).fill(0.1))).toBeCloseTo(-20, 3);
    expect(levelFraction(0)).toBe(1);
    expect(levelFraction(-100)).toBe(0);
  });

  it('manual mode opens at the threshold and holds, then closes', () => {
    const g = new VadGate({ auto: false, thresholdDb: -40 });
    expect(g.process(-60, 0)).toBe(false);
    expect(g.process(-39, 100)).toBe(true);
    // Quiet again: stays open through the hang time, then closes.
    expect(g.process(-80, 100 + HANG_MS)).toBe(true);
    expect(g.process(-80, 100 + HANG_MS + 1)).toBe(false);
    expect(g.openAtDb).toBe(-40);
  });

  it('automatic sensitivity follows the noise floor', () => {
    const g = new VadGate({ auto: true, thresholdDb: -10 });
    let t = 0;
    for (let i = 0; i < 100; i++) g.process(-70, (t += 30));
    expect(g.isOpen).toBe(false);
    // Opens a margin above the settled floor, ignoring the manual threshold.
    expect(g.openAtDb).toBeCloseTo(-70 + AUTO_MARGIN_DB, 0);
    expect(g.process(-50, (t += 30))).toBe(true);
    // A louder room raises the floor, so the same level no longer opens the gate.
    const noisy = new VadGate({ auto: true, thresholdDb: -10 });
    t = 0;
    for (let i = 0; i < 400; i++) noisy.process(-45, (t += 30));
    expect(noisy.openAtDb).toBeGreaterThan(g.openAtDb);
    expect(noisy.process(-45, (t += 30))).toBe(false);
  });

  it('speech does not drag the automatic floor upward', () => {
    const g = new VadGate({ auto: true, thresholdDb: -50 });
    let t = 0;
    for (let i = 0; i < 50; i++) g.process(-70, (t += 30));
    const floorOpen = g.openAtDb;
    for (let i = 0; i < 200; i++) g.process(-20, (t += 30));
    expect(g.openAtDb).toBeCloseTo(floorOpen, 0);
  });
});

describe('keybinds', () => {
  it('matches the main key and exact modifiers', () => {
    const b = { code: 'KeyV', ctrl: true };
    expect(matchesKeydown(b, ev('KeyV', { ctrlKey: true }))).toBe(true);
    expect(matchesKeydown(b, ev('KeyV'))).toBe(false);
    expect(matchesKeydown(b, ev('KeyV', { ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(matchesKeydown({ code: 'ShiftLeft' }, ev('ShiftLeft', { shiftKey: true }))).toBe(true);
  });

  it('ends on the main key or a required modifier release', () => {
    const b = { code: 'KeyV', ctrl: true };
    expect(endsOnKeyup(b, { code: 'KeyV' })).toBe(true);
    expect(endsOnKeyup(b, { code: 'ControlLeft' })).toBe(true);
    expect(endsOnKeyup(b, { code: 'ShiftLeft' })).toBe(false);
  });

  it('records from events: Escape cancels, modifier-only is allowed', () => {
    expect(keybindFromEvent(ev('Escape'))).toBeNull();
    expect(keybindFromEvent(ev('ControlRight', { ctrlKey: true }))).toEqual({
      code: 'ControlRight',
    });
    expect(keybindFromEvent(ev('KeyT', { altKey: true, shiftKey: true }))).toEqual({
      code: 'KeyT',
      alt: true,
      shift: true,
    });
  });

  it('formats readable labels', () => {
    expect(keybindParts({ code: 'KeyV', ctrl: true, shift: true })).toEqual(['Ctrl', 'Shift', 'V']);
    expect(keybindText(null)).toBe('Not bound');
    expect(codeLabel('Backquote')).toBe('`');
    expect(codeLabel('Digit5')).toBe('5');
    expect(codeLabel('Numpad3')).toBe('Numpad 3');
    expect(codeLabel('F5')).toBe('F5');
    expect(codeLabel('ArrowUp')).toBe('Up');
  });

  it('compares and sanitizes', () => {
    expect(sameKeybind({ code: 'KeyV' }, { code: 'KeyV' })).toBe(true);
    expect(sameKeybind({ code: 'KeyV' }, { code: 'KeyV', alt: true })).toBe(false);
    expect(sanitizeKeybind({ code: 'KeyV', ctrl: 'yes', evil: 1 })).toEqual({ code: 'KeyV' });
    expect(sanitizeKeybind({ code: '' })).toBeNull();
    expect(sanitizeKeybind('KeyV')).toBeNull();
  });
});

describe('voice protocol', () => {
  it('encodes and parses frames', () => {
    expect(parseFrame(encode(VoiceOp.HEARTBEAT, 5))).toEqual({ op: 3, d: 5 });
    expect(parseFrame('not json')).toBeNull();
    expect(parseFrame('{"x":1}')).toBeNull();
    expect(parseFrame(new ArrayBuffer(2))).toBeNull();
  });

  it('builds the voice socket URL from an endpoint', () => {
    expect(voiceUrl('voice.example.org:8443', true)).toBe('wss://voice.example.org:8443/voice?v=1');
    expect(voiceUrl('voice.example.org', false)).toBe('ws://voice.example.org/voice?v=1');
    expect(voiceUrl('wss://x.test/custom')).toBe('wss://x.test/custom');
    expect(voiceUrl('/voice', false)).toMatch(/^ws:\/\/[^/]+\/voice$/);
  });

  it('tunes the Opus fmtp line: DTX, FEC, mono, capped bitrate', () => {
    const sdp =
      'a=rtpmap:111 opus/48000/2\r\na=fmtp:111 minptime=10;useinbandfec=1;stereo=1\r\na=other\r\n';
    const out = tuneOpus(sdp, 48000);
    expect(out).toContain('usedtx=1');
    expect(out).toContain('useinbandfec=1');
    expect(out).toContain('stereo=0');
    expect(out).toContain('maxaveragebitrate=48000');
    expect(out).not.toContain('stereo=1');
    expect(out).toContain('a=other');
    // No fmtp line yet: one is added after the rtpmap.
    const bare = tuneOpus('a=rtpmap:96 opus/48000/2\r\n');
    expect(bare).toMatch(/a=fmtp:96 .*usedtx=1/);
    // No opus at all: untouched.
    expect(tuneOpus('m=video')).toBe('m=video');
  });
});

describe('connection quality', () => {
  it('classifies RTT and loss', () => {
    expect(levelFor(null, null)).toBe('unknown');
    expect(levelFor(40, 0)).toBe('good');
    expect(levelFor(200, 0)).toBe('fair');
    expect(levelFor(40, 5)).toBe('fair');
    expect(levelFor(400, 0)).toBe('poor');
    expect(levelFor(40, 12)).toBe('poor');
    expect(barsFor('good')).toBe(3);
    expect(barsFor('unknown')).toBe(0);
  });

  it('reads stats reports', () => {
    const stats = [
      { type: 'candidate-pair', state: 'succeeded', currentRoundTripTime: 0.045 },
      { type: 'remote-inbound-rtp', kind: 'audio', roundTripTime: 0.9, fractionLost: 0.04 },
      { type: 'remote-inbound-rtp', kind: 'video', fractionLost: 0.9 },
    ];
    const q = qualityFromStats({ forEach: (cb) => stats.forEach(cb) });
    expect(q).toEqual({ level: 'fair', pingMs: 45, lossPct: 4 });
    expect(qualityFromStats({ forEach: () => undefined }).level).toBe('unknown');
  });
});

describe('settings and cues', () => {
  it('sanitizes stored settings', () => {
    const s = sanitizeVoiceSettings({
      inputVolume: 999,
      pttReleaseMs: -5,
      inputMode: 'nope',
      noiseSuppression: true,
      thresholdDb: 'loud',
      keybinds: { ptt: { code: 'KeyV' }, toggleMute: 3 },
      userVolumes: { a: 150, b: 100, c: 'x' },
      localMuted: { a: true, b: false },
    });
    expect(s.inputVolume).toBe(200);
    expect(s.pttReleaseMs).toBe(0);
    expect(s.inputMode).toBe('vad');
    expect(s.thresholdDb).toBe(DEFAULT_VOICE_SETTINGS.thresholdDb);
    expect(s.keybinds.ptt).toEqual({ code: 'KeyV' });
    expect(s.keybinds.toggleMute).toBeNull();
    expect(s.userVolumes).toEqual({ a: 150 });
    expect(s.localMuted).toEqual({ a: true });
    expect(userVolumeOf(s, 'zz')).toBe(100);
    // There is no noise suppression setting at all (D-045).
    expect(Object.keys(s)).not.toContain('noiseSuppression');
    expect(sanitizeVoiceSettings(null)).toEqual(DEFAULT_VOICE_SETTINGS);
  });

  it('forces PTT when the channel denies USE_VAD', () => {
    expect(effectiveInputMode({ inputMode: 'vad' }, true)).toBe('ptt');
    expect(effectiveInputMode({ inputMode: 'vad' }, false)).toBe('vad');
    expect(effectiveInputMode({ inputMode: 'ptt' }, false)).toBe('ptt');
  });

  it('gates cues by their setting group', () => {
    const all = { soundJoinLeave: true, soundMuteDeafen: true, soundPtt: true, soundVolume: 50 };
    expect(cueEnabled('join', { ...all, soundJoinLeave: false })).toBe(false);
    expect(cueEnabled('mute', { ...all, soundJoinLeave: false })).toBe(true);
    expect(cueEnabled('pttOn', { ...all, soundPtt: false })).toBe(false);
    expect(cueEnabled('error', { ...all, soundPtt: false, soundMuteDeafen: false })).toBe(true);
    expect(CUES.selfJoin.tones.length).toBeGreaterThan(1);
  });
});
