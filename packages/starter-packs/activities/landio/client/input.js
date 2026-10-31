// @ts-check
// Input: physical keys, touch and gamepad map to two actions, `turn(dir)` and `stop` (toggle). Gameplay sees
// only actions. Keys bind by physical position (`KeyboardEvent.code`), so WASD stays put on AZERTY. Players may
// rebind the keys (two per action, D-56); the bindings are kept in this browser.
// Buffering of turns lives in the simulation (one turn per tile, two deep), so input just reports presses.

/** @typedef {{ turn: (dir: number) => void, stop: () => void, card: (code: number) => void }} Actions */

import { ACT } from '../shared/cards.js';

/**
 * The bindable actions: a direction, stop/go, or a card action (an ACT code).
 * @type {{id: string, label: string, turn?: number, stop?: true, card?: number}[]}
 */
export const ACTIONS = [
  { id: 'up', label: 'Turn up', turn: 0 },
  { id: 'right', label: 'Turn right', turn: 1 },
  { id: 'down', label: 'Turn down', turn: 2 },
  { id: 'left', label: 'Turn left', turn: 3 },
  { id: 'stop', label: 'Stop or go', stop: true },
  { id: 'item0', label: 'Use item 1', card: ACT.item0 },
  { id: 'item1', label: 'Use item 2', card: ACT.item1 },
  { id: 'ability', label: 'Use your ability', card: ACT.active },
  { id: 'pick1', label: 'Pick card 1', card: ACT.choose0 },
  { id: 'pick2', label: 'Pick card 2', card: ACT.choose0 - 1 },
  { id: 'pick3', label: 'Pick card 3', card: ACT.choose0 - 2 },
  { id: 'pick4', label: 'Pick card 4', card: ACT.choose0 - 3 },
];

/**
 * Two keys per action ('' = none). F as well as Shift for the ability: holding or tapping Shift repeatedly can
 * trigger Windows Sticky/Filter Keys. (Right Shift always counts as Left Shift.)
 * @type {Record<string, [string, string]>}
 */
export const DEFAULT_BINDINGS = {
  up: ['KeyW', 'ArrowUp'],
  right: ['KeyD', 'ArrowRight'],
  down: ['KeyS', 'ArrowDown'],
  left: ['KeyA', 'ArrowLeft'],
  stop: ['Space', ''],
  item0: ['KeyQ', ''],
  item1: ['KeyE', ''],
  ability: ['ShiftLeft', 'KeyF'],
  pick1: ['Digit1', 'Numpad1'],
  pick2: ['Digit2', 'Numpad2'],
  pick3: ['Digit3', 'Numpad3'],
  pick4: ['Digit4', 'Numpad4'],
};

/** Keys the game keeps for itself: the menu, mute, play again. */
export const RESERVED_KEYS = new Set(['Escape', 'KeyM', 'Enter', 'NumpadEnter', 'Tab']);

const BINDINGS_KEY = 'landio.keys';

/** Card keys and movement keys, derived from the bindings (rebuilt by applyBindings). Values are ACT codes. @type {Record<string, number>} */
export const CARD_KEYS = {};
/** @type {Record<string, number | 'stop'>} */
export const KEYMAP = {};

/** @param {string} code */
const norm = (code) => (code === 'ShiftRight' ? 'ShiftLeft' : code);

/** The current bindings. @type {Record<string, [string, string]>} */
export let bindings = structuredClone(DEFAULT_BINDINGS);

/** Make `b` the bindings: rebuild the key tables and keep them in this browser. @param {Record<string, [string, string]>} b @param {boolean} [save] */
export function applyBindings(b, save = true) {
  bindings = b;
  for (const k of Object.keys(CARD_KEYS)) delete CARD_KEYS[k];
  for (const k of Object.keys(KEYMAP)) delete KEYMAP[k];
  for (const a of ACTIONS)
    for (const code of b[a.id] ?? []) {
      if (!code) continue;
      if (a.card !== undefined) CARD_KEYS[code] = a.card;
      else KEYMAP[code] = a.stop ? 'stop' : /** @type {number} */ (a.turn);
    }
  if (save)
    try {
      localStorage.setItem(BINDINGS_KEY, JSON.stringify(b));
    } catch {
      // Blocked storage: the bindings last for this page.
    }
}

/** The stored bindings (anything missing or broken: the default), applied. */
export function loadBindings() {
  /** @type {Record<string, [string, string]>} */
  const b = structuredClone(DEFAULT_BINDINGS);
  try {
    const raw = JSON.parse(localStorage.getItem(BINDINGS_KEY) ?? 'null');
    if (raw && typeof raw === 'object')
      for (const a of ACTIONS) {
        const v = raw[a.id];
        if (Array.isArray(v) && v.length === 2 && v.every((c) => typeof c === 'string' && !RESERVED_KEYS.has(c))) b[a.id] = [v[0], v[1]];
      }
  } catch {
    // Unreadable: the defaults.
  }
  applyBindings(b, false);
  return b;
}

/**
 * Bind `code` to slot `slot` of action `id` ('' clears the slot). The key leaves any other action that had it.
 * Returns the label of the action that lost it, or ''. @param {string} id @param {0 | 1} slot @param {string} code
 */
export function rebind(id, slot, code) {
  const b = structuredClone(bindings);
  let moved = '';
  code = norm(code);
  if (code)
    for (const a of ACTIONS)
      for (const s of /** @type {(0 | 1)[]} */ ([0, 1]))
        if (b[a.id][s] === code && !(a.id === id && s === slot)) {
          b[a.id][s] = '';
          if (a.id !== id) moved = a.label;
        }
  b[id][slot] = code;
  applyBindings(b);
  return moved;
}

/** A short name for a key code: W, ↑, 1, Num 1, Shift, Space. @param {string} code */
export function keyLabel(code) {
  if (!code) return '';
  const arrows = { ArrowUp: '↑', ArrowRight: '→', ArrowDown: '↓', ArrowLeft: '←' };
  if (code in arrows) return arrows[/** @type {keyof typeof arrows} */ (code)];
  if (code.startsWith('Key')) return code.slice(3);
  if (code.startsWith('Digit')) return code.slice(5);
  if (code.startsWith('Numpad')) return `Num ${code.slice(6)}`;
  if (code.startsWith('Shift')) return 'Shift';
  if (code.startsWith('Control')) return 'Ctrl';
  if (code.startsWith('Alt')) return 'Alt';
  return code.replace(/([a-z])([A-Z])/g, '$1 $2');
}

/** The keys of an action, for hints: "W / ↑". @param {string} id */
export const keysOf = (id) => (bindings[id] ?? []).filter(Boolean).map(keyLabel).join(' / ');

/** The first key of an action ('—' when it has none). @param {string} id */
export const firstKey = (id) => keyLabel((bindings[id] ?? []).find(Boolean) ?? '') || '—';

/** The one-line key hint at the bottom of the screen, from the bindings. */
export function hintText() {
  const turns = ['up', 'left', 'down', 'right'].map(firstKey);
  const second = ['up', 'left', 'down', 'right'].map((id) => keyLabel(bindings[id]?.[1] ?? ''));
  let move = turns.every((k) => k.length === 1) ? turns.join('') : turns.join(' ');
  if (second.join('') === '↑←↓→') move += ' / arrows';
  return `${move} to turn · ${firstKey('stop')} to stop · ${firstKey('item0')} / ${firstKey('item1')} items · ${firstKey('ability')} ability`;
}

loadBindings();

const SWIPE_PX = 24;
const TAP_MS = 250;

/**
 * Listen for input on `surface` (the canvas) and `win`. Returns a function that removes the listeners and a
 * `pollGamepad()` to call once per frame.
 * @param {HTMLElement} surface @param {Window} win @param {Actions} act
 */
export function bindInput(surface, win, act) {
  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.repeat || e.ctrlKey || e.metaKey || e.altKey) return;
    const target = /** @type {HTMLElement|null} */ (e.target);
    // Typing in a text field (a room code, a name) is typing: no game key may eat the letter.
    if (target && (target.tagName === 'TEXTAREA' || (target.tagName === 'INPUT' && /^(text|search|email|url|)$/.test(/** @type {HTMLInputElement} */ (target).type)) || target.isContentEditable)) return;
    if (target && (target.tagName === 'INPUT' || target.tagName === 'BUTTON' || target.tagName === 'SELECT') && e.code === 'Space') return;
    const code = norm(e.code);
    const c = CARD_KEYS[code];
    if (c !== undefined) {
      e.preventDefault();
      act.card(c);
      return;
    }
    const a = KEYMAP[code];
    if (a === undefined) return;
    e.preventDefault();
    if (a === 'stop') act.stop();
    else act.turn(a);
  };

  // Touch: a swipe turns as soon as it passes the threshold (no need to lift the finger); a short touch that
  // never became a swipe is a tap: stop/go.
  /** @type {{x: number, y: number, t: number, used: boolean} | null} */
  let touch = null;
  /** @param {PointerEvent} e */
  const onDown = (e) => {
    if (e.pointerType === 'mouse') return;
    touch = { x: e.clientX, y: e.clientY, t: e.timeStamp, used: false };
    surface.setPointerCapture?.(e.pointerId);
  };
  /** @param {PointerEvent} e */
  const onMove = (e) => {
    if (!touch) return;
    const dx = e.clientX - touch.x;
    const dy = e.clientY - touch.y;
    if (Math.max(Math.abs(dx), Math.abs(dy)) < SWIPE_PX) return;
    act.turn(Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? 1 : 3) : dy > 0 ? 2 : 0);
    // Keep tracking from here, so one continuous gesture can chain turns (up, then right).
    touch = { x: e.clientX, y: e.clientY, t: touch.t, used: true };
  };
  /** @param {PointerEvent} e */
  const onUp = (e) => {
    if (touch && !touch.used && e.timeStamp - touch.t < TAP_MS) act.stop();
    touch = null;
  };

  win.addEventListener('keydown', onKey);
  surface.addEventListener('pointerdown', onDown);
  surface.addEventListener('pointermove', onMove);
  surface.addEventListener('pointerup', onUp);
  surface.addEventListener('pointercancel', () => (touch = null));

  // Gamepad: d-pad or left stick past a radial deadzone; A / start toggles stop. Edge-detected.
  let lastDir = -1;
  let lastStop = false;
  /** @type {Record<number, boolean>} */
  const lastBtn = {};
  function pollGamepad() {
    const pads = win.navigator.getGamepads ? win.navigator.getGamepads() : [];
    for (const pad of pads) {
      if (!pad) continue;
      const b = (/** @type {number} */ i) => !!pad.buttons[i]?.pressed;
      let dir = b(12) ? 0 : b(15) ? 1 : b(13) ? 2 : b(14) ? 3 : -1;
      const x = pad.axes[0] ?? 0;
      const y = pad.axes[1] ?? 0;
      if (dir < 0 && Math.hypot(x, y) > 0.5) dir = Math.abs(x) > Math.abs(y) ? (x > 0 ? 1 : 3) : y > 0 ? 2 : 0;
      if (dir >= 0 && dir !== lastDir) act.turn(dir);
      lastDir = dir;
      const stop = b(0) || b(9);
      if (stop && !lastStop) act.stop();
      lastStop = stop;
      // X / Y use the items, the right bumper the ability.
      for (const [btn, code] of /** @type {[number, number][]} */ ([[2, ACT.item0], [3, ACT.item1], [5, ACT.active]])) {
        const down = b(btn);
        if (down && !lastBtn[btn]) act.card(code);
        lastBtn[btn] = down;
      }
      return;
    }
  }

  return {
    pollGamepad,
    unbind() {
      win.removeEventListener('keydown', onKey);
      surface.removeEventListener('pointerdown', onDown);
      surface.removeEventListener('pointermove', onMove);
      surface.removeEventListener('pointerup', onUp);
    },
  };
}
