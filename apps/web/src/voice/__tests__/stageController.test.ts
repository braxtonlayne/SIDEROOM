import { beforeEach, describe, expect, it } from 'vitest';
import { VoiceController, type ControllerDeps } from '../controller';
import type { EngineEvents, VoiceEngine } from '../engine';
import { resetSession, useVoiceSession } from '../session';
import { DEFAULT_VOICE_SETTINGS, useVoiceSettings } from '../settings';
import { fakeAudio } from './fakes';

class Engine implements VoiceEngine {
  status: VoiceEngine['status'] = 'connected';
  transmitting: boolean[] = [];
  constructor(readonly events: EngineEvents) {}
  async connect() {}
  disconnect() {}
  setTransmitting(t: boolean) {
    this.transmitting.push(t);
  }
  setDeafened() {}
  setUserVolume() {}
  setUserMuted() {}
  setMasterVolume() {}
  setInputVolume() {}
  async setInputDevice() {}
  async setOutputDevice() {}
  restartIce() {}
}

function harness() {
  const listeners = new Set<(e: string, d: unknown) => void>();
  const engines: Engine[] = [];
  const deps: ControllerDeps = {
    createEngine: (ev) => {
      const e = new Engine(ev);
      engines.push(e);
      return e;
    },
    sendGateway: () => true,
    subscribe: (l) => (listeners.add(l), () => listeners.delete(l)),
    currentUserId: () => 'me',
    voiceStatesOf: () => ({}),
    audio: fakeAudio().deps,
    now: () => 0,
  };
  const c = new VoiceController(deps);
  const emit = (e: string, d: unknown) => listeners.forEach((l) => l(e, d));
  c.join({ guildId: 'g1', channelId: 'st', name: 'Stage' });
  return { c, emit, engines };
}

beforeEach(() => {
  resetSession();
  useVoiceSettings.setState({ settings: DEFAULT_VOICE_SETTINGS });
});

describe('stage audience (VOX-STG-002)', () => {
  it('keeps the microphone shut while suppressed and says so when invited to speak', async () => {
    const { emit } = harness();
    emit('VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'st',
      user_id: 'me',
      suppress: true,
    });
    expect(useVoiceSession.getState().suppressed).toBe(true);
    expect(useVoiceSession.getState().transmitting).toBe(false);
    // Invited: the flag clears, the viewer is told, and stays muted until they unmute themselves.
    emit('VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'st',
      user_id: 'me',
      suppress: false,
    });
    expect(useVoiceSession.getState().suppressed).toBe(false);
    expect(useVoiceSession.getState().notice).toBe(
      'You were invited to speak. Unmute when you are ready.',
    );
    // Stepping down later says nothing.
    useVoiceSession.setState({ notice: null });
    emit('VOICE_STATE_UPDATE', {
      guild_id: 'g1',
      channel_id: 'st',
      user_id: 'me',
      suppress: true,
    });
    expect(useVoiceSession.getState().notice).toBeNull();
  });
});
