/**
 * Cue sounds (LNG-USET-049): join / leave, mute / deafen and push-to-talk blips. Synthesized with WebAudio so
 * the app ships no audio files; each cue is a short sequence of tones.
 */
export type Cue =
  | 'join'
  | 'leave'
  | 'selfJoin'
  | 'selfLeave'
  | 'mute'
  | 'unmute'
  | 'deafen'
  | 'undeafen'
  | 'pttOn'
  | 'pttOff'
  | 'error';

export type CueGroup = 'joinLeave' | 'muteDeafen' | 'ptt' | 'always';

interface Tone {
  hz: number;
  ms: number;
}

export const CUES: Record<Cue, { group: CueGroup; tones: Tone[] }> = {
  join: {
    group: 'joinLeave',
    tones: [
      { hz: 523, ms: 70 },
      { hz: 784, ms: 110 },
    ],
  },
  leave: {
    group: 'joinLeave',
    tones: [
      { hz: 784, ms: 70 },
      { hz: 440, ms: 110 },
    ],
  },
  selfJoin: {
    group: 'joinLeave',
    tones: [
      { hz: 440, ms: 70 },
      { hz: 659, ms: 70 },
      { hz: 880, ms: 120 },
    ],
  },
  selfLeave: {
    group: 'joinLeave',
    tones: [
      { hz: 880, ms: 70 },
      { hz: 659, ms: 70 },
      { hz: 440, ms: 120 },
    ],
  },
  mute: { group: 'muteDeafen', tones: [{ hz: 400, ms: 90 }] },
  unmute: { group: 'muteDeafen', tones: [{ hz: 620, ms: 90 }] },
  deafen: {
    group: 'muteDeafen',
    tones: [
      { hz: 500, ms: 60 },
      { hz: 330, ms: 90 },
    ],
  },
  undeafen: {
    group: 'muteDeafen',
    tones: [
      { hz: 330, ms: 60 },
      { hz: 500, ms: 90 },
    ],
  },
  pttOn: { group: 'ptt', tones: [{ hz: 900, ms: 45 }] },
  pttOff: { group: 'ptt', tones: [{ hz: 600, ms: 45 }] },
  error: { group: 'always', tones: [{ hz: 220, ms: 160 }] },
};

export interface CueSettings {
  soundJoinLeave: boolean;
  soundMuteDeafen: boolean;
  soundPtt: boolean;
  /** Percent 0..100. */
  soundVolume: number;
}

export function cueEnabled(cue: Cue, s: CueSettings): boolean {
  switch (CUES[cue].group) {
    case 'joinLeave':
      return s.soundJoinLeave;
    case 'muteDeafen':
      return s.soundMuteDeafen;
    case 'ptt':
      return s.soundPtt;
    default:
      return true;
  }
}

type SinkContext = AudioContext & { setSinkId?: (id: string) => Promise<void> };

export class CuePlayer {
  private ctx: AudioContext | null = null;

  constructor(private readonly createContext: () => AudioContext | null) {}

  /** Play a cue at `volume` (0..1). Silent when WebAudio is unavailable or the volume is zero. */
  play(cue: Cue, volume: number): boolean {
    if (volume <= 0) return false;
    this.ctx ??= this.createContext();
    const ctx = this.ctx;
    if (!ctx) return false;
    void ctx.resume?.();
    let t = ctx.currentTime;
    for (const tone of CUES[cue].tones) {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      const dur = tone.ms / 1000;
      osc.type = 'sine';
      osc.frequency.value = tone.hz;
      gain.gain.setValueAtTime(0, t);
      gain.gain.linearRampToValueAtTime(Math.min(1, volume) * 0.35, t + 0.008);
      gain.gain.linearRampToValueAtTime(0, t + dur);
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.start(t);
      osc.stop(t + dur + 0.02);
      t += dur;
    }
    return true;
  }

  async setOutputDevice(deviceId: string): Promise<void> {
    const c = this.ctx as SinkContext | null;
    if (c?.setSinkId) await c.setSinkId(deviceId === 'default' ? '' : deviceId);
  }

  dispose(): void {
    void this.ctx?.close?.().catch(() => undefined);
    this.ctx = null;
  }
}
