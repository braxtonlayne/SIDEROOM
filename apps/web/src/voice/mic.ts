/**
 * Microphone capture: getUserMedia (echo cancellation + AGC by choice, never noise suppression), an input
 * gain stage, a level meter for the voice-activity gate and the settings test, and an on/off transmit gate.
 *
 * Graph: MediaStreamSource -> Analyser (meter) -> Gain (input volume) -> Gate gain -> MediaStreamDestination,
 * with soundboard sounds joining at the destination (VOX-SB-003).
 * The destination track is what the peer connection sends; disabling it is the "gate". The gate gain closes the
 * microphone path itself, so a sound can still go out (the track is enabled while it plays) while the voice
 * stays shut. Without WebAudio the raw track is sent and only the gate works.
 */
import { micConstraints, toMicError, type AudioDeps } from './deps';
import { SOUND_CLIP_SECONDS } from './soundPlayer';
import { useVoiceSession } from './session';
import { effectiveInputMode, getVoiceSettings, useVoiceSettings } from './settings';
import { rmsDb, SILENCE_DB, VadEngineHost, type VadDetector, type VadTapRole } from './vad';

export interface MicOptions {
  deviceId: string;
  echoCancellation: boolean;
  autoGainControl: boolean;
  /** Percent 0..200. */
  inputVolume: number;
  /** 'call' for the microphone of a call (it wins the detector over the settings test, the default). */
  vadRole?: VadTapRole;
}

export const METER_INTERVAL_MS = 30;

/**
 * D-1048, D-1050: the one place the optional Silero detector is started and stopped. A microphone graph registers
 * its tap (the context and the source node, before any gain: the same signal the energy gate's analyser reads) and
 * the host starts the detector only when the member chose Silero and voice activity is the input mode. The
 * detector's code is reached through a dynamic import, so with the Standard engine none of it is ever fetched.
 */
export const vadHost = new VadEngineHost({
  wanted: () => {
    const s = getVoiceSettings();
    // The EFFECTIVE mode: a channel that forces Push to Talk never starts the detector.
    return (
      s.vadEngine === 'silero' &&
      effectiveInputMode(s, useVoiceSession.getState().pttForced) === 'vad'
    );
  },
  engine: () => getVoiceSettings().vadEngine,
  load: () => import('./silero/sileroVad').then((m): VadDetector => m.createSileroVad()),
  now: () => performance.now(),
});
useVoiceSettings.subscribe((st, prev) => {
  if (
    st.settings.vadEngine !== prev.settings.vadEngine ||
    st.settings.inputMode !== prev.settings.inputMode
  )
    vadHost.refresh();
});
// Browser tests only (built with VITE_SR_E2E): the detector's figures, read by the end-to-end specs.
if (import.meta.env.VITE_SR_E2E && typeof window !== 'undefined') {
  Object.defineProperty(window, '__srVadStats', {
    configurable: true,
    get: () => vadHost.snapshot(),
  });
}

export class MicPipeline {
  private stream: MediaStream | null = null;
  private ctx: AudioContext | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private analyser: AnalyserNode | null = null;
  private gain: GainNode | null = null;
  private gateGain: GainNode | null = null;
  /** Sounds playing into the outgoing audio; the track stays enabled while any does. */
  private sounds = 0;
  private dest: MediaStreamAudioDestinationNode | null = null;
  private out: MediaStreamTrack | null = null;
  private timer: ReturnType<typeof setInterval> | undefined;
  private buf: Float32Array | null = null;
  private levelListeners = new Set<(db: number) => void>();
  private lostListeners = new Set<() => void>();
  private gateOpen = false;
  private lastDb = SILENCE_DB;

  constructor(private readonly deps: AudioDeps) {}

  get active(): boolean {
    return this.stream !== null;
  }
  get level(): number {
    return this.lastDb;
  }
  /** The track to publish (processed when WebAudio is available). */
  get track(): MediaStreamTrack | null {
    return this.out;
  }
  /** The stream carrying `track` (for `addTrack`). */
  get outStream(): MediaStream | null {
    return this.dest?.stream ?? this.stream;
  }

  onLevel(cb: (db: number) => void): () => void {
    this.levelListeners.add(cb);
    return () => this.levelListeners.delete(cb);
  }
  /** The capture track ended (device unplugged, permission revoked). */
  onLost(cb: () => void): () => void {
    this.lostListeners.add(cb);
    return () => this.lostListeners.delete(cb);
  }

  async start(o: MicOptions): Promise<MediaStreamTrack> {
    this.stop();
    let stream: MediaStream;
    try {
      stream = await this.deps.getUserMedia(micConstraints(o));
    } catch (e) {
      // A remembered device that is gone: fall back to the system default instead of failing.
      if (o.deviceId !== 'default' && isConstraintError(e)) {
        try {
          stream = await this.deps.getUserMedia(micConstraints({ ...o, deviceId: 'default' }));
        } catch (e2) {
          throw toMicError(e2);
        }
      } else {
        throw toMicError(e);
      }
    }
    const raw = stream.getAudioTracks()[0];
    if (!raw) {
      stream.getTracks().forEach((t) => t.stop());
      throw toMicError(new DOMException('no audio track', 'NotFoundError'));
    }
    this.stream = stream;
    raw.addEventListener('ended', this.onEnded);
    this.ctx = this.deps.createAudioContext();
    if (this.ctx) {
      const ctx = this.ctx;
      this.source = ctx.createMediaStreamSource(stream);
      this.analyser = ctx.createAnalyser();
      this.analyser.fftSize = 1024;
      this.buf = new Float32Array(this.analyser.fftSize);
      this.gain = ctx.createGain();
      this.gain.gain.value = o.inputVolume / 100;
      this.gateGain = ctx.createGain();
      this.gateGain.gain.value = this.gateOpen ? 1 : 0;
      this.dest = ctx.createMediaStreamDestination();
      this.source.connect(this.analyser);
      this.analyser.connect(this.gain);
      this.gain.connect(this.gateGain);
      this.gateGain.connect(this.dest);
      this.out = this.dest.stream.getAudioTracks()[0] ?? raw;
      void ctx.resume?.();
      vadHost.setTap(this, o.vadRole ?? 'test', ctx, this.source);
    } else {
      this.out = raw;
    }
    this.out.enabled = this.gateOpen || this.sounds > 0;
    this.timer = setInterval(this.sample, METER_INTERVAL_MS);
    return this.out;
  }

  private readonly onEnded = () => {
    for (const cb of [...this.lostListeners]) cb();
  };

  private readonly sample = () => {
    if (!this.analyser || !this.buf) return;
    this.analyser.getFloatTimeDomainData(this.buf);
    this.lastDb = rmsDb(this.buf);
    for (const cb of [...this.levelListeners]) cb(this.lastDb);
  };

  /** Open or close the transmit gate (mute, push-to-talk, voice activity). */
  setGate(open: boolean): void {
    this.gateOpen = open;
    if (this.gateGain) this.gateGain.gain.value = open ? 1 : 0;
    if (this.out) this.out.enabled = open || this.sounds > 0;
  }

  /**
   * Decode a sound file and mix it into the outgoing audio at `volume` (0 to 1), whether or not the voice gate is
   * open. Resolves false when there is no WebAudio, no open microphone or the file does not decode.
   */
  async playSound(data: ArrayBuffer, volume: number): Promise<boolean> {
    const ctx = this.ctx;
    const dest = this.dest;
    if (!ctx || !dest || !ctx.decodeAudioData) return false;
    let buffer: AudioBuffer;
    try {
      buffer = await ctx.decodeAudioData(data);
    } catch {
      return false;
    }
    if (this.ctx !== ctx) return false; // the call ended while decoding
    const src = ctx.createBufferSource();
    src.buffer = buffer;
    const level = ctx.createGain();
    level.gain.value = Math.min(1, Math.max(0, volume));
    src.connect(level);
    level.connect(dest);
    this.sounds++;
    if (this.out) this.out.enabled = true;
    src.onended = () => {
      this.sounds = Math.max(0, this.sounds - 1);
      if (this.out) this.out.enabled = this.gateOpen || this.sounds > 0;
      try {
        src.disconnect();
        level.disconnect();
      } catch {
        /* already disconnected */
      }
    };
    src.start(0, 0, SOUND_CLIP_SECONDS);
    return true;
  }

  setInputVolume(percent: number): void {
    if (this.gain) this.gain.gain.value = Math.max(0, percent) / 100;
  }

  stop(): void {
    vadHost.clearTap(this);
    clearInterval(this.timer);
    this.timer = undefined;
    this.stream?.getAudioTracks().forEach((t) => t.removeEventListener('ended', this.onEnded));
    this.stream?.getTracks().forEach((t) => t.stop());
    this.out?.stop?.();
    try {
      this.source?.disconnect();
      this.analyser?.disconnect();
      this.gain?.disconnect();
      this.gateGain?.disconnect();
    } catch {
      /* already disconnected */
    }
    void this.ctx?.close?.().catch(() => undefined);
    this.stream = this.ctx = this.source = this.analyser = this.gain = this.dest = this.out = null;
    this.gateGain = null;
    this.sounds = 0;
    this.buf = null;
    this.lastDb = SILENCE_DB;
  }
}

function isConstraintError(e: unknown): boolean {
  const name = e instanceof Error ? e.name : '';
  return name === 'OverconstrainedError' || name === 'NotFoundError';
}
