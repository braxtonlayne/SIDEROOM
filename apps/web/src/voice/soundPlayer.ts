/**
 * Plays a soundboard sound to the person themselves (VOX-SB-003): the preview in the popover and the
 * sidetone of a sound they just sent. Separate from the call's audio graph, so it works outside a call and
 * follows the chosen output device.
 */
import type { AudioDeps } from './deps';

type SinkContext = AudioContext & { setSinkId?: (id: string) => Promise<void> };

export class LocalSoundPlayer {
  private ctx: AudioContext | null = null;
  private playing = new Set<AudioBufferSourceNode>();

  constructor(private readonly deps: AudioDeps) {}

  /** Play `data` (a sound file) at `volume` 0 to 1. Resolves false when it cannot be played. */
  async play(data: ArrayBuffer, volume: number, sinkId = 'default'): Promise<boolean> {
    this.ctx ??= this.deps.createAudioContext();
    const ctx = this.ctx;
    if (!ctx?.decodeAudioData) return false;
    if (sinkId !== 'default')
      void (ctx as SinkContext).setSinkId?.(sinkId)?.catch?.(() => undefined);
    let buffer: AudioBuffer;
    try {
      buffer = await ctx.decodeAudioData(data);
    } catch {
      return false;
    }
    if (this.ctx !== ctx) return false;
    void ctx.resume?.();
    const src = ctx.createBufferSource();
    src.buffer = buffer;
    const level = ctx.createGain();
    level.gain.value = Math.min(1, Math.max(0, volume));
    src.connect(level);
    level.connect(ctx.destination);
    this.playing.add(src);
    src.onended = () => {
      this.playing.delete(src);
      try {
        src.disconnect();
        level.disconnect();
      } catch {
        /* already disconnected */
      }
    };
    src.start();
    return true;
  }

  /** Stop everything that is playing (leaving the call, closing the popover). */
  stop(): void {
    for (const s of this.playing) {
      try {
        s.stop();
      } catch {
        /* not started */
      }
    }
    this.playing.clear();
  }

  dispose(): void {
    this.stop();
    void this.ctx?.close?.().catch(() => undefined);
    this.ctx = null;
  }
}
