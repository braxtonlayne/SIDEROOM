// @ts-check
// Sound: a small mixer over Web Audio. The game calls only this API.
// Sounds are synthesized by the recipes in client/sfx.js; this file owns the graph, limits and levels.

import { RECIPES, BUS } from './sfx.js';

/**
 * @typedef {object} SfxOptions
 * @property {number} [size]    a magnitude 0..1 (capture size, blast size) the sound may scale with
 * @property {number} [dist]    tiles from the listener (own head); far sounds are quieter
 * @property {number} [pan]     -1 left .. 1 right
 * @property {number} [detune]  pitch variation in cents, set by play() (random in [-100, 100])
 */

const KEY_VOLUME = 'landio.volume';
const KEY_MUTED = 'landio.muted';
const DEFAULT_VOLUME = 0.7;
const MAX_VOICES = 14;
const REPEAT_GAP_MS = 45;
const MAX_DIST = 40;
const TAIL_S = 0.1;

/** @type {AudioContext | null} */
let ctx = null;
/** @type {GainNode | null} */
let master = null;
/** @type {Record<'sfx' | 'ui', GainNode> | null} */
let buses = null;

let voices = 0;
/** @type {Map<string, number>} */
const lastPlayedAt = new Map();

/** Reads a stored 0..1 volume; any missing or broken value falls back to the default. */
function readVolume() {
  try {
    const raw = localStorage.getItem(KEY_VOLUME);
    if (raw === null || raw === '') return DEFAULT_VOLUME;
    return clamp01(Number(raw), DEFAULT_VOLUME);
  } catch {
    return DEFAULT_VOLUME;
  }
}

function readMuted() {
  try {
    return localStorage.getItem(KEY_MUTED) === 'true';
  } catch {
    return false;
  }
}

/** @param {string} key @param {string} value */
function store(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Private windows and blocked storage: the setting just lasts for this page.
  }
}

/** @param {number} v @param {number} fallback */
function clamp01(v, fallback) {
  if (!Number.isFinite(v)) return fallback;
  return Math.min(1, Math.max(0, v));
}

let volumeLevel = readVolume();
let isMuted = readMuted();

/** The slider is perceptual, so the gain curve is squared. */
function applyMasterGain() {
  if (!ctx || !master) return;
  const target = isMuted ? 0 : volumeLevel * volumeLevel;
  // A short ramp avoids clicks when the slider moves or mute toggles mid-sound.
  master.gain.setTargetAtTime(target, ctx.currentTime, 0.015);
}

/** Builds the graph once: buses -> master -> safety limiter -> destination. */
function build(/** @type {AudioContext} */ c) {
  const limiter = c.createDynamicsCompressor();
  limiter.threshold.value = -6;
  limiter.knee.value = 0;
  limiter.ratio.value = 20;
  limiter.attack.value = 0.003;
  limiter.release.value = 0.15;
  limiter.connect(c.destination);

  const m = c.createGain();
  m.connect(limiter);

  const sfxBus = c.createGain();
  sfxBus.gain.value = 0.9;
  sfxBus.connect(m);

  const uiBus = c.createGain();
  uiBus.gain.value = 0.6;
  uiBus.connect(m);

  ctx = c;
  master = m;
  buses = { sfx: sfxBus, ui: uiBus };
  applyMasterGain();
}

/** Own-property check, so names like 'constructor' never match the plain recipe/bus tables. @param {object} obj @param {string} key */
function has(obj, key) {
  return Object.prototype.hasOwnProperty.call(obj, key);
}

/** @type {{ unlock(): void, play(name: string, opts?: SfxOptions): void, toggleMute(): boolean, setVolume(v: number): void, readonly volume: number, readonly muted: boolean }} */
export const sfx = {
  /** Create/resume the AudioContext; call from a user gesture (browsers require one). */
  unlock() {
    try {
      if (!ctx) {
        const Ctor = /** @type {any} */ (globalThis).AudioContext || /** @type {any} */ (globalThis).webkitAudioContext;
        if (!Ctor) return;
        build(new Ctor());
      }
      if (ctx && ctx.state === 'suspended') {
        ctx.resume().catch(() => {});
      }
    } catch {
      // Audio blocked or unavailable: the game runs silently.
    }
  },

  /** Play a named sound (names in client/sfx.js). @param {string} name @param {SfxOptions} [opts] */
  play(name, opts = {}) {
    try {
      if (!ctx || !buses) return;
      if (!has(RECIPES, name)) return;
      const makeSound = RECIPES[name];

      const dist = opts.dist;
      if (dist !== undefined && dist > MAX_DIST) return;
      if (voices >= MAX_VOICES) return;

      const now = performance.now();
      const last = lastPlayedAt.get(name);
      if (last !== undefined && now - last < REPEAT_GAP_MS) return;
      lastPlayedAt.set(name, now);

      const c = ctx;
      const busName = has(BUS, name) ? BUS[name] : 'sfx';
      const bus = buses[busName];

      const distGain = dist === undefined ? 1 : Math.max(0.15, 1 - dist / 30);
      // +-1.5 dB per play so repeated sounds do not sound identical.
      const jitterDb = Math.random() * 3 - 1.5;
      const jitter = Math.pow(10, jitterDb / 20);

      const voiceGain = c.createGain();
      voiceGain.gain.value = distGain * jitter;

      /** @type {StereoPannerNode | null} */
      let panner = null;
      if (opts.pan !== undefined) {
        panner = c.createStereoPanner();
        panner.pan.value = Math.min(1, Math.max(-1, opts.pan));
        voiceGain.connect(panner);
        panner.connect(bus);
      } else {
        voiceGain.connect(bus);
      }

      /** @type {SfxOptions} */
      const recipeOpts = { ...opts, detune: Math.random() * 200 - 100 };

      voices++;
      let length = 0;
      try {
        length = makeSound(c, voiceGain, c.currentTime, recipeOpts);
      } catch {
        voices--;
        voiceGain.disconnect();
        panner?.disconnect();
        return;
      }

      setTimeout(() => {
        voices--;
        try {
          voiceGain.disconnect();
          panner?.disconnect();
        } catch {
          // Already torn down with the context.
        }
      }, (length + TAIL_S) * 1000);
    } catch {
      // A failed sound must never break gameplay.
    }
  },

  /** Toggle mute; returns the new muted state. */
  toggleMute() {
    isMuted = !isMuted;
    store(KEY_MUTED, String(isMuted));
    applyMasterGain();
    return isMuted;
  },

  /** @param {number} v 0..1 */
  setVolume(v) {
    volumeLevel = clamp01(v, volumeLevel);
    store(KEY_VOLUME, String(volumeLevel));
    applyMasterGain();
  },

  get volume() {
    return volumeLevel;
  },
  get muted() {
    return isMuted;
  },
};
