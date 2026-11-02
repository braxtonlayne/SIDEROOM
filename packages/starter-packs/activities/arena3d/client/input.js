// @ts-check
// Keyboard and pointer-locked mouse into the simulation's integer input (sim/input.js). Mouse deltas are
// accumulated here as floats and only the rounded integer yaw/pitch ever reaches the simulation, so the same
// recorded input replays identically.
//
// Touch (client/touch.js) feeds the same accumulators: `addLook` is the mouse path with a scale, and the stick and
// buttons arrive as a bit mask (`setTouchButtons`) OR-ed into the keyboard's. So a phone player's input is the same
// integer yaw/pitch/bit set a keyboard player's is.
//
// Pointer lock: the sandbox allows it (`allow-pointer-lock`). Esc releases it (the browser does that); the game
// then pauses and shows "click to resume", and a click on the page asks for it again. Browsers refuse a new
// lock for about a second after Esc, so a refused request just leaves the prompt up for another click.

import { BTN, PITCH_LIMIT, idleInput, wrapYaw } from '../sim/input.js';

/** @type {Record<string, number>} */
const KEYS = {
  KeyW: BTN.FORWARD, ArrowUp: BTN.FORWARD,
  KeyS: BTN.BACK, ArrowDown: BTN.BACK,
  KeyA: BTN.LEFT, ArrowLeft: BTN.LEFT,
  KeyD: BTN.RIGHT, ArrowRight: BTN.RIGHT,
  Space: BTN.JUMP,
  // Not Ctrl: Ctrl+W (crouch while walking) closes the tab, and preventDefault cannot stop it.
  KeyC: BTN.CROUCH, ShiftLeft: BTN.CROUCH,
};

/** Yaw units per mouse pixel per sensitivity step (sensitivity 5 is about 0.15 degrees per pixel). */
const UNITS_PER_PX = 5.5;

export class InputState {
  /**
   * @param {Window} win @param {HTMLElement} target the element that takes pointer lock (the canvas)
   * @param {{ sensitivity: number, invertY: boolean }} settings read on every mouse move
   */
  constructor(win, target, settings) {
    this.win = win;
    this.target = target;
    this.settings = settings;
    this.buttons = 0;
    this.yaw = 0;
    this.pitch = 0;
    this.locked = false;
    /** Touch controls are shown and playing (no pointer lock on a phone). */
    this.touchOn = false;
    this.touchButtons = 0;
    /** @type {Set<(locked: boolean) => void>} */
    this.lockListeners = new Set();
    const doc = win.document;
    /** @param {KeyboardEvent} e */
    this.onKey = (e) => {
      const bit = KEYS[e.code];
      if (!bit || !this.locked) return;
      e.preventDefault();
      if (e.type === 'keydown') this.buttons |= bit;
      else this.buttons &= ~bit;
    };
    /** @param {MouseEvent} e */
    this.onMove = (e) => {
      if (this.locked) this.addLook(e.movementX, e.movementY);
    };
    this.onLockChange = () => {
      this.locked = doc.pointerLockElement === this.target;
      if (!this.locked) this.buttons = 0;
      this.lockListeners.forEach((cb) => cb(this.locked));
    };
    // Firefox and Safari report a refused lock with this event, not a rejected promise.
    this.onLockError = () => this.lockListeners.forEach((cb) => cb(false));
    this.onBlur = () => {
      this.buttons = 0;
    };
    win.addEventListener('keydown', this.onKey);
    win.addEventListener('keyup', this.onKey);
    win.addEventListener('blur', this.onBlur);
    doc.addEventListener('mousemove', this.onMove);
    doc.addEventListener('pointerlockchange', this.onLockChange);
    doc.addEventListener('pointerlockerror', this.onLockError);
  }

  /**
   * Turn the view by a pointer movement of `dx`, `dy` pixels (the mouse, or a finger dragging). The float
   * accumulators are rounded only in `sample()`.
   * @param {number} dx @param {number} dy @param {number} [scale] 1 for a mouse pixel
   */
  addLook(dx, dy, scale = 1) {
    const k = UNITS_PER_PX * this.settings.sensitivity * scale;
    this.yaw -= dx * k;
    this.pitch += (this.settings.invertY ? dy : -dy) * k;
    this.pitch = Math.max(-PITCH_LIMIT, Math.min(PITCH_LIMIT, this.pitch));
  }

  /** The touch stick and buttons as a BTN bit set (0 lets go of everything). @param {number} mask */
  setTouchButtons(mask) {
    this.touchButtons = mask & 63;
  }

  /** Touch controls on or off (on while the match is being played with them). @param {boolean} on */
  setTouch(on) {
    this.touchOn = on;
    if (!on) this.touchButtons = 0;
  }

  /** Ask for pointer lock (call from a click). Resolves false when the browser refuses (too soon after Esc). */
  async requestLock() {
    try {
      // Some browsers return a promise, older ones return undefined.
      await /** @type {any} */ (this.target).requestPointerLock?.();
      return true;
    } catch {
      return false;
    }
  }

  /** The element that takes pointer lock changed (the renderer swapped in a fresh canvas after a GPU loss). @param {HTMLElement} el */
  setTarget(el) {
    this.target = el;
    this.locked = this.win.document.pointerLockElement === el;
  }

  /** @param {(locked: boolean) => void} cb */
  onLock(cb) {
    this.lockListeners.add(cb);
    return () => this.lockListeners.delete(cb);
  }

  /** This tick's input. Unlocked (paused): no buttons, the view stays where it was. */
  sample() {
    const base = idleInput();
    base.yaw = wrapYaw(this.yaw);
    base.pitch = Math.round(this.pitch);
    base.buttons = (this.locked ? this.buttons : 0) | (this.touchOn ? this.touchButtons : 0);
    return base;
  }

  /** Set the view (e.g. to face the arena on spawn). @param {number} yaw @param {number} [pitch] */
  look(yaw, pitch = 0) {
    this.yaw = yaw;
    this.pitch = pitch;
  }

  destroy() {
    const doc = this.win.document;
    this.win.removeEventListener('keydown', this.onKey);
    this.win.removeEventListener('keyup', this.onKey);
    this.win.removeEventListener('blur', this.onBlur);
    doc.removeEventListener('mousemove', this.onMove);
    doc.removeEventListener('pointerlockchange', this.onLockChange);
    doc.removeEventListener('pointerlockerror', this.onLockError);
  }
}
