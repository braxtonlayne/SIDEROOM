import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { VoiceState } from '../../store/types';
import { VoiceController, type ControllerDeps } from '../controller';
import { type ConnectParams, type EngineEvents, type VoiceEngine } from '../engine';
import { vadHost } from '../mic';
import { resetSession, useVoiceSession } from '../session';
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
function joinAndServe(h: Harness, t = target, extra: Record<string, unknown> = {}) {
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
    ...extra,
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
  vi.restoreAllMocks();
  vi.useRealTimers();
});

/** Make the shared host report what a loaded detector would. */
function detector(over: {
  drives: boolean;
  open: boolean;
  state: 'idle' | 'loading' | 'ready' | 'fallback';
}) {
  vi.spyOn(vadHost, 'drives', 'get').mockReturnValue(over.drives);
  vi.spyOn(vadHost, 'open', 'get').mockReturnValue(over.open);
  vi.spyOn(vadHost, 'state', 'get').mockReturnValue(over.state);
  vi.spyOn(vadHost, 'role', 'get').mockReturnValue('call');
  (vadHost as unknown as { emit(): void }).emit();
}
const level = (h: Harness, db: number) => {
  h.clock.t += 30;
  h.last().events.level(db);
};

describe('the Silero detector beside the energy gate (D-1050, D-1051)', () => {
  beforeEach(() => {
    useVoiceSettings
      .getState()
      .update({ inputMode: 'vad', autoSensitivity: false, thresholdDb: -45, vadEngine: 'silero' });
  });

  it('hands the engine the call role for the detector, and nothing else changes in its options', async () => {
    const h = harness();
    h.c.start();
    await connected(h);
    expect(h.last().connectParams!.mic).toMatchObject({
      vadRole: 'call',
      deviceId: 'default',
      inputVolume: 100,
    });
  });

  it('while the model loads, the energy gate decides exactly as before', async () => {
    const h = harness();
    h.c.start();
    await connected(h);
    detector({ drives: false, open: false, state: 'loading' });
    level(h, -30);
    expect(sess().transmitting).toBe(true);
    h.clock.t += 400;
    level(h, -80);
    expect(sess().transmitting).toBe(false);
  });

  it('once ready, the detector decides: loud sound it calls silence stays closed, its speech opens', async () => {
    const h = harness();
    h.c.start();
    await connected(h);
    detector({ drives: true, open: false, state: 'ready' });
    level(h, -20);
    expect(sess().transmitting).toBe(false);
    expect(h.last().lastTransmit).not.toBe(true);
    detector({ drives: true, open: true, state: 'ready' });
    expect(sess().transmitting).toBe(true);
    expect(h.last().lastTransmit).toBe(true);
    level(h, -90); // silence on the meter, speech by the model: stays open
    expect(sess().transmitting).toBe(true);
    detector({ drives: true, open: false, state: 'ready' });
    expect(sess().transmitting).toBe(false);
  });

  it('on a fallback the energy gate takes over at once and one notice is shown, once per call', async () => {
    const h = harness();
    h.c.start();
    await connected(h);
    detector({ drives: true, open: false, state: 'ready' });
    level(h, -30);
    expect(sess().transmitting).toBe(false);
    detector({ drives: false, open: false, state: 'fallback' });
    expect(sess().transmitting).toBe(true); // the loud level it heard is now enough
    expect(sess().notice).toMatch(/Silero voice detector could not start/);
    h.c.dismissNotice();
    detector({ drives: false, open: false, state: 'fallback' });
    expect(sess().notice).toBeNull();
    // the next call may notice again
    h.c.leave();
    await connected(h);
    detector({ drives: false, open: false, state: 'fallback' });
    expect(sess().notice).toMatch(/Silero voice detector could not start/);
  });

  it('a fallback of the settings test is not a notice in the call', async () => {
    const h = harness();
    h.c.start();
    await connected(h);
    detector({ drives: false, open: false, state: 'fallback' });
    h.c.dismissNotice();
    vi.spyOn(vadHost, 'role', 'get').mockReturnValue('test');
    h.c.leave();
    expect(sess().notice).toBeNull();
  });

  it('Push to Talk is not touched: the detector never opens the microphone', async () => {
    const h = harness();
    h.c.start();
    useVoiceSettings.getState().update({ inputMode: 'ptt' });
    await connected(h);
    detector({ drives: true, open: true, state: 'ready' });
    expect(sess().transmitting).toBe(false);
  });
});
