import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Op } from '../../gateway/protocol';
import type { VoiceState } from '../../store/types';
import {
  MAX_MIC_RECOVERIES,
  MAX_REJOINS,
  SERVER_WAIT_MS,
  VoiceController,
  type ControllerDeps,
} from '../controller';
import {
  VoiceConnectError,
  type ConnectParams,
  type EngineEvents,
  type VoiceEngine,
} from '../engine';
import { HANG_MS } from '../vad';
import { IDLE_SESSION, PING_HISTORY, resetSession, useVoiceSession } from '../session';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../settings';
import { fakeAudio } from './fakes';

class MockEngine implements VoiceEngine {
  status: VoiceEngine['status'] = 'idle';
  connectParams: ConnectParams | null = null;
  transmitting: boolean[] = [];
  deafened: boolean[] = [];
  volumes: Record<string, number> = {};
  muted: Record<string, boolean> = {};
  master: number[] = [];
  input: number[] = [];
  outputDevices: string[] = [];
  inputDevices: unknown[] = [];
  disconnected = 0;
  private resolve!: () => void;
  private reject!: (e: unknown) => void;
  constructor(public events: EngineEvents) {}
  connect(p: ConnectParams) {
    this.connectParams = p;
    return new Promise<void>((res, rej) => {
      this.resolve = res;
      this.reject = rej;
    });
  }
  ok() {
    this.status = 'connected';
    this.resolve();
  }
  fail(e: unknown) {
    this.reject(e);
  }
  disconnect() {
    this.disconnected++;
  }
  setTransmitting(t: boolean) {
    this.transmitting.push(t);
  }
  setDeafened(d: boolean) {
    this.deafened.push(d);
  }
  setUserVolume(id: string, v: number) {
    this.volumes[id] = v;
  }
  setUserMuted(id: string, m: boolean) {
    this.muted[id] = m;
  }
  setMasterVolume(v: number) {
    this.master.push(v);
  }
  setInputVolume(v: number) {
    this.input.push(v);
  }
  async setInputDevice(o: unknown) {
    this.inputDevices.push(o);
  }
  async setOutputDevice(d: string) {
    this.outputDevices.push(d);
  }
  restartIce() {}
  get lastTransmit() {
    return this.transmitting.at(-1);
  }
}

interface Harness {
  c: VoiceController;
  engines: MockEngine[];
  sent: { op: number; d: Record<string, unknown> }[];
  emit(event: string, data: unknown): void;
  states: Record<string, Record<string, VoiceState>>;
  gatewayOpen: { value: boolean };
  audio: ReturnType<typeof fakeAudio>;
  clock: { t: number };
  last(): MockEngine;
}

function harness(): Harness {
  const engines: MockEngine[] = [];
  const sent: Harness['sent'] = [];
  const listeners = new Set<(e: string, d: unknown) => void>();
  const states: Harness['states'] = {};
  const gatewayOpen = { value: true };
  const clock = { t: 0 };
  const audio = fakeAudio();
  const deps: ControllerDeps = {
    createEngine: (events) => {
      const e = new MockEngine(events);
      engines.push(e);
      return e;
    },
    sendGateway: (op, d) => {
      if (!gatewayOpen.value) return false;
      sent.push({ op, d: d as Record<string, unknown> });
      return true;
    },
    subscribe: (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    currentUserId: () => 'me',
    voiceStatesOf: (g) => states[g] ?? {},
    audio: audio.deps,
    now: () => clock.t,
  };
  const c = new VoiceController(deps);
  return {
    c,
    engines,
    sent,
    states,
    gatewayOpen,
    audio,
    clock,
    emit: (e, d) => [...listeners].forEach((l) => l(e, d)),
    last: () => engines.at(-1)!,
  };
}

const target = { guildId: 'g1', channelId: 'c1', name: 'General' };

/** Join and deliver both gateway events so the engine starts. */
function joinAndServe(h: Harness, t = target) {
  h.c.join(t);
  h.emit('VOICE_STATE_UPDATE', {
    guild_id: t.guildId,
    channel_id: t.channelId,
    user_id: 'me',
    session_id: 'sess-1',
  });
  h.emit('VOICE_SERVER_UPDATE', {
    guild_id: t.guildId,
    endpoint: 'voice.test',
    token: 't0k',
    ice_servers: [{ urls: 'stun:x' }],
  });
}
async function connected(h: Harness, t = target) {
  joinAndServe(h, t);
  h.last().ok();
  await Promise.resolve();
  await Promise.resolve();
}
const sess = () => useVoiceSession.getState();

beforeEach(() => {
  vi.useFakeTimers();
  resetSession();
  useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
});
afterEach(() => {
  vi.useRealTimers();
});

describe('joining', () => {
  it('sends VOICE_STATE_UPDATE and connects when the voice server arrives', async () => {
    const h = harness();
    h.c.join(target);
    expect(h.sent).toEqual([
      {
        op: Op.VOICE_STATE_UPDATE,
        d: { guild_id: 'g1', channel_id: 'c1', self_mute: false, self_deaf: false },
      },
    ]);
    expect(sess().status).toBe('joining');
    expect(sess().channelName).toBe('General');
    expect(h.engines).toHaveLength(0);
    h.emit('VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c1',
      user_id: 'me',
      session_id: 'sess-1',
    });
    expect(h.engines).toHaveLength(0);
    h.emit('VOICE_SERVER_UPDATE', {
      guild_id: 'g1',
      endpoint: 'voice.test',
      token: 't0k',
      ice_servers: [{ urls: 'stun:x' }],
    });
    expect(h.engines).toHaveLength(1);
    expect(sess().status).toBe('connecting');
    expect(h.last().connectParams).toMatchObject({
      endpoint: 'voice.test',
      token: 't0k',
      userId: 'me',
      iceServers: [{ urls: 'stun:x' }],
      mic: { echoCancellation: true, autoGainControl: true },
    });
    h.last().ok();
    await Promise.resolve();
    await Promise.resolve();
    expect(sess().status).toBe('connected');
  });

  it('works when the server update arrives before our own state', () => {
    const h = harness();
    h.c.join(target);
    h.emit('VOICE_SERVER_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c1',
      endpoint: 'wss://v.test/voice/ws',
      token: 't',
    });
    expect(h.engines).toHaveLength(1);
  });

  it('ignores a server update for a different channel (stale after a move)', () => {
    const h = harness();
    h.c.join(target);
    h.emit('VOICE_SERVER_UPDATE', {
      guild_id: 'g1',
      channel_id: 'old',
      endpoint: 'wss://v.test/voice/ws',
      token: 't',
    });
    expect(h.engines).toHaveLength(0);
  });

  it('shows why the server refused the join (SR_VOICE_ERROR)', () => {
    const h = harness();
    h.c.join(target);
    h.emit('SR_VOICE_ERROR', {
      guild_id: 'g1',
      channel_id: 'c1',
      code: 'channel_full',
      message: 'x',
    });
    expect(sess().status).toBe('error');
    expect(sess().error).toEqual({ code: 'refused', message: 'This voice channel is full.' });
    h.c.join(target);
    h.emit('SR_VOICE_ERROR', {
      guild_id: 'g1',
      channel_id: 'c1',
      code: 'missing_permission',
      message: 'x',
    });
    expect(sess().error?.message).toContain("don't have permission");
  });

  it("ignores another guild's server update and other people's state", () => {
    const h = harness();
    h.c.join(target);
    h.emit('VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c1',
      user_id: 'someone',
      session_id: 'x',
    });
    h.emit('VOICE_SERVER_UPDATE', { guild_id: 'other', endpoint: 'e', token: 't' });
    h.emit('VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c1',
      user_id: 'me',
      session_id: 's',
    });
    expect(h.engines).toHaveLength(0);
  });

  it('reports an error when the gateway is not open', () => {
    const h = harness();
    h.gatewayOpen.value = false;
    h.c.join(target);
    expect(sess().status).toBe('error');
    expect(sess().error?.code).toBe('gateway');
  });

  it('gives up when the voice server never answers', () => {
    const h = harness();
    h.c.join(target);
    vi.advanceTimersByTime(SERVER_WAIT_MS + 1);
    expect(sess().status).toBe('error');
    expect(sess().error?.code).toBe('timeout');
    // It tells the gateway we are not joining after all.
    expect(h.sent.at(-1)!.d.channel_id).toBeNull();
  });

  it('surfaces a denied microphone and leaves the channel', async () => {
    const h = harness();
    joinAndServe(h);
    h.last().fail(
      new VoiceConnectError({
        code: 'mic',
        message: 'Microphone access is blocked.',
        mic: 'permission-denied',
      }),
    );
    await Promise.resolve();
    await Promise.resolve();
    expect(sess().status).toBe('error');
    expect(sess().error).toMatchObject({ code: 'mic', mic: 'permission-denied' });
    expect(h.sent.at(-1)!.d.channel_id).toBeNull();
    expect(h.last().disconnected).toBeGreaterThan(0);
  });

  it('joining the same channel again is a no-op; another channel replaces the connection', async () => {
    const h = harness();
    await connected(h);
    h.c.join(target);
    expect(h.sent).toHaveLength(1);
    h.c.join({ ...target, channelId: 'c2', name: 'Movie night' });
    expect(h.engines[0]!.disconnected).toBeGreaterThan(0);
    expect(h.sent.at(-1)!.d.channel_id).toBe('c2');
    expect(sess().channelName).toBe('Movie night');
    expect(sess().status).toBe('joining');
  });

  it('leave sends channel_id null and resets the session', async () => {
    const h = harness();
    await connected(h);
    h.c.leave();
    expect(h.sent.at(-1)!.d).toMatchObject({ guild_id: 'g1', channel_id: null });
    expect(sess().status).toBe('idle');
    expect(sess().channelId).toBeNull();
    expect(h.last().disconnected).toBeGreaterThan(0);
  });

  it('re-sends the voice state after a brand-new gateway session', async () => {
    const h = harness();
    await connected(h);
    const before = h.sent.length;
    h.emit('READY', {});
    expect(h.sent).toHaveLength(before + 1);
    expect(h.sent.at(-1)!.d.channel_id).toBe('c1');
  });
});

describe('mute and deafen', () => {
  it('mute closes the transmit gate and tells the gateway', async () => {
    const h = harness();
    await connected(h);
    h.c.toggleMute();
    expect(sess().selfMute).toBe(true);
    expect(h.sent.at(-1)!.d).toMatchObject({ self_mute: true, self_deaf: false });
    h.c.toggleMute();
    expect(sess().selfMute).toBe(false);
  });

  it('deafen mutes the mic and silences remote audio; undeafen restores the old mute state', async () => {
    const h = harness();
    await connected(h);
    h.c.toggleDeafen();
    expect(sess()).toMatchObject({ selfDeaf: true, selfMute: true });
    expect(h.last().deafened.at(-1)).toBe(true);
    expect(h.sent.at(-1)!.d).toMatchObject({ self_mute: true, self_deaf: true });
    h.c.toggleDeafen();
    expect(sess()).toMatchObject({ selfDeaf: false, selfMute: false });
    expect(h.last().deafened.at(-1)).toBe(false);
    // If the mic was muted before deafening it stays muted afterwards.
    h.c.toggleMute();
    h.c.toggleDeafen();
    h.c.toggleDeafen();
    expect(sess()).toMatchObject({ selfDeaf: false, selfMute: true });
  });

  it('unmuting while deafened also undeafens (Discord)', async () => {
    const h = harness();
    await connected(h);
    h.c.toggleDeafen();
    h.c.toggleMute();
    expect(sess()).toMatchObject({ selfDeaf: false, selfMute: false });
    expect(h.last().deafened.at(-1)).toBe(false);
  });

  it('carries mute / deafen into the next join', () => {
    const h = harness();
    h.c.toggleMute(); // not connected: still remembered
    h.c.join(target);
    expect(h.sent[0]!.d).toMatchObject({ self_mute: true });
  });
});

describe('input modes', () => {
  const speak = (h: Harness, db: number) => {
    h.clock.t += 30;
    h.last().events.level(db);
  };

  it('voice activity opens the gate on speech and closes after the hang time', async () => {
    const h = harness();
    useVoiceSettings
      .getState()
      .update({ inputMode: 'vad', autoSensitivity: false, thresholdDb: -45 });
    await connected(h);
    speak(h, -70);
    expect(sess().transmitting).toBe(false);
    speak(h, -30);
    expect(h.last().lastTransmit).toBe(true);
    expect(sess().transmitting).toBe(true);
    h.clock.t += HANG_MS + 100;
    speak(h, -80);
    expect(h.last().lastTransmit).toBe(false);
    expect(sess().transmitting).toBe(false);
  });

  it('does not transmit while muted even when speaking', async () => {
    const h = harness();
    useVoiceSettings.getState().update({ autoSensitivity: false, thresholdDb: -45 });
    await connected(h);
    h.c.toggleMute();
    speak(h, -20);
    expect(h.last().lastTransmit).toBe(false);
    h.c.toggleMute();
    expect(h.last().lastTransmit).toBe(true);
  });

  it('push to talk transmits only while the key is held (plus the release delay)', async () => {
    const h = harness();
    useVoiceSettings.getState().update({ inputMode: 'ptt', pttReleaseMs: 300 });
    useVoiceSettings.getState().setKeybind('ptt', { code: 'KeyV' });
    const stop = h.c.start();
    await connected(h);
    speak(h, -10); // loud, but VAD is not in play
    expect(sess().transmitting).toBe(false);
    window.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyV', cancelable: true }));
    expect(sess().pttActive).toBe(true);
    expect(h.last().lastTransmit).toBe(true);
    window.dispatchEvent(new KeyboardEvent('keyup', { code: 'KeyV', cancelable: true }));
    vi.advanceTimersByTime(299);
    expect(sess().transmitting).toBe(true);
    vi.advanceTimersByTime(2);
    expect(sess().transmitting).toBe(false);
    expect(h.last().lastTransmit).toBe(false);
    stop();
  });

  it('plays push-to-talk cues on press and release, unless turned off', async () => {
    const h = harness();
    useVoiceSettings.getState().update({ inputMode: 'ptt', pttReleaseMs: 0 });
    useVoiceSettings.getState().setKeybind('ptt', { code: 'KeyV' });
    const stop = h.c.start();
    await connected(h);
    const tones = () => h.audio.contexts.reduce((n, c) => n + c.oscillators, 0);
    const base = tones();
    window.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyV', cancelable: true }));
    expect(tones()).toBe(base + 1);
    window.dispatchEvent(new KeyboardEvent('keyup', { code: 'KeyV', cancelable: true }));
    expect(tones()).toBe(base + 2);
    useVoiceSettings.getState().update({ soundPtt: false });
    window.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyV', cancelable: true }));
    window.dispatchEvent(new KeyboardEvent('keyup', { code: 'KeyV', cancelable: true }));
    expect(tones()).toBe(base + 2);
    stop();
  });

  it('a channel that denies USE_VAD forces push to talk even if the user chose voice activity', async () => {
    const h = harness();
    useVoiceSettings
      .getState()
      .update({ inputMode: 'vad', autoSensitivity: false, thresholdDb: -45 });
    useVoiceSettings.getState().setKeybind('ptt', { code: 'KeyV' });
    const stop = h.c.start();
    await connected(h, { ...target, pttForced: true } as typeof target);
    expect(sess().pttForced).toBe(true);
    speak(h, -20);
    expect(sess().transmitting).toBe(false);
    window.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyV', cancelable: true }));
    expect(sess().transmitting).toBe(true);
    // Permissions change: the channel allows VAD again.
    window.dispatchEvent(new KeyboardEvent('keyup', { code: 'KeyV', cancelable: true }));
    vi.advanceTimersByTime(500);
    h.c.setPttForced(false);
    speak(h, -20);
    expect(sess().transmitting).toBe(true);
    stop();
  });

  it('the PTT key types normally outside a call', () => {
    const h = harness();
    useVoiceSettings.getState().update({ inputMode: 'ptt' });
    useVoiceSettings.getState().setKeybind('ptt', { code: 'KeyV' });
    const stop = h.c.start();
    const e = new KeyboardEvent('keydown', { code: 'KeyV', cancelable: true });
    window.dispatchEvent(e);
    expect(e.defaultPrevented).toBe(false);
    stop();
  });

  it('a server mute keeps the mic closed', async () => {
    const h = harness();
    useVoiceSettings.getState().update({ autoSensitivity: false, thresholdDb: -45 });
    await connected(h);
    h.emit('VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c1',
      user_id: 'me',
      session_id: 'sess-1',
      mute: true,
    });
    expect(sess().serverMute).toBe(true);
    speak(h, -20);
    expect(h.last().lastTransmit).toBe(false);
    h.emit('VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c1',
      user_id: 'me',
      session_id: 'sess-1',
      mute: false,
    });
    expect(sess().serverMute).toBe(false);
    speak(h, -20);
    expect(h.last().lastTransmit).toBe(true);
  });

  it('keyboard shortcuts toggle mute, deafen and disconnect while in a call', async () => {
    const h = harness();
    useVoiceSettings.getState().setKeybind('toggleMute', { code: 'KeyM', ctrl: true });
    useVoiceSettings.getState().setKeybind('toggleDeafen', { code: 'KeyD', ctrl: true });
    useVoiceSettings.getState().setKeybind('disconnect', { code: 'KeyQ', ctrl: true });
    const stop = h.c.start();
    const press = (code: string) =>
      window.dispatchEvent(new KeyboardEvent('keydown', { code, ctrlKey: true, cancelable: true }));
    await connected(h);
    press('KeyM');
    expect(sess().selfMute).toBe(true);
    press('KeyD');
    expect(sess().selfDeaf).toBe(true);
    press('KeyQ');
    expect(sess().status).toBe('idle');
    stop();
  });
});

describe('server-driven changes', () => {
  it('a moderator disconnect ends the session with a message', async () => {
    const h = harness();
    await connected(h);
    h.emit('VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: null, user_id: 'me' });
    expect(sess().status).toBe('error');
    expect(sess().error?.code).toBe('disconnected');
    expect(h.last().disconnected).toBeGreaterThan(0);
  });

  it('a moderator move follows the viewer into the new channel', async () => {
    const h = harness();
    await connected(h);
    h.emit('VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c9',
      user_id: 'me',
      session_id: 'sess-2',
    });
    expect(sess().channelId).toBe('c9');
    expect(h.engines[0]!.disconnected).toBeGreaterThan(0);
    // No engine until the new room's voice server arrives.
    expect(h.engines).toHaveLength(1);
    h.emit('VOICE_SERVER_UPDATE', {
      guild_id: 'g1',
      channel_id: 'c9',
      endpoint: 'wss://voice2.test/voice/ws',
      token: 'n',
    });
    expect(h.engines).toHaveLength(2);
    expect(h.last().connectParams).toMatchObject({
      endpoint: 'wss://voice2.test/voice/ws',
      token: 'n',
    });
  });

  it('a fresh VOICE_SERVER_UPDATE while connected reconnects to the new node', async () => {
    const h = harness();
    await connected(h);
    h.emit('VOICE_SERVER_UPDATE', { guild_id: 'g1', endpoint: 'node-b.test', token: 'z' });
    expect(h.engines).toHaveLength(2);
    expect(h.engines[0]!.disconnected).toBeGreaterThan(0);
  });

  it('asks the gateway for a new server when the engine reports a fatal error, a few times', async () => {
    const h = harness();
    await connected(h);
    const base = h.sent.length;
    for (let i = 0; i < MAX_REJOINS; i++) {
      h.last().events.fatal({ code: 'closed', message: 'x' });
      expect(sess().status).toBe('reconnecting');
      // Leave, then join again: a repeated join of the same channel would not issue a new server update.
      expect(h.sent).toHaveLength(base + 2 * (i + 1));
      expect(h.sent.at(-2)!.d.channel_id).toBeNull();
      expect(h.sent.at(-1)!.d.channel_id).toBe('c1');
      h.emit('VOICE_SERVER_UPDATE', { guild_id: 'g1', endpoint: `n${i}.test`, token: 't' });
    }
    h.last().events.fatal({ code: 'closed', message: 'x' });
    expect(sess().status).toBe('error');
    expect(sess().error?.code).toBe('connect');
  });

  it('4014 from the voice socket waits for the gateway instead of re-joining', async () => {
    const h = harness();
    await connected(h);
    const sentBefore = h.sent.length;
    h.last().events.fatal({ code: 'removed', message: 'x' });
    expect(h.sent).toHaveLength(sentBefore);
    expect(sess().status).toBe('connecting');
    // The gateway says it was a disconnect.
    h.emit('VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: null, user_id: 'me' });
    expect(sess().error?.code).toBe('disconnected');
  });

  it('replaced by another tab, or a full server, stops with a clear message', async () => {
    const h = harness();
    await connected(h);
    h.last().events.fatal({
      code: 'replaced',
      message: 'You joined this voice channel from another window.',
    });
    expect(sess().error).toMatchObject({ code: 'replaced' });
    const h2 = harness();
    await connected(h2);
    h2.last().events.fatal({ code: 'full', message: 'This voice server is full.' });
    expect(sess().error).toMatchObject({ code: 'refused' });
  });

  it('tracks speakers and clears them when they leave', async () => {
    const h = harness();
    await connected(h);
    h.last().events.speaking('u1', true, false);
    expect(sess().speaking).toEqual({ u1: true });
    h.last().events.speaking('me', true, false); // own ring follows transmitting, not the echo
    expect(sess().speaking).toEqual({ u1: true });
    h.last().events.peer('u1', false);
    expect(sess().speaking).toEqual({});
  });

  it('mirrors connection quality and reconnecting status', async () => {
    const h = harness();
    await connected(h);
    h.last().events.quality({ level: 'poor', pingMs: 400, lossPct: 2 });
    expect(sess().quality.level).toBe('poor');
    h.last().events.status('reconnecting');
    expect(sess().status).toBe('reconnecting');
    h.last().events.status('connected');
    expect(sess().status).toBe('connected');
  });

  it('a lost microphone falls back to the default input and keeps the call (VOX-CLI-001)', async () => {
    const h = harness();
    useVoiceSettings.setState({
      settings: { ...DEFAULT_VOICE_SETTINGS, inputDeviceId: 'usb-1' },
    });
    await connected(h);
    h.last().events.micLost();
    await Promise.resolve();
    await Promise.resolve();
    expect(h.last().inputDevices.at(-1)).toMatchObject({ deviceId: 'default' });
    expect(sess().status).toBe('connected');
    expect(sess().error).toBeNull();
    expect(sess().notice).toMatch(/default input/);
    h.c.dismissNotice();
    expect(sess().notice).toBeNull();
  });

  it('a lost microphone ends the call with a fix-it message when the default fails too', async () => {
    const h = harness();
    await connected(h);
    h.last().setInputDevice = () => Promise.reject(new Error('nope'));
    h.last().events.micLost();
    await Promise.resolve();
    await Promise.resolve();
    expect(sess().error).toMatchObject({ code: 'mic', mic: 'no-device' });
  });

  it('shows and clears the egress governor notice (VOX-UI-014)', async () => {
    const h = harness();
    await connected(h);
    h.last().events.governor!(true);
    expect(sess().reducedQuality).toBe(true);
    h.last().events.governor!(false);
    expect(sess().reducedQuality).toBe(false);
  });

  it('a refused camera keeps the call and shows the limit with an upgrade hint (VOX-UI-013)', async () => {
    const h = harness();
    await connected(h);
    h.emit('SR_VOICE_ERROR', { guild_id: 'g1', channel_id: 'c1', code: 'video_limit', limit: 5 });
    expect(sess().status).toBe('connected');
    expect(sess().refusal).toMatchObject({ code: 'videoLimit' });
    expect(sess().refusal!.message).toContain('video limit (5)');
    h.c.dismissRefusal();
    expect(sess().refusal).toBeNull();
    h.emit('SR_VOICE_ERROR', { guild_id: 'g1', channel_id: 'c1', code: 'viewer_cap' });
    expect(sess().refusal).toMatchObject({ code: 'viewerCap' });
    expect(sess().status).toBe('connected');
  });

  it('understands the codes the gateway sends for refused video and streams', async () => {
    const h = harness();
    await connected(h);
    h.emit('SR_VOICE_ERROR', {
      guild_id: 'g1',
      channel_id: 'c1',
      code: 'video_limit_reached',
      limit: 4,
      count: 4,
    });
    expect(sess().refusal).toMatchObject({ code: 'videoLimit' });
    h.c.dismissRefusal();
    // Stream ops are answered without a channel.
    h.emit('SR_VOICE_ERROR', {
      guild_id: null,
      channel_id: null,
      code: 'stream_full',
      limit: 2,
      count: 2,
    });
    expect(sess().refusal).toMatchObject({ code: 'viewerCap' });
    h.c.dismissRefusal();
    h.emit('SR_VOICE_ERROR', { guild_id: null, channel_id: null, code: 'unknown_stream' });
    expect(sess().status).toBe('connected');
    expect(sess().refusal).toBeNull();
  });

  it('gives up after repeated device losses', async () => {
    const h = harness();
    await connected(h);
    for (let i = 0; i < MAX_MIC_RECOVERIES; i++) {
      h.last().events.micLost();
      await Promise.resolve();
      await Promise.resolve();
    }
    expect(sess().status).toBe('connected');
    h.last().events.micLost();
    expect(sess().error).toMatchObject({ code: 'mic', mic: 'no-device' });
  });

  it('keeps a ping history for the graph, capped, and clears it on leave', async () => {
    const h = harness();
    await connected(h);
    for (let i = 0; i < PING_HISTORY + 5; i++)
      h.last().events.quality({ level: 'good', pingMs: 20 + i, lossPct: 0 });
    expect(sess().pingHistory).toHaveLength(PING_HISTORY);
    expect(sess().pingHistory.at(-1)).toBe(20 + PING_HISTORY + 4);
    h.last().events.quality({ level: 'unknown', pingMs: null, lossPct: null });
    expect(sess().pingHistory).toHaveLength(PING_HISTORY);
    h.c.leave();
    expect(sess().pingHistory).toEqual([]);
  });
});

describe('cues and settings', () => {
  const oscillators = (h: Harness) => h.audio.contexts.reduce((n, c) => n + c.oscillators, 0);

  it('plays join and leave sounds for other people, honouring the setting', async () => {
    const h = harness();
    await connected(h);
    const base = oscillators(h); // the self-join cue
    expect(base).toBeGreaterThan(0);
    h.emit('VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: 'c1', user_id: 'u2' });
    expect(oscillators(h)).toBe(base + 2);
    h.emit('VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: null, user_id: 'u2' });
    expect(oscillators(h)).toBe(base + 4);
    // Someone elsewhere is silent.
    h.emit('VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: 'c7', user_id: 'u3' });
    expect(oscillators(h)).toBe(base + 4);
    useVoiceSettings.getState().update({ soundJoinLeave: false });
    h.emit('VOICE_STATE_UPDATE', { guild_id: 'g1', channel_id: 'c1', user_id: 'u4' });
    expect(oscillators(h)).toBe(base + 4);
  });

  it('pushes settings changes to a live engine', async () => {
    const h = harness();
    useVoiceSettings.getState().update({ userVolumes: { u1: 180 }, localMuted: { u2: true } });
    const stop = h.c.start();
    await connected(h);
    const e = h.last();
    expect(e.volumes.u1).toBe(180);
    expect(e.muted.u2).toBe(true);
    const s = useVoiceSettings.getState();
    s.update({
      outputVolume: 70,
      inputVolume: 120,
      outputDeviceId: 'spk-2',
      inputDeviceId: 'mic-2',
    });
    s.setUserVolume('u1', 50);
    s.setLocalMuted('u2', false);
    expect(e.master.at(-1)).toBe(70);
    expect(e.input.at(-1)).toBe(120);
    expect(e.outputDevices.at(-1)).toBe('spk-2');
    expect(e.inputDevices).toHaveLength(1);
    expect(e.volumes.u1).toBe(50);
    expect(e.muted.u2).toBe(false);
    stop();
  });
});

describe('session defaults', () => {
  it('starts idle', () => {
    expect(IDLE_SESSION.status).toBe('idle');
  });
});
