// @ts-check
// Touch controls for phones and tablets: a move stick (left thumb), look by dragging anywhere else (right thumb),
// a jump button and a crouch toggle. They feed the same integer input path as the keyboard and mouse
// (client/input.js): the stick becomes the FORWARD/BACK/LEFT/RIGHT bits, the buttons the JUMP/CROUCH bits, and
// the drag becomes the same accumulated yaw/pitch as mouse movement. Nothing here reaches sim/ directly, so a
// touch player's inputs are indistinguishable on the wire and in a replay log from a keyboard player's.
//
// Two layers: `TouchController` is the logic (pointer positions in, bits and look out; no DOM, tested in Node)
// and `mountTouch` is the thin DOM glue (pointer events on the elements declared in index.html).

import { BTN } from '../sim/input.js';

/** Stick travel, in CSS pixels, that counts as full deflection. */
export const STICK_RADIUS = 56;
/** Under this fraction of full deflection the stick is centred (no buttons). */
export const STICK_DEAD = 0.3;
/** Touch look turns this many times faster than a mouse pixel at the same sensitivity (a finger drags less far). */
export const TOUCH_LOOK_SCALE = 1.8;
/** sin(22.5 degrees): the stick reads as 8 directions, 45 degrees each, like WASD. */
const SECTOR = 0.3827;

/**
 * The movement bits for a stick pushed `dx`, `dy` CSS pixels from where the thumb went down (y grows downwards).
 * @param {number} dx @param {number} dy
 */
export function stickButtons(dx, dy) {
  const len = Math.hypot(dx, dy);
  if (len / STICK_RADIUS < STICK_DEAD) return 0;
  const nx = dx / len;
  const ny = dy / len;
  let b = 0;
  if (ny < -SECTOR) b |= BTN.FORWARD;
  if (ny > SECTOR) b |= BTN.BACK;
  if (nx < -SECTOR) b |= BTN.LEFT;
  if (nx > SECTOR) b |= BTN.RIGHT;
  return b;
}

/** @typedef {{ addLook(dx: number, dy: number, scale?: number): void, setTouchButtons(mask: number): void }} LookSink */

export class TouchController {
  /** @param {LookSink} sink where look and buttons go (client/input.js InputState) */
  constructor(sink) {
    this.sink = sink;
    /** @type {{ id: number, ox: number, oy: number, x: number, y: number } | null} */
    this.stick = null;
    /** @type {{ id: number, x: number, y: number } | null} */
    this.look = null;
    /** @type {Map<number, number>} pointer id -> the button bit it holds down */
    this.held = new Map();
    this.crouchOn = false;
    this.mask = 0;
  }

  /** Recompute the held bits and hand them to the input. */
  push() {
    let m = this.stick ? stickButtons(this.stick.x - this.stick.ox, this.stick.y - this.stick.oy) : 0;
    for (const bit of this.held.values()) m |= bit;
    if (this.crouchOn) m |= BTN.CROUCH;
    this.mask = m;
    this.sink.setTouchButtons(m);
  }

  /** @param {number} id @param {number} x @param {number} y */
  stickDown(id, x, y) {
    if (this.stick) return false;
    this.stick = { id, ox: x, oy: y, x, y };
    this.push();
    return true;
  }

  /** @param {number} id @param {number} x @param {number} y */
  stickMove(id, x, y) {
    if (this.stick?.id !== id) return;
    // The knob stops at full deflection; the thumb can slide on without the stick running away from it.
    const dx = x - this.stick.ox;
    const dy = y - this.stick.oy;
    const len = Math.hypot(dx, dy);
    const k = len > STICK_RADIUS ? STICK_RADIUS / len : 1;
    this.stick.x = this.stick.ox + dx * k;
    this.stick.y = this.stick.oy + dy * k;
    this.push();
  }

  /** @param {number} id @param {number} x @param {number} y */
  lookDown(id, x, y) {
    if (this.look) return false;
    this.look = { id, x, y };
    return true;
  }

  /** @param {number} id @param {number} x @param {number} y */
  lookMove(id, x, y) {
    if (this.look?.id !== id) return;
    this.sink.addLook(x - this.look.x, y - this.look.y, TOUCH_LOOK_SCALE);
    this.look.x = x;
    this.look.y = y;
  }

  /** A held button (jump). @param {number} id @param {number} bit */
  buttonDown(id, bit) {
    this.held.set(id, bit);
    this.push();
  }

  /** Crouch is a toggle: one tap squats, the next stands (a thumb is busy looking). Returns the new state. */
  toggleCrouch() {
    this.crouchOn = !this.crouchOn;
    this.push();
    return this.crouchOn;
  }

  /** Any pointer lifted or cancelled. @param {number} id */
  up(id) {
    if (this.stick?.id === id) this.stick = null;
    if (this.look?.id === id) this.look = null;
    this.held.delete(id);
    this.push();
  }

  /** Let go of everything (the controls hid, the page lost focus). */
  releaseAll() {
    this.stick = null;
    this.look = null;
    this.held.clear();
    this.crouchOn = false;
    this.push();
  }
}

/**
 * Whether to show the controls: the primary pointer is a finger, or `?touch=1` asks for them. Devices with both a
 * mouse and a screen get them on the first real touch instead (see mountTouch).
 * @param {Window} win
 */
export function wantsTouch(win) {
  try {
    if (new URLSearchParams(win.location.search).get('touch') === '1') return true;
    return !!win.matchMedia?.('(pointer: coarse)').matches;
  } catch {
    return false;
  }
}

/**
 * Wire the elements of index.html (`#touch` and the ones in it) to a TouchController. `setActive(true)` shows the
 * controls (when this is a touch device) and `false` hides them and lets go of every finger.
 * @param {Document} doc @param {Window} win @param {LookSink} sink
 * @param {{ onMenu: () => void }} opt
 */
export function mountTouch(doc, win, sink, opt) {
  /** @param {string} id */
  const $ = (id) => /** @type {HTMLElement} */ (doc.getElementById(id));
  const root = $('touch');
  const zone = $('touch-stick-zone');
  const base = $('touch-base');
  const knob = $('touch-knob');
  const jump = $('touch-jump');
  const crouch = $('touch-crouch');
  const menu = $('touch-menu');
  const ctl = new TouchController(sink);
  let enabled = wantsTouch(win);
  let active = false;

  const render = () => {
    root.hidden = !(enabled && active);
    doc.body.classList.toggle('touch', enabled);
    const s = ctl.stick;
    base.hidden = !s;
    if (s) {
      const r = zone.getBoundingClientRect();
      // Physical pixels from the pointer events; the page is dir=ltr, so the base's logical start is its left.
      base.style.transform = `translate(${s.ox - r.left - 56}px, ${s.oy - r.top - 56}px)`;
      knob.style.transform = `translate(${s.x - s.ox}px, ${s.y - s.oy}px)`;
    }
    crouch.setAttribute('aria-pressed', String(ctl.crouchOn));
    crouch.classList.toggle('on', ctl.crouchOn);
    jump.classList.toggle('on', ctl.held.size > 0);
  };

  /** @param {PointerEvent} e */
  const grab = (e) => {
    e.preventDefault();
    try {
      /** @type {HTMLElement} */ (e.currentTarget).setPointerCapture(e.pointerId);
    } catch {
      /* the pointer is already gone */
    }
  };
  /** @param {HTMLElement} el @param {(e: PointerEvent) => void} down @param {(e: PointerEvent) => void} [move] */
  const bind = (el, down, move) => {
    el.addEventListener('pointerdown', (e) => {
      grab(e);
      down(e);
      render();
    });
    if (move)
      el.addEventListener('pointermove', (e) => {
        move(e);
        render();
      });
    for (const t of ['pointerup', 'pointercancel', 'lostpointercapture']) {
      el.addEventListener(t, (e) => {
        ctl.up(/** @type {PointerEvent} */ (e).pointerId);
        render();
      });
    }
    el.addEventListener('contextmenu', (e) => e.preventDefault());
  };
  bind($('touch-look'), (e) => ctl.lookDown(e.pointerId, e.clientX, e.clientY), (e) => ctl.lookMove(e.pointerId, e.clientX, e.clientY));
  bind(zone, (e) => ctl.stickDown(e.pointerId, e.clientX, e.clientY), (e) => ctl.stickMove(e.pointerId, e.clientX, e.clientY));
  bind(jump, (e) => ctl.buttonDown(e.pointerId, BTN.JUMP));
  bind(crouch, (e) => {
    // Toggle on the press, not the release: a thumb that slides off must not leave the player stuck squatting.
    e.preventDefault();
    ctl.toggleCrouch();
  });
  menu.addEventListener('click', () => opt.onMenu());

  // A phone that reports a mouse as its primary pointer (some desktop browsers in touch mode, laptops with a
  // screen) gets the controls on the first real touch.
  const onFirstTouch = (/** @type {PointerEvent} */ e) => {
    if (e.pointerType !== 'touch' || enabled) return;
    enabled = true;
    render();
  };
  doc.addEventListener('pointerdown', onFirstTouch, true);
  const onBlur = () => {
    ctl.releaseAll();
    render();
  };
  win.addEventListener('blur', onBlur);

  render();
  return {
    ctl,
    get enabled() {
      return enabled;
    },
    /** @param {boolean} on */
    setActive(on) {
      active = on;
      if (!on) ctl.releaseAll();
      render();
    },
    destroy() {
      doc.removeEventListener('pointerdown', onFirstTouch, true);
      win.removeEventListener('blur', onBlur);
      ctl.releaseAll();
    },
  };
}
