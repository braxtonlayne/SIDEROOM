/**
 * Remote audio playback (VOX-CLI-004/005): one <audio> element per person plus a WebAudio gain stage, so
 * per-user volume can go to 200% and local mute / deafen are instant.
 *
 * Why both: Chrome only delivers a remote WebRTC stream to WebAudio when the stream is also attached to a
 * media element, so the element stays muted and the audio is heard through the gain graph. Without WebAudio
 * the element itself plays (volume capped at 100%) and carries mute / deafen.
 */
import type { AudioDeps } from './deps';

interface Voice {
  el: HTMLAudioElement;
  source: MediaStreamAudioSourceNode | null;
  gain: GainNode | null;
  volume: number;
  muted: boolean;
}

type SinkElement = HTMLAudioElement & { setSinkId?: (id: string) => Promise<void> };
type SinkContext = AudioContext & { setSinkId?: (id: string) => Promise<void> };

/** Gain applied to other voices while a priority speaker talks. */
export const DUCK = 0.4;

export class RemoteAudio {
  private voices = new Map<string, Voice>();
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private masterVolume = 100;
  private deafened = false;
  private sinkId = 'default';
  /** People speaking with the priority bit: everyone else is ducked while any of them talks. */
  private priority = new Set<string>();

  constructor(private readonly deps: AudioDeps) {}

  get users(): string[] {
    return [...this.voices.keys()];
  }

  private context(): AudioContext | null {
    if (this.ctx) return this.ctx;
    this.ctx = this.deps.createAudioContext();
    if (this.ctx) {
      this.master = this.ctx.createGain();
      this.master.connect(this.ctx.destination);
      this.applyMaster();
      void (this.ctx as SinkContext).setSinkId?.(this.sink())?.catch?.(() => undefined);
    }
    return this.ctx;
  }

  private sink = () => (this.sinkId === 'default' ? '' : this.sinkId);

  add(userId: string, stream: MediaStream, volume = 100, muted = false): void {
    this.remove(userId);
    const el = this.deps.createAudioElement();
    el.srcObject = stream;
    el.autoplay = true;
    const ctx = this.context();
    const v: Voice = { el, source: null, gain: null, volume, muted };
    if (ctx && this.master) {
      el.muted = true;
      v.source = ctx.createMediaStreamSource(stream);
      v.gain = ctx.createGain();
      v.source.connect(v.gain);
      v.gain.connect(this.master);
      void ctx.resume?.();
    }
    void (el as SinkElement).setSinkId?.(this.sink())?.catch?.(() => undefined);
    this.voices.set(userId, v);
    this.apply(userId, v);
    void el.play?.()?.catch?.(() => undefined);
  }

  remove(userId: string): void {
    const v = this.voices.get(userId);
    if (!v) return;
    this.voices.delete(userId);
    this.priority.delete(userId);
    try {
      v.source?.disconnect();
      v.gain?.disconnect();
    } catch {
      /* already disconnected */
    }
    v.el.pause?.();
    v.el.srcObject = null;
  }

  has(userId: string): boolean {
    return this.voices.has(userId);
  }

  setVolume(userId: string, percent: number): void {
    const v = this.voices.get(userId);
    if (!v) return;
    v.volume = percent;
    this.apply(userId, v);
  }

  setMuted(userId: string, muted: boolean): void {
    const v = this.voices.get(userId);
    if (!v) return;
    v.muted = muted;
    this.apply(userId, v);
  }

  /** Priority speaker (spec 4.3): other voices are ducked while a priority speaker is talking. */
  setPriority(userId: string, on: boolean): void {
    const had = this.priority.has(userId);
    if (on === had) return;
    if (on) this.priority.add(userId);
    else this.priority.delete(userId);
    for (const [id, v] of this.voices) this.apply(id, v);
  }

  setMasterVolume(percent: number): void {
    this.masterVolume = percent;
    this.applyMaster();
    for (const [id, v] of this.voices) this.apply(id, v);
  }

  /** Deafen silences every remote element at once. */
  setDeafened(deafened: boolean): void {
    this.deafened = deafened;
    this.applyMaster();
    for (const [id, v] of this.voices) this.apply(id, v);
  }

  async setOutputDevice(deviceId: string): Promise<void> {
    this.sinkId = deviceId || 'default';
    const id = this.sink();
    const jobs: Promise<void>[] = [];
    const c = this.ctx as SinkContext | null;
    if (c?.setSinkId) jobs.push(c.setSinkId(id));
    for (const v of this.voices.values()) {
      const el = v.el as SinkElement;
      if (el.setSinkId) jobs.push(el.setSinkId(id));
    }
    await Promise.all(jobs);
  }

  /** Whether this browser can route output to a chosen device. */
  static canSelectOutput(): boolean {
    return typeof HTMLMediaElement !== 'undefined' && 'setSinkId' in HTMLMediaElement.prototype;
  }

  private applyMaster(): void {
    if (this.master) this.master.gain.value = this.deafened ? 0 : this.masterVolume / 100;
  }

  private apply(userId: string, v: Voice): void {
    const silent = this.deafened || v.muted;
    const ducked = this.priority.size > 0 && !this.priority.has(userId) ? DUCK : 1;
    const level = (v.volume / 100) * ducked;
    if (v.gain) {
      v.gain.gain.value = silent ? 0 : level;
      // The element only keeps the stream flowing; it must never double the sound.
      v.el.muted = true;
    } else {
      v.el.muted = silent;
      v.el.volume = Math.min(1, level * (this.masterVolume / 100));
    }
  }

  dispose(): void {
    for (const id of [...this.voices.keys()]) this.remove(id);
    this.master?.disconnect();
    void this.ctx?.close?.().catch(() => undefined);
    this.ctx = this.master = null;
  }
}
