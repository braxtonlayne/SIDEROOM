// @ts-check
// Boot: the menu is plain HTML, so it paints before any of this runs. Then the platform handshake (SIDE ROOM or
// the local stand-in), settings from storage, and, lazily in the background, Jolt (~3 MB) and the renderer.
// "Play" starts the match and takes pointer lock; Esc pauses; a click resumes.

import { getPlatform } from '../platform/platform.js';
import { loadJolt } from '../sim/jolt-loader.js';
import { createRenderer } from '../render/renderer.js';
import { BundledAssetSource } from '../assets/source.js';
import { loadGltf } from '../assets/gltf.js';
import { loadSettings, saveSettings, normaliseSettings } from './settings.js';
import { InputState } from './input.js';
import { mountTouch } from './touch.js';
import { Hud } from './hud.js';
import { Sfx } from './sfx.js';
import { Game } from './game.js';
import { spatialVoiceFor } from './spatial-voice.js';

const doc = document;
/** @param {string} id */
const $ = (id) => /** @type {HTMLElement} */ (doc.getElementById(id));
/** @param {string} id */
const input = (id) => /** @type {HTMLInputElement} */ (doc.getElementById(id));
const canvas = /** @type {HTMLCanvasElement} */ ($('view'));
const status = $('status');
const play = /** @type {HTMLButtonElement} */ ($('btn-play'));

/** @param {string} which */
function show(which) {
  for (const id of ['screen-main', 'screen-settings', 'screen-howto']) $(id).hidden = id !== which;
}

async function boot() {
  const platform = getPlatform(window);
  status.textContent = platform.kind === 'sideroom' ? 'Connecting to SIDE ROOM…' : 'Looking for other tabs…';
  // Physics loads while the handshake runs; the menu is already on screen.
  // (setTimeout, not requestAnimationFrame: rAF never fires in a background tab.)
  const jolt = new Promise((r) => setTimeout(r, 0)).then(() => loadJolt());
  await platform.ready();
  let settings = loadSettings(platform.storage);
  const sfx = new Sfx();
  sfx.setVolume(settings.volume);
  const keys = new InputState(window, canvas, settings);
  const hud = new Hud(doc);
  // Phones and tablets: an on-screen stick, a drag-to-look surface and jump/crouch buttons (client/touch.js).
  const touch = mountTouch(doc, window, keys, { onMenu: () => toMenu() });
  /** @type {Game | null} */
  let game = null;

  // Settings screen: every change is saved at once.
  const bind = () => {
    input('set-name').value = settings.name;
    input('set-sens').value = String(settings.sensitivity);
    input('set-invert').checked = settings.invertY;
    input('set-fov').value = String(settings.fov);
    input('set-volume').value = String(settings.volume);
    /** @type {HTMLSelectElement} */ ($('set-renderer')).value = settings.renderer;
    input('set-net').checked = settings.showNet;
    if (platform.kind !== 'sideroom') $('settings-note').textContent = 'Saved in this browser. Renderer changes apply next time the game opens.';
  };
  const onChange = () => {
    settings = normaliseSettings({
      name: input('set-name').value.trim(),
      sensitivity: Number(input('set-sens').value),
      invertY: input('set-invert').checked,
      fov: Number(input('set-fov').value),
      volume: Number(input('set-volume').value),
      renderer: /** @type {HTMLSelectElement} */ ($('set-renderer')).value,
      showNet: input('set-net').checked,
    });
    Object.assign(keys.settings, settings);
    sfx.setVolume(settings.volume);
    if (game) Object.assign(game.opt.settings, settings);
    if (!saveSettings(platform.storage, settings)) $('settings-note').textContent = 'Could not save (storage is full or not available); kept for this session.';
  };
  for (const id of ['set-name', 'set-sens', 'set-invert', 'set-fov', 'set-volume', 'set-renderer', 'set-net']) $(id).addEventListener('change', onChange);
  bind();
  $('btn-settings').addEventListener('click', () => show('screen-settings'));
  $('btn-howto').addEventListener('click', () => show('screen-howto'));
  $('btn-settings-back').addEventListener('click', () => show('screen-main'));
  $('btn-howto-back').addEventListener('click', () => show('screen-main'));

  status.textContent = 'Loading physics…';
  const J = await jolt;
  status.textContent = 'Starting the renderer…';
  // Losing the GPU (a driver reset, a crashed GPU process) is recovered by the renderer itself: it rebuilds, or falls
  // back from WebGPU to WebGL2 on a fresh canvas element, and the notice below says what happened.
  const notice = $('gpu-notice');
  let noticeTimer = 0;
  /** @param {string} text @param {number} [forMs] 0: until replaced */
  const say = (text, forMs = 0) => {
    clearTimeout(noticeTimer);
    notice.textContent = text;
    notice.hidden = !text;
    if (text && forMs) noticeTimer = window.setTimeout(() => say(''), forMs);
  };
  const renderer = await createRenderer(canvas, {
    prefer: settings.renderer,
    onLost: () => say('Graphics were reset. Restoring…'),
    onRecovered: (kind, was) => say(kind === was ? 'Graphics restored.' : 'Graphics restored on WebGL2 (WebGPU was lost).', 4000),
    onFailed: (why) => {
      console.error('GPU recovery failed:', why);
      say('The GPU was lost and could not be restored. Reload to continue.');
      status.textContent = 'The GPU was lost. Reload to continue.';
    },
    replaceCanvas: () => {
      // A canvas that has had a 'webgpu' context cannot give a 'webgl2' one: swap in a new element, same id and place.
      const old = renderer?.canvas ?? canvas;
      const next = doc.createElement('canvas');
      next.id = old.id;
      next.className = old.className;
      const label = old.getAttribute('aria-label');
      if (label) next.setAttribute('aria-label', label);
      old.replaceWith(next);
      return next;
    },
  });

  const ch = platform.channel;
  status.textContent = `${renderer.kind === 'webgpu' ? 'WebGPU' : 'WebGL2'} · ${ch?.isHost ? 'you are hosting' : 'joining the host'}${platform.guest ? ' · guest' : ''}`;
  play.disabled = false;
  play.textContent = 'Play';

  const menu = $('menu');
  const resume = $('resume');
  const hudEl = $('hud');
  let playing = false;
  // The match starts now, behind the menu: a host must answer peers' HELLOs even before its own player
  // presses Play (its player just stands still until then).
  game = new Game({ J, platform, renderer, canvas, input: keys, settings, hud, sfx, voice: spatialVoiceFor([]) });
  game.start();
  // The game's own crash-report half (brief §3, wave 45): the state hash and the input log, for devtools,
  // a crash reporter or a bug replay (the log uses replay/script.js's event format).
  const g = game;
  Object.defineProperty(window, 'arena3d', {
    configurable: true,
    value: Object.freeze({
      tick: () => g.world.tick,
      stateHash: () => g.world.stateHash(),
      inputLog: () => (g.host ?? g.client)?.inputLog.snapshot() ?? [],
      role: () => (g.host ? 'host' : 'peer'),
    }),
  });
  // The sample glTF model (optional: it needs connect-src 'self', see assets/source.js).
  new BundledAssetSource()
    .json('models/beacon.json')
    .then((json) => loadGltf(json))
    .then((model) => {
      const pad = game?.world.level.pad;
      if (game && pad) game.scene.setBeacon(model, [(pad.min[0] + pad.max[0]) / 2, 0.02, (pad.min[1] + pad.max[1]) / 2]);
    })
    .catch((e) => console.warn('beacon model not loaded:', e));
  play.addEventListener('click', async () => {
    sfx.unlock();
    playing = true;
    menu.hidden = true;
    hudEl.hidden = false;
    if (touch.enabled) {
      // No pointer lock on a phone: the controls are on screen and there is nothing to pause.
      keys.setTouch(true);
      touch.setActive(true);
      resume.hidden = true;
    } else if (!(await keys.requestLock())) resume.hidden = false;
    platform.setActivity('In the arena');
  });
  keys.onLock((locked) => {
    if (!playing || keys.touchOn) return;
    resume.hidden = locked;
  });
  // A click anywhere on the paused screen (or the canvas) takes the mouse again.
  const relock = async (/** @type {Event} */ e) => {
    if (!playing || keys.touchOn || keys.locked || (e.target instanceof HTMLElement && e.target.id === 'btn-menu')) return;
    await keys.requestLock();
  };
  resume.addEventListener('click', relock);
  canvas.addEventListener('click', relock);
  // After a GPU recovery on a fresh canvas element: it takes the pointer lock and the clicks now.
  renderer.onCanvasChange?.((next) => {
    keys.setTarget(next);
    next.addEventListener('click', relock);
  });
  function toMenu() {
    playing = false;
    keys.setTouch(false);
    touch.setActive(false);
    resume.hidden = true;
    hudEl.hidden = true;
    menu.hidden = false;
    show('screen-main');
    play.textContent = 'Back to the arena';
  }
  $('btn-menu').addEventListener('click', toMenu);
}

boot().catch((e) => {
  console.error(e);
  status.textContent = `Could not start: ${e instanceof Error ? e.message : String(e)}`;
  play.textContent = 'Unavailable';
});
