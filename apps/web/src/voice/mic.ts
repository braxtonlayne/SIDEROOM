/**
 * Microphone capture: getUserMedia (echo cancellation + AGC by choice, never noise suppression), an input
 * gain stage, a level meter for the voice-activity gate and the settings test, and an on/off transmit gate.
 *
 * Graph: MediaStreamSource -> Analyser (meter) -> Gain (input volume) -> MediaStreamDestination.
 * The destination track is what the peer connection sends; disabling it is the "gate". Without WebAudio the
 * raw track is sent and only the gate works.
 */
import { micConstraints, toMicError, type AudioDeps } from './deps';
import { rmsDb, SILENCE_DB } from './vad';

export interface MicOptions {
  deviceId: string;
  echoCancellation: boolean;
  autoGainControl: boolean;
  /** Percent 0..200. */
  inputVolume: number;
}

export const METER_INTERVAL_MS = 30;

export class MicPipeline {
  private stream: MediaStream | null = null;
  private ctx: AudioContext | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private analyser: AnalyserNode | null = null;
  private gain: GainNode | null = null;
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
      this.dest = ctx.createMediaStreamDestination();
      this.source.connect(this.analyser);
      this.analyser.connect(this.gain);
      this.gain.connect(this.dest);
      this.out = this.dest.stream.getAudioTracks()[0] ?? raw;
      void ctx.resume?.();
    } else {
      this.out = raw;
    }
    this.out.enabled = this.gateOpen;
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
    if (this.out) this.out.enabled = open;
  }

  setInputVolume(percent: number): void {
    if (this.gain) this.gain.gain.value = Math.max(0, percent) / 100;
  }

  stop(): void {
    clearInterval(this.timer);
    this.timer = undefined;
    this.stream?.getAudioTracks().forEach((t) => t.removeEventListener('ended', this.onEnded));
    this.stream?.getTracks().forEach((t) => t.stop());
    this.out?.stop?.();
    try {
      this.source?.disconnect();
      this.analyser?.disconnect();
      this.gain?.disconnect();
    } catch {
      /* already disconnected */
    }
    void this.ctx?.close?.().catch(() => undefined);
    this.stream = this.ctx = this.source = this.analyser = this.gain = this.dest = this.out = null;
    this.buf = null;
    this.lastDb = SILENCE_DB;
  }
}

function isConstraintError(e: unknown): boolean {
  const name = e instanceof Error ? e.name : '';
  return name === 'OverconstrainedError' || name === 'NotFoundError';
}
