// Per-device Live player preferences (TV-PLY-003/004): the CRT filter and the tuning static (picture and sound).
import { create } from 'zustand';

export interface LivePrefs {
  /** Scanlines, slight curvature and vignette over the picture. */
  crt: boolean;
  /** A short noise burst between channels. */
  tuningStatic: boolean;
  /** Sound with the static. Off by default: a hiss nobody asked for is a bad surprise. */
  tuningSound: boolean;
}

const KEY = 'sr.cinema.tv.v1';
const DEFAULTS: LivePrefs = { crt: false, tuningStatic: true, tuningSound: false };

function load(): LivePrefs {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return { ...DEFAULTS, ...(JSON.parse(raw) as Partial<LivePrefs>) };
  } catch {
    /* storage unavailable */
  }
  return DEFAULTS;
}

export const useLivePrefs = create<LivePrefs>(() => load());

export function setLivePrefs(patch: Partial<LivePrefs>) {
  useLivePrefs.setState(patch);
  try {
    localStorage.setItem(KEY, JSON.stringify(useLivePrefs.getState()));
  } catch {
    /* best effort */
  }
}

/** A quarter second of filtered noise through Web Audio. Silent where AudioContext does not exist. */
export function playStaticBurst(ms = 280) {
  try {
    const Ctx =
      window.AudioContext ??
      (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctx) return;
    const ctx = new Ctx();
    const frames = Math.floor((ctx.sampleRate * ms) / 1000);
    const buf = ctx.createBuffer(1, frames, ctx.sampleRate);
    const data = buf.getChannelData(0);
    for (let i = 0; i < frames; i++) data[i] = (Math.random() * 2 - 1) * (1 - i / frames);
    const src = ctx.createBufferSource();
    const gain = ctx.createGain();
    gain.gain.value = 0.08;
    src.buffer = buf;
    src.connect(gain).connect(ctx.destination);
    src.onended = () => void ctx.close();
    src.start();
  } catch {
    /* no sound is fine */
  }
}
