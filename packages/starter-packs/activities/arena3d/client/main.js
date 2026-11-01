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
  const jolt = new Promise((r) => requestAnimationFrame(() => setTimeout(r, 0))).then(() => loadJolt());
  await platform.ready();
  let settings = loadSettings(platform.storage);
  const sfx = new Sfx();
  sfx.setVolume(settings.volume);
  const keys = new InputState(window, canvas, settings);
  const hud = new Hud(doc);
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
  const renderer = await createRenderer(canvas, { prefer: settings.renderer, onLost: (why) => (status.textContent = `The GPU was lost (${why}). Reload to continue.`) });

  const ch = platform.channel;
  status.textContent = `${renderer.kind === 'webgpu' ? 'WebGPU' : 'WebGL2'} · ${ch?.isHost ? 'you are hosting' : 'joining the host'}${platform.guest ? ' · guest' : ''}`;
  play.disabled = false;
  play.textContent = 'Play';

  const menu = $('menu');
  const resume = $('resume');
  const hudEl = $('hud');
  let playing = false;
  play.addEventListener('click', async () => {
    sfx.unlock();
    if (!game) {
      game = new Game({ J, platform, renderer, canvas, input: keys, settings, hud, sfx, voice: spatialVoiceFor([]) });
      game.start();
      // The sample glTF model (optional: it needs connect-src 'self', see assets/source.js).
      new BundledAssetSource()
        .json('models/beacon.json')
        .then((json) => loadGltf(json))
        .then((model) => {
          const pad = game?.world.level.pad;
          if (game && pad) game.scene.setBeacon(model, [(pad.min[0] + pad.max[0]) / 2, 0.02, (pad.min[1] + pad.max[1]) / 2]);
        })
        .catch((e) => console.warn('beacon model not loaded:', e));
    }
    playing = true;
    menu.hidden = true;
    hudEl.hidden = false;
    if (!(await keys.requestLock())) resume.hidden = false;
    platform.setActivity('In the arena');
  });
  keys.onLock((locked) => {
    if (!playing) return;
    resume.hidden = locked;
  });
  // A click anywhere on the paused screen (or the canvas) takes the mouse again.
  const relock = async (/** @type {Event} */ e) => {
    if (!playing || keys.locked || (e.target instanceof HTMLElement && e.target.id === 'btn-menu')) return;
    await keys.requestLock();
  };
  resume.addEventListener('click', relock);
  canvas.addEventListener('click', relock);
  $('btn-menu').addEventListener('click', () => {
    playing = false;
    resume.hidden = true;
    hudEl.hidden = true;
    menu.hidden = false;
    show('screen-main');
    play.textContent = 'Back to the arena';
  });
}

boot().catch((e) => {
  console.error(e);
  status.textContent = `Could not start: ${e instanceof Error ? e.message : String(e)}`;
  play.textContent = 'Unavailable';
});
