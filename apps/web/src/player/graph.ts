/**
 * MUS-PLY-001 + MUS-LIB-001 (wave-30 integration): the Web Audio graph.
 *
 *   deck A `<audio>` -> MediaElementSource -> deck gain A --+
 *   deck B `<audio>` -> MediaElementSource -> deck gain B --+-> EQ chain (preamp, ten bands) -> master volume
 *                                                               -> balance (stereo panner) -> analyser
 *                                                               -> destination
 *
 * MUS-SKN-002 (D-525): the balance is a `StereoPannerNode` driven by the store's `pan`; a browser without
 * `createStereoPanner` keeps the old chain (master straight into the analyser) and the balance does nothing.
 *
 * The two decks are the gapless/crossfade pair of `music/queue/gapless.ts`: the deck gains are the only nodes
 * a track change moves (a cut on the audio clock, or the equal-power crossfade). The EQ is music-lib's chain
 * (`music/eq/eqGraph.ts`, with the headroom preamp and the 30 ms glide); its gains come from the player store
 * (`eq`, `eqEnabled`), the one source of truth the skins, the Equalizer section and the saved settings share.
 * The master gain carries the volume so the elements always play at 1; the analyser feeds the VU bar.
 * A graph with one deck (no second element) still works: the player then changes tracks with a plain cut.
 */
import { createEqChain, type EqChain, type EqContext } from '../music/eq/eqGraph';

export interface AudioGraph {
  ctx: AudioContext;
  /** One gain per deck, after that deck's media source. */
  deckGains: GainNode[];
  eq: EqChain;
  master: GainNode;
  /** Balance between master and analyser; null where the browser has no stereo panner. */
  panner: StereoPannerNode | null;
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

export function buildGraph(ctx: AudioContext, decks: readonly HTMLAudioElement[]): AudioGraph {
  const eq = createEqChain(ctx as unknown as EqContext);
  const deckGains = decks.map((audio, i) => {
    const source = ctx.createMediaElementSource(audio);
    const g = ctx.createGain();
    // Deck A starts audible; the gapless engine silences the idle deck before it prefetches into it.
    g.gain.value = i === 0 ? 1 : 0;
    source.connect(g);
    g.connect(eq.input as unknown as AudioNode);
    return g;
  });
  const master = ctx.createGain();
  const analyser = ctx.createAnalyser();
  analyser.fftSize = 64;
  analyser.smoothingTimeConstant = 0.8;
  (eq.output as unknown as AudioNode).connect(master);
  let panner: StereoPannerNode | null = null;
  if (typeof ctx.createStereoPanner === 'function') {
    panner = ctx.createStereoPanner();
    master.connect(panner);
    panner.connect(analyser);
  } else {
    master.connect(analyser);
  }
  analyser.connect(ctx.destination);
  return { ctx, deckGains, eq, master, panner, analyser };
}

/** The balance, -1 (left) .. 1 (right). */
export function applyPan(graph: AudioGraph, pan: number): void {
  if (graph.panner) graph.panner.pan.value = pan;
}

/** Apply EQ gains from the store; a disabled EQ is flat (and its preamp back at 0 dB). */
export function applyEq(graph: AudioGraph, gains: readonly number[], enabled: boolean): void {
  graph.eq.setGains(gains, enabled);
}

export function applyVolume(graph: AudioGraph, volume: number, muted: boolean): void {
  graph.master.gain.value = muted ? 0 : volume;
}

/** Loudness 0..1 from the analyser, for the VU bar. */
export function readLevel(graph: AudioGraph, scratch: Uint8Array): number {
  graph.analyser.getByteFrequencyData(scratch);
  let sum = 0;
  for (const v of scratch) sum += v;
  return scratch.length ? sum / scratch.length / 255 : 0;
}
