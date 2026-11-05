// @ts-check
// The shared canvas: draws the turn's ink (shared/rules.js strokes) on white paper, and turns the drawer's pointer
// (mouse, pen or finger: pointer events, touch-action none) into ink ops in the 1000 x 750 logical space. Points are
// sent in batches every 50 ms so a phone on a slow link still draws smoothly.

import { CANVAS_H, CANVAS_W, ERASER, PALETTE, PAPER, SIZES, strokesOf } from '../shared/rules.js';

const FLUSH_MS = 50;
/** Points closer than this (logical units) to the previous one are skipped. */
const MIN_STEP = 2;

/**
 * @param {HTMLCanvasElement} canvas
 * @param {{ canDraw: () => boolean, tool: () => { c: number, w: number }, emit: (op: object) => void }} opt
 */
export function createPad(canvas, opt) {
  const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext('2d'));
  /** @type {import('../shared/rules.js').InkOp[]} */
  let ops = [];
  let dirty = true;
  let nextId = Math.floor(Math.random() * 1e6) * 100;
  /** @type {{ id: number, c: number, w: number, buf: number[], lastX: number, lastY: number, pointer: number } | null} */
  let live = null;
  /** @type {ReturnType<typeof setInterval> | null} */
  let flusher = null;

  function size() {
    const r = canvas.getBoundingClientRect();
    const dpr = Math.min(3, window.devicePixelRatio || 1);
    const w = Math.max(1, Math.round(r.width * dpr));
    const h = Math.max(1, Math.round(r.height * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
      dirty = true;
    }
  }

  function render() {
    size();
    if (!dirty) return;
    dirty = false;
    const sx = canvas.width / CANVAS_W;
    const sy = canvas.height / CANVAS_H;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = PAPER;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(sx, 0, 0, sy, 0, 0);
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    for (const s of strokesOf(ops)) {
      const color = s.c === ERASER ? PAPER : (PALETTE[s.c] ?? PALETTE[0]);
      const width = SIZES[s.w] ?? SIZES[0];
      const p = s.pts;
      if (p.length === 2) {
        ctx.fillStyle = color;
        ctx.beginPath();
        ctx.arc(p[0], p[1], width / 2, 0, Math.PI * 2);
        ctx.fill();
        continue;
      }
      ctx.strokeStyle = color;
      ctx.lineWidth = width;
      ctx.beginPath();
      ctx.moveTo(p[0], p[1]);
      for (let i = 2; i < p.length; i += 2) ctx.lineTo(p[i], p[i + 1]);
      ctx.stroke();
    }
  }

  /** @param {PointerEvent} e */
  function point(e) {
    const r = canvas.getBoundingClientRect();
    return [
      Math.round(((e.clientX - r.left) / Math.max(1, r.width)) * CANVAS_W),
      Math.round(((e.clientY - r.top) / Math.max(1, r.height)) * CANVAS_H),
    ];
  }

  function flush() {
    if (!live || live.buf.length === 0) return;
    opt.emit({ k: 'p', id: live.id, c: live.c, w: live.w, pts: live.buf });
    live.buf = [];
  }

  canvas.addEventListener('pointerdown', (e) => {
    if (!opt.canDraw() || (e.pointerType === 'mouse' && e.button !== 0)) return;
    e.preventDefault();
    canvas.setPointerCapture?.(e.pointerId);
    const { c, w } = opt.tool();
    const [x, y] = point(e);
    live = { id: ++nextId, c, w, buf: [x, y], lastX: x, lastY: y, pointer: e.pointerId };
    flush();
    flusher = setInterval(flush, FLUSH_MS);
  });
  canvas.addEventListener('pointermove', (e) => {
    if (!live || e.pointerId !== live.pointer) return;
    e.preventDefault();
    const events = typeof e.getCoalescedEvents === 'function' ? e.getCoalescedEvents() : [e];
    for (const ev of events.length ? events : [e]) {
      const [x, y] = point(ev);
      if (Math.hypot(x - live.lastX, y - live.lastY) < MIN_STEP) continue;
      live.buf.push(x, y);
      live.lastX = x;
      live.lastY = y;
      if (live.buf.length >= 120) flush();
    }
  });
  const end = (/** @type {PointerEvent} */ e) => {
    if (!live || e.pointerId !== live.pointer) return;
    flush();
    live = null;
    if (flusher) clearInterval(flusher);
    flusher = null;
  };
  canvas.addEventListener('pointerup', end);
  canvas.addEventListener('pointercancel', end);
  canvas.addEventListener('lostpointercapture', end);

  return {
    /** @param {import('../shared/rules.js').InkOp[]} next */
    setInk(next) {
      ops = next;
      dirty = true;
    },
    render,
    invalidate() {
      dirty = true;
    },
    /** Stop a stroke in progress (the turn ended). */
    cancel() {
      live = null;
      if (flusher) clearInterval(flusher);
      flusher = null;
    },
  };
}
