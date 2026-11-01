// @ts-check
// Player settings, saved through the platform's key-value store: in SIDE ROOM that is the SDK's `storage`
// capability (the host keeps it per member; the sandbox has no localStorage), elsewhere the tab's localStorage.

const KEY = 'arena3d.settings';

/**
 * @typedef {object} Settings
 * @property {string} name          blank: the platform's name
 * @property {number} sensitivity   1..20
 * @property {boolean} invertY
 * @property {number} fov           degrees, 60..100
 * @property {number} volume        0..1
 * @property {'auto' | 'webgl2'} renderer
 * @property {boolean} showNet      the network line in the HUD
 */

/** @type {Readonly<Settings>} */
export const DEFAULTS = Object.freeze({ name: '', sensitivity: 5, invertY: false, fov: 75, volume: 0.6, renderer: 'auto', showNet: true });

/** @param {number} v @param {number} lo @param {number} hi @param {number} d */
const clamp = (v, lo, hi, d) => (Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : d);

/** Any stored or typed value as valid settings. @param {any} raw @returns {Settings} */
export function normaliseSettings(raw) {
  const r = raw && typeof raw === 'object' ? raw : {};
  return {
    name: String(r.name ?? '').replace(/[\u0000-\u001f]/g, '').slice(0, 24),
    sensitivity: clamp(Number(r.sensitivity), 1, 20, DEFAULTS.sensitivity),
    invertY: r.invertY === true,
    fov: clamp(Number(r.fov), 60, 100, DEFAULTS.fov),
    volume: clamp(Number(r.volume), 0, 1, DEFAULTS.volume),
    renderer: r.renderer === 'webgl2' ? 'webgl2' : 'auto',
    showNet: r.showNet !== false,
  };
}

/** @param {import('../platform/platform.js').KeyValue} store @returns {Settings} */
export function loadSettings(store) {
  try {
    return normaliseSettings(JSON.parse(store.getItem(KEY) ?? '{}'));
  } catch {
    return { ...DEFAULTS };
  }
}

/** @param {import('../platform/platform.js').KeyValue} store @param {Settings} s */
export function saveSettings(store, s) {
  try {
    store.setItem(KEY, JSON.stringify(normaliseSettings(s)));
    if (s.name) store.setItem('arena3d.name', s.name);
    return true;
  } catch {
    return false; // quota or no storage capability: settings stay for this session only
  }
}
