// @ts-check
// Sound recipes, synthesized with Web Audio (no audio files).
// Each recipe builds and starts its nodes into `out` (a GainNode of the right bus) at time `t`, and returns the
// sound's length in seconds. Envelopes start from a floor rather than 0 because exponential ramps cannot reach 0.

/**
 * @typedef {import('./audio.js').SfxOptions & { detune?: number }} RecipeOptions
 *   `detune` is in cents and is applied to every oscillator a recipe creates.
 */

/**
 * @typedef {(ctx: BaseAudioContext, out: AudioNode, t: number, opts: RecipeOptions) => number} Recipe
 *   returns the sound's length in seconds
 */

const FLOOR = 0.0001;

/** One shared white-noise buffer per context: building it costs far more than starting a source on it. @type {WeakMap<BaseAudioContext, AudioBuffer>} */
const noiseCache = new WeakMap();

/** @param {BaseAudioContext} ctx @returns {AudioBuffer} */
function noiseBuffer(ctx) {
  let buf = noiseCache.get(ctx);
  if (!buf) {
    buf = ctx.createBuffer(1, ctx.sampleRate * 2, ctx.sampleRate);
    const data = buf.getChannelData(0);
    for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
    noiseCache.set(ctx, buf);
  }
  return buf;
}

/**
 * A gain stage with a linear attack (at least 2 ms, to avoid clicks) and an exponential release to the floor.
 * @param {BaseAudioContext} ctx
 * @param {AudioNode} dest
 * @param {number} t
 * @param {number} peak
 * @param {number} attack
 * @param {number} dur
 * @returns {GainNode}
 */
function shaped(ctx, dest, t, peak, attack, dur) {
  const g = ctx.createGain();
  const a = Math.min(Math.max(attack, 0.002), dur * 0.8);
  g.gain.setValueAtTime(FLOOR, t);
  g.gain.linearRampToValueAtTime(peak, t + a);
  g.gain.exponentialRampToValueAtTime(FLOOR, t + dur);
  g.connect(dest);
  return g;
}

/**
 * @param {BaseAudioContext} ctx
 * @param {AudioNode} dest
 * @param {{ type: OscillatorType, freq: number, t: number, dur: number, to?: number, detune?: number }} p
 *   `to` glides the pitch exponentially to that frequency over `dur`.
 * @returns {OscillatorNode}
 */
function tone(ctx, dest, p) {
  const o = ctx.createOscillator();
  o.type = p.type;
  o.frequency.setValueAtTime(p.freq, p.t);
  if (p.to !== undefined) o.frequency.exponentialRampToValueAtTime(p.to, p.t + p.dur);
  o.detune.value = p.detune ?? 0;
  o.connect(dest);
  o.start(p.t);
  o.stop(p.t + p.dur);
  return o;
}

/**
 * @param {BaseAudioContext} ctx
 * @param {AudioNode} dest
 * @param {number} t
 * @param {number} dur
 */
function noise(ctx, dest, t, dur) {
  const s = ctx.createBufferSource();
  s.buffer = noiseBuffer(ctx);
  s.connect(dest);
  s.start(t);
  s.stop(t + dur);
}

/**
 * @param {BaseAudioContext} ctx
 * @param {AudioNode} dest
 * @param {BiquadFilterType} type
 * @param {number} freq
 * @param {number} [q]
 * @returns {BiquadFilterNode}
 */
function filter(ctx, dest, type, freq, q = 1) {
  const f = ctx.createBiquadFilter();
  f.type = type;
  f.frequency.value = freq;
  f.Q.value = q;
  f.connect(dest);
  return f;
}

/** @param {AudioParam} param @param {number} from @param {number} to @param {number} t @param {number} dur */
function sweep(param, from, to, t, dur) {
  param.setValueAtTime(from, t);
  param.exponentialRampToValueAtTime(to, t + dur);
}

/** @param {number} v @param {number} lo @param {number} hi */
const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** @type {Record<string, Recipe>} */
export const RECIPES = {
  turn(ctx, out, t, opts) {
    const dur = 0.025;
    const g = shaped(ctx, out, t, 0.05, 0.002, dur);
    tone(ctx, g, { type: 'sine', freq: 1800, t, dur, detune: opts.detune ?? 0 });
    return dur;
  },

  leave(ctx, out, t, opts) {
    const dur = 0.08;
    const g = shaped(ctx, out, t, 0.25, 0.003, dur);
    tone(ctx, g, { type: 'triangle', freq: 160, to: 220, t, dur, detune: opts.detune ?? 0 });
    return dur;
  },

  capture(ctx, out, t, opts) {
    const s = clamp(opts.size ?? 0, 0, 1);
    const d = opts.detune ?? 0;
    const base = 520 * 2 ** ((7 * s) / 12);
    const dur = 0.18 + 0.27 * s;
    /** A triangle note with a soft sine an octave up, so it has body without going harsh. */
    const note = (/** @type {number} */ t0, /** @type {number} */ f, /** @type {number} */ len) => {
      const g = shaped(ctx, out, t0, 0.22, 0.005, len);
      tone(ctx, g, { type: 'triangle', freq: f, t: t0, dur: len, detune: d });
      const oct = ctx.createGain();
      oct.gain.value = 0.35;
      oct.connect(g);
      tone(ctx, oct, { type: 'sine', freq: 2 * f, t: t0, dur: len, detune: d });
    };
    note(t, base, dur * 0.6);
    note(t + dur * 0.35, base * 1.5, dur * 0.65);
    return dur;
  },

  bigCapture(ctx, out, t, opts) {
    const dur = 0.6;
    const d = opts.detune ?? 0;
    const g = shaped(ctx, out, t, 0.18, 0.08, dur);
    // C major triad, each voice detuned a few cents against its neighbours for the swell.
    const chord = [
      { f: 523.25, cents: -6 },
      { f: 659.25, cents: 0 },
      { f: 783.99, cents: 6 },
    ];
    for (const { f, cents } of chord) {
      tone(ctx, g, { type: 'triangle', freq: f, t, dur, detune: d + cents });
    }
    return dur;
  },

  kill(ctx, out, t, opts) {
    const d = opts.detune ?? 0;
    const g1 = shaped(ctx, out, t, 0.3, 0.002, 0.06);
    noise(ctx, filter(ctx, g1, 'bandpass', 900, 1.2), t, 0.06);
    const g2 = shaped(ctx, out, t, 0.12, 0.002, 0.09);
    tone(ctx, g2, { type: 'square', freq: 330, to: 660, t, dur: 0.09, detune: d });
    const g3 = shaped(ctx, out, t, 0.1, 0.002, 0.12);
    tone(ctx, g3, { type: 'sine', freq: 1760, t, dur: 0.12, detune: d });
    return 0.12;
  },

  death(ctx, out, t, opts) {
    const dur = 0.7;
    const d = opts.detune ?? 0;
    const g = shaped(ctx, out, t, 0.22, 0.01, dur);
    const lp = filter(ctx, g, 'lowpass', 4000, 1);
    sweep(lp.frequency, 4000, 300, t, dur);
    tone(ctx, lp, { type: 'sawtooth', freq: 440, to: 70, t, dur, detune: d });
    // The low thump under the sweep is the body hitting the ground.
    const gt = shaped(ctx, out, t, 0.25, 0.003, 0.2);
    noise(ctx, filter(ctx, gt, 'lowpass', 200), t, 0.2);
    return dur;
  },

  enemyDeath(ctx, out, t, opts) {
    const dur = 0.12;
    const g = shaped(ctx, out, t, 0.2, 0.003, dur);
    noise(ctx, filter(ctx, g, 'lowpass', 600), t, dur);
    const gs = shaped(ctx, out, t, 0.2, 0.003, dur);
    tone(ctx, gs, { type: 'sine', freq: 110, t, dur, detune: opts.detune ?? 0 });
    return dur;
  },

  cardOffer(ctx, out, t, opts) {
    const d = opts.detune ?? 0;
    const notes = [
      { f: 880, at: 0 },
      { f: 1108, at: 0.09 },
      { f: 1318, at: 0.18 },
    ];
    for (const { f, at } of notes) {
      const g = shaped(ctx, out, t + at, 0.15, 0.005, 0.12);
      tone(ctx, g, { type: 'sine', freq: f, t: t + at, dur: 0.12, detune: d });
    }
    return 0.3;
  },

  cardPick(ctx, out, t, opts) {
    const gc = shaped(ctx, out, t, 0.3, 0.002, 0.01);
    noise(ctx, gc, t, 0.01);
    const g = shaped(ctx, out, t, 0.2, 0.003, 0.15);
    tone(ctx, g, { type: 'sine', freq: 1318, t, dur: 0.15, detune: opts.detune ?? 0 });
    return 0.15;
  },

  denied(ctx, out, t, opts) {
    const dur = 0.12;
    const g = shaped(ctx, out, t, 0.08, 0.003, dur);
    const lp = filter(ctx, g, 'lowpass', 600);
    tone(ctx, lp, { type: 'square', freq: 110, t, dur, detune: opts.detune ?? 0 });
    return dur;
  },

  itemUse(ctx, out, t, opts) {
    const dur = 0.09;
    const g = shaped(ctx, out, t, 0.2, 0.003, dur);
    tone(ctx, g, { type: 'sine', freq: 600, to: 900, t, dur, detune: opts.detune ?? 0 });
    return dur;
  },

  dash(ctx, out, t) {
    const dur = 0.35;
    const g = shaped(ctx, out, t, 0.25, 0.04, dur);
    const bp = filter(ctx, g, 'bandpass', 400);
    sweep(bp.frequency, 400, 2500, t, dur);
    noise(ctx, bp, t, dur);
    return dur;
  },

  grenadeThrow(ctx, out, t, opts) {
    const dur = 0.25;
    const g = shaped(ctx, out, t, 0.18, 0.005, dur);
    tone(ctx, g, { type: 'sine', freq: 700, to: 1300, t, dur, detune: opts.detune ?? 0 });
    return dur;
  },

  grenadeBoom(ctx, out, t, opts) {
    const d = opts.detune ?? 0;
    const s = clamp(opts.size ?? 0.5, 0, 1);
    const dur = 0.6 * (0.7 + 0.6 * s);
    const g = shaped(ctx, out, t, 0.3, 0.002, dur);
    const lp = filter(ctx, g, 'lowpass', 3000, 0.7);
    sweep(lp.frequency, 3000, 200, t, dur);
    noise(ctx, lp, t, dur);
    const gt = shaped(ctx, out, t, 0.25, 0.002, dur);
    tone(ctx, gt, { type: 'sine', freq: 60, t, dur, detune: d });
    return dur;
  },

  snare(ctx, out, t, opts) {
    const dur = 0.08;
    const d = opts.detune ?? 0;
    const g = shaped(ctx, out, t, 0.1, 0.002, dur);
    tone(ctx, g, { type: 'square', freq: 1200, t, dur, detune: d });
    tone(ctx, g, { type: 'square', freq: 1700, t, dur, detune: d });
    const gc = shaped(ctx, out, t, 0.2, 0.002, 0.01);
    noise(ctx, gc, t, 0.01);
    return dur;
  },

  wall(ctx, out, t, opts) {
    const dur = 0.15;
    const g = shaped(ctx, out, t, 0.3, 0.002, dur);
    noise(ctx, filter(ctx, g, 'lowpass', 800), t, dur);
    const gs = shaped(ctx, out, t, 0.3, 0.002, dur);
    tone(ctx, gs, { type: 'sine', freq: 90, t, dur, detune: opts.detune ?? 0 });
    return dur;
  },

  shieldBlock(ctx, out, t, opts) {
    const dur = 0.5;
    const d = opts.detune ?? 0;
    const g = shaped(ctx, out, t, 0.14, 0.002, dur);
    tone(ctx, g, { type: 'sine', freq: 1568, t, dur, detune: d });
    tone(ctx, g, { type: 'sine', freq: 2093, t, dur, detune: d });
    return dur;
  },

  emp(ctx, out, t, opts) {
    const dur = 0.3;
    const d = opts.detune ?? 0;
    const g = shaped(ctx, out, t, 0.18, 0.003, dur);
    // Tremolo: a 30 Hz LFO moves the saw's gain between 0.5 and 1.0.
    const trem = ctx.createGain();
    trem.gain.value = 0.75;
    trem.connect(g);
    tone(ctx, trem, { type: 'sawtooth', freq: 1200, to: 80, t, dur, detune: d });
    const lfo = ctx.createOscillator();
    lfo.frequency.value = 30;
    const depth = ctx.createGain();
    depth.gain.value = 0.25;
    lfo.connect(depth);
    depth.connect(trem.gain);
    lfo.start(t);
    lfo.stop(t + dur);
    return dur;
  },

  rewind(ctx, out, t) {
    const dur = 0.35;
    const g = shaped(ctx, out, t, 0.25, 0.25, dur);
    const bp = filter(ctx, g, 'bandpass', 2500);
    sweep(bp.frequency, 2500, 400, t, dur);
    noise(ctx, bp, t, dur);
    return dur;
  },

  phase(ctx, out, t, opts) {
    const dur = 0.4;
    const g = shaped(ctx, out, t, 0.2, 0.02, dur);
    const carrier = tone(ctx, g, { type: 'sine', freq: 500, t, dur, detune: opts.detune ?? 0 });
    // 8 Hz vibrato of +-40 Hz on the carrier pitch.
    const lfo = ctx.createOscillator();
    lfo.frequency.value = 8;
    const depth = ctx.createGain();
    depth.gain.value = 40;
    lfo.connect(depth);
    depth.connect(carrier.frequency);
    lfo.start(t);
    lfo.stop(t + dur);
    return dur;
  },

  secondWind(ctx, out, t, opts) {
    const d = opts.detune ?? 0;
    // C5 E5 G5 C6, each note rings for 0.3 s so the last one ends at 0.69 s.
    const notes = [523.25, 659.25, 783.99, 1046.5];
    notes.forEach((f, i) => {
      const at = t + i * 0.13;
      const g = shaped(ctx, out, at, 0.15, 0.005, 0.3);
      tone(ctx, g, { type: 'triangle', freq: f, t: at, dur: 0.3, detune: d });
    });
    return 0.7;
  },

  mirage(ctx, out, t, opts) {
    const dur = 0.3;
    const d = opts.detune ?? 0;
    const g = shaped(ctx, out, t, 0.25, 0.05, dur);
    // Two sines 4 Hz apart: the slow beat between them is the shimmer.
    tone(ctx, g, { type: 'sine', freq: 1046, t, dur, detune: d });
    tone(ctx, g, { type: 'sine', freq: 1050, t, dur, detune: d });
    return dur;
  },

  claimflag(ctx, out, t) {
    for (const off of [0, 0.03, 0.06]) {
      const g = shaped(ctx, out, t + off, 0.25, 0.003, 0.04);
      noise(ctx, filter(ctx, g, 'bandpass', 1500, 2), t + off, 0.04);
    }
    return 0.1;
  },

  uiClick(ctx, out, t) {
    const dur = 0.015;
    const g = shaped(ctx, out, t, 0.15, 0.002, dur);
    noise(ctx, g, t, dur);
    return dur;
  },
};

/** Which bus each sound goes to: 'ui' for interface sounds, else 'sfx'. @type {Record<string, 'sfx' | 'ui'>} */
export const BUS = {
  turn: 'ui',
  leave: 'sfx',
  capture: 'sfx',
  bigCapture: 'sfx',
  kill: 'sfx',
  death: 'sfx',
  enemyDeath: 'sfx',
  cardOffer: 'ui',
  cardPick: 'ui',
  denied: 'ui',
  itemUse: 'sfx',
  dash: 'sfx',
  grenadeThrow: 'sfx',
  grenadeBoom: 'sfx',
  snare: 'sfx',
  wall: 'sfx',
  shieldBlock: 'sfx',
  emp: 'sfx',
  rewind: 'sfx',
  phase: 'sfx',
  secondWind: 'sfx',
  mirage: 'sfx',
  claimflag: 'sfx',
  uiClick: 'ui',
};
