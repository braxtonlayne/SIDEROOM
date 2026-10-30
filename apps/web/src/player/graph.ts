/**
 * MUS-PLY-001: the Web Audio graph. `<audio>` -> MediaElementSource -> ten EQ bands -> crossfade gain ->
 * master volume -> analyser -> destination. The analyser feeds the VU bar; the crossfade gain is the node
 * the fade-out and fade-in around a track change moves; the master gain carries the volume so the element
 * itself always plays at 1.
 */
import { EQ_BANDS } from './types';

export interface AudioGraph {
  ctx: AudioContext;
  bands: BiquadFilterNode[];
  fade: GainNode;
  master: GainNode;
  analyser: AnalyserNode;
}

export type ContextFactory = () => AudioContext | null;

export const defaultContextFactory: ContextFactory = () => {
  const Ctor =
    typeof window === 'undefined'
      ? undefined
      : (window.AudioContext ??
        (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext);
  return Ctor ? new Ctor() : null;
};

export function buildGraph(ctx: AudioContext, audio: HTMLAudioElement): AudioGraph {
  const source = ctx.createMediaElementSource(audio);
  const last = EQ_BANDS.length - 1;
  const bands = EQ_BANDS.map((hz, i) => {
    const f = ctx.createBiquadFilter();
    f.type = i === 0 ? 'lowshelf' : i === last ? 'highshelf' : 'peaking';
    f.frequency.value = hz;
    f.Q.value = 1.1;
    f.gain.value = 0;
    return f;
  });
  const fade = ctx.createGain();
  const master = ctx.createGain();
  const analyser = ctx.createAnalyser();
  analyser.fftSize = 64;
  analyser.smoothingTimeConstant = 0.8;
  let node: AudioNode = source;
  for (const n of [...bands, fade, master, analyser]) {
    node.connect(n);
    node = n;
  }
  node.connect(ctx.destination);
  return { ctx, bands, fade, master, analyser };
}

/** Apply EQ gains; a disabled EQ is flat. */
export function applyEq(graph: AudioGraph, gains: readonly number[], enabled: boolean): void {
  graph.bands.forEach((b, i) => {
    b.gain.value = enabled ? (gains[i] ?? 0) : 0;
  });
}

export function applyVolume(graph: AudioGraph, volume: number, muted: boolean): void {
  graph.master.gain.value = muted ? 0 : volume;
}

/** Ramp the crossfade gain to `target` over `seconds` (0 = at once). */
export function rampFade(graph: AudioGraph, target: number, seconds: number): void {
  const g = graph.fade.gain;
  const now = graph.ctx.currentTime;
  g.cancelScheduledValues(now);
  g.setValueAtTime(g.value, now);
  if (seconds <= 0) g.setValueAtTime(target, now);
  else g.linearRampToValueAtTime(target, now + seconds);
}

/** Loudness 0..1 from the analyser, for the VU bar. */
export function readLevel(graph: AudioGraph, scratch: Uint8Array): number {
  graph.analyser.getByteFrequencyData(scratch);
  let sum = 0;
  for (const v of scratch) sum += v;
  return scratch.length ? sum / scratch.length / 255 : 0;
}
