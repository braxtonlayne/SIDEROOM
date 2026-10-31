// @ts-check
// Personal settings (D-55): how the game looks and sounds on this device. Kept in this browser only; nothing
// here changes the game for anyone else. Volume and mute live in client/audio.js, which already stores them.

const KEY = 'landio.prefs';

/**
 * @typedef {object} Prefs
 * @property {'system'|'on'|'off'} motion   reduced motion: follow the system setting, or force it
 * @property {boolean} shake                screen shake on hits and deaths
 * @property {boolean} names                name tags over heads
 * @property {boolean} minimap
 * @property {'auto'|'high'|'low'} quality  resolution: adapt to this machine, or fix it
 */

/** @type {Prefs} */
export const DEFAULT_PREFS = { motion: 'system', shake: true, names: true, minimap: true, quality: 'auto' };

/** The stored settings, with anything missing or broken back at its default. @returns {Prefs} */
export function loadPrefs() {
  /** @type {Partial<Prefs>} */
  let raw = {};
  try {
    raw = JSON.parse(localStorage.getItem(KEY) ?? '{}') ?? {};
  } catch {
    raw = {};
  }
  const p = { ...DEFAULT_PREFS };
  if (raw.motion === 'system' || raw.motion === 'on' || raw.motion === 'off') p.motion = raw.motion;
  if (typeof raw.shake === 'boolean') p.shake = raw.shake;
  if (typeof raw.names === 'boolean') p.names = raw.names;
  if (typeof raw.minimap === 'boolean') p.minimap = raw.minimap;
  if (raw.quality === 'auto' || raw.quality === 'high' || raw.quality === 'low') p.quality = raw.quality;
  return p;
}

/** @param {Prefs} p */
export function savePrefs(p) {
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    // Private windows and blocked storage: the settings last for this page.
  }
}
