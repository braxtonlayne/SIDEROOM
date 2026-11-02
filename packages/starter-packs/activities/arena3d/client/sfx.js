// @ts-check
// Synthesised sound effects (Web Audio). No audio files: every sound is an oscillator or filtered noise made
// here, so there is nothing third-party to license. The AudioContext starts on the first user gesture (Play).

export class Sfx {
  constructor() {
    /** @type {AudioContext | null} */
    this.ctx = null;
    /** @type {GainNode | null} */
    this.master = null;
    this.volume = 0.6;
  }

  /** Call from a click/keypress. */
  unlock() {
    if (this.ctx) return;
    const AC = /** @type {any} */ (globalThis).AudioContext ?? /** @type {any} */ (globalThis).webkitAudioContext;
    if (!AC) return;
    this.ctx = /** @type {AudioContext} */ (new AC());
    this.master = this.ctx.createGain();
    this.master.gain.value = this.volume;
    this.master.connect(this.ctx.destination);
  }

  /** @param {number} v 0..1 */
  setVolume(v) {
    this.volume = v;
    if (this.master) this.master.gain.value = v;
  }

  /** @param {number} f0 @param {number} f1 @param {number} dur @param {OscillatorType} type @param {number} gain */
  tone(f0, f1, dur, type, gain) {
    const ctx = this.ctx;
    if (!ctx || !this.master) return;
    const t = ctx.currentTime;
    const osc = ctx.createOscillator();
    const g = ctx.createGain();
    osc.type = type;
    osc.frequency.setValueAtTime(f0, t);
    osc.frequency.exponentialRampToValueAtTime(f1, t + dur);
    g.gain.setValueAtTime(gain, t);
    g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
    osc.connect(g).connect(this.master);
    osc.start(t);
    osc.stop(t + dur + 0.02);
  }

  /** @param {number} dur @param {number} cutoff @param {number} gain */
  noise(dur, cutoff, gain) {
    const ctx = this.ctx;
    if (!ctx || !this.master) return;
    const n = Math.floor(ctx.sampleRate * dur);
    const buf = ctx.createBuffer(1, n, ctx.sampleRate);
    const data = buf.getChannelData(0);
    for (let i = 0; i < n; i++) data[i] = (Math.random() * 2 - 1) * (1 - i / n);
    const src = ctx.createBufferSource();
    src.buffer = buf;
    const f = ctx.createBiquadFilter();
    f.type = 'lowpass';
    f.frequency.value = cutoff;
    const g = ctx.createGain();
    g.gain.value = gain;
    src.connect(f).connect(g).connect(this.master);
    src.start();
  }

  jump() { this.tone(220, 520, 0.14, 'triangle', 0.25); }
  land() { this.noise(0.12, 500, 0.5); }
  step() { this.noise(0.05, 900, 0.12); }
  join() { this.tone(660, 990, 0.18, 'sine', 0.2); }
  score() { this.tone(523, 1046, 0.3, 'square', 0.12); }
}
