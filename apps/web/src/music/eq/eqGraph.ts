/**
 * MUS-LIB-001: the 10-band EQ as a Web Audio chain: preamp (headroom) -> low shelf -> 8 peaking filters ->
 * high shelf. `createEqChain` builds it on any context with the three factory methods it needs (a real
 * `AudioContext`, or a fake in tests) and returns the two ends to connect. `setGains` is the only thing the
 * player calls while music plays, so a preset change is a few `AudioParam` writes, never a rebuild.
 *
 * The player's own graph (`player/graph.ts`) builds the same ten filters inline; switching it to this chain
 * adds the preamp and a smooth ramp, and is the integrator's one-line change (see the handoff).
 */
import { EQ_FREQS, dbToGain, normalizeGains, preampDb } from './presets';

/** Quality factor of the peaking bands (about one octave wide at the octave spacing). */
export const EQ_Q = 1.1;
/** Seconds over which a gain change glides, so moving a slider does not click. */
export const EQ_RAMP_SEC = 0.03;

export interface EqParam {
  value: number;
  setTargetAtTime?(target: number, startTime: number, timeConstant: number): unknown;
}

export interface EqNode {
  connect(destination: EqNode): unknown;
  disconnect?(): unknown;
}

export interface EqFilter extends EqNode {
  type: string;
  frequency: EqParam;
  Q: EqParam;
  gain: EqParam;
}

export interface EqGain extends EqNode {
  gain: EqParam;
}

/** What the chain needs from an `AudioContext`. */
export interface EqContext {
  readonly currentTime: number;
  createBiquadFilter(): EqFilter;
  createGain(): EqGain;
}

export interface EqChain {
  /** Connect the source here. */
  readonly input: EqNode;
  /** Connect this to the next stage. */
  readonly output: EqNode;
  readonly bands: readonly EqFilter[];
  readonly preamp: EqGain;
  /** Apply gains in dB (ten values, clamped); `enabled = false` makes the chain flat (a straight wire in effect). */
  setGains(gains: readonly number[], enabled?: boolean): void;
  /** Take the chain apart. */
  dispose(): void;
}

export function createEqChain(ctx: EqContext): EqChain {
  const last = EQ_FREQS.length - 1;
  const preamp = ctx.createGain();
  const bands = EQ_FREQS.map((hz, i) => {
    const f = ctx.createBiquadFilter();
    f.type = i === 0 ? 'lowshelf' : i === last ? 'highshelf' : 'peaking';
    f.frequency.value = hz;
    f.Q.value = EQ_Q;
    f.gain.value = 0;
    return f;
  });
  let node: EqNode = preamp;
  for (const b of bands) {
    node.connect(b);
    node = b;
  }

  const write = (param: EqParam, target: number) => {
    if (param.setTargetAtTime) param.setTargetAtTime(target, ctx.currentTime, EQ_RAMP_SEC / 3);
    else param.value = target;
  };

  return {
    input: preamp,
    output: bands[last]!,
    bands,
    preamp,
    setGains(gains, enabled = true) {
      const g = enabled ? normalizeGains(gains) : normalizeGains([]);
      bands.forEach((b, i) => write(b.gain, g[i]!));
      write(preamp.gain, dbToGain(preampDb(g)));
    },
    dispose() {
      preamp.disconnect?.();
      for (const b of bands) b.disconnect?.();
    },
  };
}
