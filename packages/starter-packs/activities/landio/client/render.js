// @ts-check
// Canvas renderer. Draws only the tiles in view: one batched path per owner for land (plus two shared
// bevel paths), one for trails, one for borders. Patterns ride on top of colour so colour is never the only
// cue. The camera follows a point with exponential smoothing; shake moves the camera, never the game.

import { lookColor, lookPattern } from '../shared/look.js';
import { CHUNK, FLAG_STOPPED } from './world.js';
import { FX } from './fx/index.js';
import { MODE_VIEWS } from './modes/index.js';

/** @typedef {import('./world.js').World} World */

const BG = '#0d1220';
const GRID = 'rgba(255,255,255,0.045)';
const OUTSIDE = '#070a12';
/** Wall tiles: the colour outside the map, with a raised rim so they read as solid. */
const WALL_RIM = 'rgba(255,255,255,0.07)';
/** Gold tiles: a warm ground, and a small gem drawn on top whoever owns the tile. */
const GOLD_GROUND = '#3a2f12';
const GOLD_GEM = '#f5c542';
const WAVE_MS = 150;
const REPAINT_BUDGET = 2;
const POP_MS = 110;
const NEW_CHUNK_BUDGET = 4;
/**
 * Most chunk canvases ever allocated. Each is GPU memory (16×16 tiles of at most 40 px: 640² px, 1.6 MB), so
 * the total stays under ~65 MB and is reused, never freed and reallocated: churning GPU textures on a small
 * integrated GPU is a crash risk (D-43).
 */
const MAX_CANVASES = 40;
const WAVE_MAX_TILES = 1500;
/** Most canvas pixels drawn per frame, whatever the screen and pixel ratio. */
const MAX_PIXELS = 2_500_000;
/** Adaptive quality: frames slower than SLOW_MS (p90 over a second) lower the resolution a step; QUICK_MS raises it. */
const SLOW_MS = 19;
const QUICK_MS = 13;
const WAVE_STAGGER_MS = 9;
const WAVE_MAX_DELAY_MS = 450;

/** @param {string} hex @param {number} f -1..1, darker..lighter */
function shade(hex, f) {
  const n = parseInt(hex.slice(1), 16);
  const ch = (/** @type {number} */ v) => Math.round(f < 0 ? v * (1 + f) : v + (255 - v) * f);
  return `rgb(${ch(n >> 16)},${ch((n >> 8) & 255)},${ch(n & 255)})`;
}

/** Ease-out with a little overshoot (the "pop"). @param {number} t 0..1 */
const easeOutBack = (t) => {
  const c = 1.70158;
  const u = t - 1;
  return 1 + (c + 1) * u * u * u + c * u * u;
};

export class Renderer {
  /**
   * @param {HTMLCanvasElement} canvas
   * @param {{reducedMotion: boolean, shake: boolean, names: boolean, quality: 'auto' | 'high' | 'low'}} prefs
   *   read every frame, so the settings menu changes them live
   */
  constructor(canvas, prefs) {
    this.canvas = canvas;
    this.ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext('2d', { alpha: false }));
    this.prefs = prefs;
    this.cam = { x: 0, y: 0, fx: 0, fy: 0, ready: false };
    /** Own head "pop" on a turn: until when (ms). */
    this.popUntil = 0;
    this.trauma = 0;
    this.shakeT = 0;
    this.tile = 24;
    this.dpr = 1;
    /** Resolution multiplier (0.5..1), lowered when frames run slow, raised when they run fast. */
    this.quality = 1;
    this.quickRuns = 0;
    this.slowRuns = 0;
    /** @type {Map<number, number>} tile -> animation start time */
    this.wave = new Map();
    /** @type {{x: number, y: number, vx: number, vy: number, life: number, max: number, color: string}[]} */
    this.particles = [];
    /** @type {Map<number, CanvasPattern | null>} */
    this.patterns = new Map();
    this.frameMs = 0;
    /** @type {Map<number, {cv: HTMLCanvasElement, ver: number, T: number, used: number, gen: number}>} cached chunk pixels */
    this.chunks = new Map();
    /** @type {HTMLCanvasElement[]} pooled chunk canvases */
    this.free = [];
    this.canvases = 0;
    this.partial = 0;
    this.frameNo = 0;
    /** @type {Record<string, number | string>} where the last draw went (ms), for the hiccup log */
    this.parts = {};
    this.slowObj = '';
    /** @type {{x: number, y: number, r: number, color: string, t0: number, ms: number}[]} expanding rings */
    this.rings = [];
    /** @type {{x: number, y: number, text: string, color: string, t0: number}[]} floating labels */
    this.texts = [];
    /** Chunk repaints this frame; beyond REPAINT_BUDGET a stale chunk shows its old pixels one more frame. */
    this.painted = 0;
  }

  /**
   * The pixels of chunk (cx, cy) at tile size T, repainted only when the world changed a tile in it.
   * @param {World} world @param {number} cx @param {number} cy @param {number} T
   */
  chunk(world, cx, cy, T) {
    const key = cy * world.cw + cx;
    let c = this.chunks.get(key);
    if (!c) {
      const cv = this.takeCanvas();
      if (!cv) return null; // pool exhausted this frame: the caller paints this chunk directly
      c = { cv, ver: -1, T: 0, used: 0, gen: -1 };
      this.chunks.set(key, c);
    }
    c.used = this.frameNo;
    const ver = world.chunkVer[key];
    const whole = c.T !== T || c.gen !== world.allGen || c.ver < 0;
    if (!whole && c.ver === ver) return c.cv;
    const S = CHUNK * T;
    const g = /** @type {CanvasRenderingContext2D} */ (c.cv.getContext('2d'));
    const d = whole ? 'all' : world.takeDirtyTiles(key);
    if (d === 'all' || !d) {
      // A whole repaint. A big capture, a territory changing hands, or a new tile size (the resolution
      // adapted) touches many chunks at once: a few per frame; the rest show their previous pixels (scaled if
      // the size changed) for a frame or two instead of one long frame. A chunk with no pixels yet paints now.
      if (this.painted >= REPAINT_BUDGET && c.ver >= 0) {
        if (!whole) world.dirtyTiles.set(key, 'all');
        return c.cv;
      }
      // A chunk with no pixels yet (a camera jump after a respawn reveals a whole screen of them): a few per
      // frame; the caller fills the rest with the background until their turn.
      if (c.ver < 0 && this.painted >= NEW_CHUNK_BUDGET) return null;
      this.painted++;
      if (c.cv.width !== S) c.cv.width = c.cv.height = S;
      if (whole) world.takeDirtyTiles(key);
      g.clearRect(0, 0, S, S);
      this.paintChunk(g, world, cx, cy, T);
    } else {
      // A few tiles changed (a trail step, a small capture): repaint only them, clipped, so the pixels are
      // exactly what a whole repaint would draw there.
      const X0 = cx * CHUNK;
      const Y0 = cy * CHUNK;
      g.save();
      g.beginPath();
      for (const i of d) {
        const x = i % world.w;
        const y = (i - x) / world.w;
        g.rect((x - X0) * T, (y - Y0) * T, T, T);
      }
      g.clip();
      g.clearRect(0, 0, S, S);
      this.paintChunk(g, world, cx, cy, T);
      g.restore();
      this.partial++;
    }
    c.ver = ver;
    c.T = T;
    c.gen = world.allGen;
    return c.cv;
  }

  /** Drop the least recently used chunks beyond `cap`. @param {number} cap */
  evict(cap) {
    if (this.chunks.size <= cap) return;
    const old = [...this.chunks.entries()].filter(([, c]) => c.used < this.frameNo).sort((a, b) => a[1].used - b[1].used);
    for (const [key, c] of old) {
      if (this.chunks.size <= cap) break;
      this.free.push(c.cv); // back to the pool: GPU memory is reused, never churned
      this.chunks.delete(key);
    }
  }

  /**
   * A canvas for a chunk: from the pool, or a new one while fewer than MAX_CANVASES exist, or the least
   * recently used chunk's (not one drawn this frame). Null when none is free this frame.
   * @returns {HTMLCanvasElement | null}
   */
  takeCanvas() {
    const pooled = this.free.pop();
    if (pooled) return pooled;
    if (this.canvases < MAX_CANVASES) {
      this.canvases++;
      return document.createElement('canvas');
    }
    let lruKey = -1;
    let lru = Infinity;
    for (const [key, c] of this.chunks)
      if (c.used < this.frameNo && c.used < lru) {
        lru = c.used;
        lruKey = key;
      }
    if (lruKey < 0) return null;
    const cv = /** @type {{cv: HTMLCanvasElement}} */ (this.chunks.get(lruKey)).cv;
    this.chunks.delete(lruKey);
    return cv;
  }

  /**
   * Paint one chunk: background, grid, land per owner with pattern and bevel, borders between owners, trails.
   * @param {CanvasRenderingContext2D} g @param {World} world @param {number} cx @param {number} cy @param {number} T
   */
  paintChunk(g, world, cx, cy, T) {
    const { w, h } = world;
    const X0 = cx * CHUNK;
    const Y0 = cy * CHUNK;
    const nx = Math.min(CHUNK, w - X0);
    const ny = Math.min(CHUNK, h - Y0);
    g.fillStyle = BG;
    g.fillRect(0, 0, nx * T, ny * T);
    const grid = new Path2D();
    for (let lx = 0; lx < nx; lx++) grid.rect(lx * T, 0, 1, ny * T);
    for (let ly = 0; ly < ny; ly++) grid.rect(0, ly * T, nx * T, 1);
    g.fillStyle = GRID;
    g.fill(grid);

    // Terrain: walls cover the grid like the space outside the map; gold warms the ground.
    const walls = new Path2D();
    const rims = new Path2D();
    const goldGround = new Path2D();
    const gems = new Path2D();
    const gem = Math.max(2, Math.round(T * 0.16));
    let anyTerrain = false;
    for (let ly = 0; ly < ny; ly++)
      for (let lx = 0; lx < nx; lx++) {
        const x = X0 + lx;
        const y = Y0 + ly;
        const i = y * w + x;
        const t = world.terrain[i];
        if (!t) continue;
        anyTerrain = true;
        const px = lx * T;
        const py = ly * T;
        if (t === 1) {
          walls.rect(px, py, T, T);
          // A rim on each side that faces open ground.
          if (y > 0 && world.terrain[i - w] !== 1) rims.rect(px, py, T, 2);
          if (x > 0 && world.terrain[i - 1] !== 1) rims.rect(px, py, 2, T);
          if (y + 1 < h && world.terrain[i + w] !== 1) rims.rect(px, py + T - 2, T, 2);
          if (x + 1 < w && world.terrain[i + 1] !== 1) rims.rect(px + T - 2, py, 2, T);
        } else if (t === 2) {
          goldGround.rect(px + 1, py + 1, T - 1, T - 1);
          // A diamond in the tile's corner, so gold still shows under land.
          const gx = px + T * 0.78;
          const gy = py + T * 0.22;
          gems.moveTo(gx, gy - gem);
          gems.lineTo(gx + gem, gy);
          gems.lineTo(gx, gy + gem);
          gems.lineTo(gx - gem, gy);
          gems.closePath();
        }
      }
    if (anyTerrain) {
      g.fillStyle = GOLD_GROUND;
      g.fill(goldGround);
      g.fillStyle = OUTSIDE;
      g.fill(walls);
      g.fillStyle = WALL_RIM;
      g.fill(rims);
    }

    /** @type {Map<number, Path2D>} */
    const land = new Map();
    /** @type {Map<number, Path2D>} */
    const trails = new Map();
    const lit = new Path2D();
    const dark = new Path2D();
    const edges = new Path2D();
    const bev = Math.max(2, Math.round(T / 8));
    for (let ly = 0; ly < ny; ly++) {
      const y = Y0 + ly;
      for (let lx = 0; lx < nx; lx++) {
        const x = X0 + lx;
        const i = y * w + x;
        const b = world.base[i];
        const px = lx * T;
        const py = ly * T;
        if (b) {
          let path = land.get(b);
          if (!path) land.set(b, (path = new Path2D()));
          path.rect(px, py, T, T);
          lit.rect(px, py, T, bev);
          lit.rect(px, py + bev, bev, T - bev);
          dark.rect(px + bev, py + T - bev, T - bev, bev);
          dark.rect(px + T - bev, py + bev, bev, T - 2 * bev);
          if (x + 1 < w && world.base[i + 1] !== b) edges.rect(px + T - 1, py, 2, T);
          if (x > 0 && world.base[i - 1] !== b) edges.rect(px - 1, py, 2, T);
          if (y + 1 < h && world.base[i + w] !== b) edges.rect(px, py + T - 1, T, 2);
          if (y > 0 && world.base[i - w] !== b) edges.rect(px, py - 1, T, 2);
        }
        const tr = world.trail[i];
        if (tr) {
          let path = trails.get(tr);
          if (!path) trails.set(tr, (path = new Path2D()));
          path.rect(px, py, T, T);
        }
      }
    }
    // Patterns repeat every 8 px: shift them so they line up across chunk borders.
    const shift = new DOMMatrix([1, 0, 0, 1, -((X0 * T) % 8), -((Y0 * T) % 8)]);
    for (const [owner, path] of land) {
      const look = world.roster.get(owner)?.look ?? owner;
      g.fillStyle = lookColor(look);
      g.fill(path);
      const pat = this.pattern(look);
      if (pat) {
        pat.setTransform(shift);
        g.fillStyle = pat;
        g.fill(path);
      }
    }
    g.fillStyle = 'rgba(255,255,255,0.22)';
    g.fill(lit);
    g.fillStyle = 'rgba(0,0,0,0.28)';
    g.fill(dark);
    g.fillStyle = 'rgba(0,0,0,0.55)';
    g.fill(edges);
    // Trails: translucent colour with a dotted centre line.
    const dot = Math.max(2, Math.round(T / 5));
    for (const [owner, path] of trails) {
      const c = lookColor(world.roster.get(owner)?.look ?? owner);
      g.globalAlpha = 0.5;
      g.fillStyle = c;
      g.fill(path);
      g.globalAlpha = 1;
      const dots = new Path2D();
      for (let ly = 0; ly < ny; ly++)
        for (let lx = 0; lx < nx; lx++)
          if (world.trail[(Y0 + ly) * w + X0 + lx] === owner) dots.rect(lx * T + (T - dot) / 2, ly * T + (T - dot) / 2, dot, dot);
      g.fillStyle = shade(c, 0.35);
      g.fill(dots);
    }
    if (anyTerrain) {
      g.fillStyle = GOLD_GEM;
      g.fill(gems);
    }
  }

  resize() {
    // Resolution: the device pixel ratio, times the adaptive quality, and never more than MAX_PIXELS in all
    // (a 2560x1440 screen at 1.5x is 8M pixels; drawing that every frame is what made big screens slow).
    const cssW = Math.max(1, this.canvas.clientWidth);
    const cssH = Math.max(1, this.canvas.clientHeight);
    let dpr = Math.min(window.devicePixelRatio || 1, 2) * this.quality;
    if (cssW * cssH * dpr * dpr > MAX_PIXELS) dpr = Math.sqrt(MAX_PIXELS / (cssW * cssH));
    dpr = Math.max(0.5, dpr);
    const w = Math.round(this.canvas.clientWidth * dpr);
    const h = Math.round(this.canvas.clientHeight * dpr);
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    this.dpr = dpr;
    // About 26 tiles across the short side, so phones and desktops see a similar area.
    this.tile = Math.max(12, Math.min(40, Math.round(Math.min(w, h) / 26)));
  }

  /** Captured tiles grow in, staggered by distance from the capturing head. @param {number[]} tiles @param {number} w @param {{x: number, y: number} | undefined} from @param {number} now */
  captureWave(tiles, w, from, now) {
    if (this.prefs.reducedMotion) return;
    // Only tiles on screen grow in, and only so many: a whole territory changing hands can be thousands.
    const v = this.view;
    for (const i of tiles) {
      if (this.wave.size >= WAVE_MAX_TILES) break;
      const x = i % w;
      const y = (i - x) / w;
      if (v && (x < v.x0 - 2 || x > v.x1 + 2 || y < v.y0 - 2 || y > v.y1 + 2)) continue;
      const d = from ? Math.abs(x - from.x) + Math.abs(y - from.y) : 0;
      this.wave.set(i, now + Math.min(d * WAVE_STAGGER_MS, WAVE_MAX_DELAY_MS));
    }
  }

  /**
   * Adapt the resolution to how fast frames actually are on this machine. Called once a second with the
   * last second's frame times (visible page only).
   * @param {number[]} frameMs
   */
  adapt(frameMs) {
    // A quality the player fixed in their settings wins over adapting.
    if (this.prefs.quality !== 'auto') {
      this.quality = this.prefs.quality === 'high' ? 1 : 0.5;
      return;
    }
    if (frameMs.length < 20) return;
    const s = [...frameMs].sort((a, b) => a - b);
    const p90 = s[Math.floor(s.length * 0.9)];
    // Down after 2 slow seconds in a row, up after 5 quick ones: a step repaints every chunk (spread over
    // frames), so it must not flip back and forth.
    if (p90 > SLOW_MS && this.quality > 0.5) {
      this.quickRuns = 0;
      if (++this.slowRuns >= 2) {
        this.quality = Math.max(0.5, this.quality * 0.8);
        this.slowRuns = 0;
      }
    } else if (p90 < QUICK_MS && this.quality < 1) {
      this.slowRuns = 0;
      if (++this.quickRuns >= 5) {
        this.quality = Math.min(1, this.quality / 0.8);
        this.quickRuns = 0;
      }
    } else {
      this.quickRuns = 0;
      this.slowRuns = 0;
    }
  }

  /** The colour of a look. @param {number} look */
  color(look) {
    return lookColor(look);
  }

  /** An expanding ring at tile (x, y) up to radius r tiles. @param {number} x @param {number} y @param {number} r @param {string} color @param {number} ms */
  ring(x, y, r, color, ms) {
    this.rings.push({ x: x + 0.5, y: y + 0.5, r, color, t0: performance.now(), ms: this.prefs.reducedMotion ? ms / 2 : ms });
  }

  /** A short label rising from tile (x, y). @param {number} x @param {number} y @param {string} text @param {string} color */
  floatText(x, y, text, color) {
    this.texts.push({ x: x + 0.5, y, text, color, t0: performance.now() });
    if (this.texts.length > 20) this.texts.shift();
  }

  /** The local head turned: pop it. */
  pop() {
    this.popUntil = performance.now() + POP_MS;
  }

  /** @param {number} amount */
  shake(amount) {
    if (this.prefs.reducedMotion || !this.prefs.shake) return;
    this.trauma = Math.min(1, this.trauma + amount);
  }

  /** A burst of squares where a head died. @param {number} x @param {number} y @param {string} color @param {number} n */
  burst(x, y, color, n) {
    const count = this.prefs.reducedMotion ? Math.min(n, 6) : n;
    for (let k = 0; k < count; k++) {
      const a = (k / count) * Math.PI * 2 + k;
      const sp = 2 + ((k * 37) % 10) / 2;
      this.particles.push({ x: x + 0.5, y: y + 0.5, vx: Math.cos(a) * sp, vy: Math.sin(a) * sp, life: 0, max: 600 + (k % 5) * 80, color });
    }
  }

  /** @param {number} look */
  pattern(look) {
    if (this.patterns.has(look)) return this.patterns.get(look) ?? null;
    const kind = lookPattern(look);
    let p = null;
    if (kind !== 'solid') {
      const c = document.createElement('canvas');
      c.width = c.height = 8;
      const g = /** @type {CanvasRenderingContext2D} */ (c.getContext('2d'));
      g.fillStyle = 'rgba(0,0,0,0.28)';
      if (kind === 'dots') g.fillRect(3, 3, 2, 2);
      else if (kind === 'stripes')
        for (let k = -8; k < 8; k += 4) {
          g.beginPath();
          g.moveTo(k, 8);
          g.lineTo(k + 8, 0);
          g.lineTo(k + 9.5, 0);
          g.lineTo(k + 1.5, 8);
          g.fill();
        }
      else {
        g.fillRect(0, 0, 8, 1);
        g.fillRect(0, 0, 1, 8);
      }
      p = this.ctx.createPattern(c, 'repeat');
    }
    this.patterns.set(look, p);
    return p;
  }

  /**
   * Draw a frame.
   * @param {World} world @param {number} r playout tick
   * @param {{follow: {x: number, y: number} | null, me: number, now: number, dt: number, cut?: boolean,
   *   own?: {head: {x: number, y: number, dir: number, flags: number}, trail: number[]} | null}} view
   *   `own`: the predicted own head and the trail tiles it has laid that the playout has not reached yet
   */
  draw(world, r, view) {
    const t0 = performance.now();
    const { ctx, canvas } = this;
    const T = this.tile;
    const W = canvas.width;
    const H = canvas.height;
    const { w, h } = world;

    // Camera: centred on the head, exponential follow (frame-rate independent). A teleport (respawn) is a
    // hard cut instead of a swoop across the map. A look-ahead and an edge clamp were tried and felt
    // disorienting (D-48).
    const target = view.follow ?? { x: w / 2, y: h / 2 };
    const jumped = this.cam.ready && Math.abs(target.x - this.cam.fx) + Math.abs(target.y - this.cam.fy) > 6;
    if (!this.cam.ready || view.cut || jumped) {
      this.cam.x = target.x;
      this.cam.y = target.y;
      this.cam.ready = true;
    } else {
      const k = 1 - Math.exp(-view.dt / 90);
      this.cam.x += (target.x - this.cam.x) * k;
      this.cam.y += (target.y - this.cam.y) * k;
    }
    this.cam.fx = target.x;
    this.cam.fy = target.y;
    let sx = 0;
    let sy = 0;
    if (this.trauma > 0) {
      this.trauma = Math.max(0, this.trauma - view.dt / 700);
      this.shakeT += view.dt / 33;
      const s = this.trauma * this.trauma * T * 0.6;
      sx = s * Math.sin(this.shakeT * 1.7);
      sy = s * Math.sin(this.shakeT * 2.3);
    }
    const ox = Math.round(W / 2 - (this.cam.x + 0.5) * T + sx);
    const oy = Math.round(H / 2 - (this.cam.y + 0.5) * T + sy);

    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = OUTSIDE;
    ctx.fillRect(0, 0, W, H);
    if (!w) return;
    ctx.setTransform(1, 0, 0, 1, ox, oy);
    const x0 = Math.max(0, Math.floor(-ox / T));
    const y0 = Math.max(0, Math.floor(-oy / T));
    const x1 = Math.min(w - 1, Math.ceil((W - ox) / T));
    const y1 = Math.min(h - 1, Math.ceil((H - oy) / T));

    // Tiles come from cached chunks: a chunk is repainted only when the world bumped its version (at most once
    // per tick), so a frame is a handful of image copies instead of thousands of tile fills.
    this.frameNo++;
    this.painted = 0;
    this.partial = 0;
    const S = CHUNK * T;
    const cx1 = Math.floor(x1 / CHUNK);
    const cy1 = Math.floor(y1 / CHUNK);
    for (let cy = Math.floor(y0 / CHUNK); cy <= cy1; cy++)
      for (let cx = Math.floor(x0 / CHUNK); cx <= cx1; cx++) {
        const cv = this.chunk(world, cx, cy, T);
        if (cv) ctx.drawImage(cv, cx * S, cy * S, S, S); // S, S: a chunk still at an old tile size is scaled
        else {
          // Not painted yet (budget or pool): plain background for a frame or two.
          ctx.fillStyle = BG;
          ctx.fillRect(cx * S, cy * S, S, S);
        }
      }
    this.evict(((cx1 - Math.floor(x0 / CHUNK) + 1) * (cy1 - Math.floor(y0 / CHUNK) + 1)) * 3);

    const tTiles = performance.now();
    // Tiles still growing in from a capture: cover the cached tile and draw it scaled.
    const now = view.now;
    for (const [i, start] of this.wave) {
      const p = (now - start) / WAVE_MS;
      if (p >= 1) {
        this.wave.delete(i);
        continue;
      }
      const x = i % w;
      const y = (i - x) / w;
      if (x < x0 || x > x1 || y < y0 || y > y1) continue;
      const b = world.base[i];
      ctx.fillStyle = BG;
      ctx.fillRect(x * T, y * T, T, T);
      if (!b || p <= 0) continue;
      const s = T * Math.max(0, Math.min(1.08, easeOutBack(p)));
      const off = (T - s) / 2;
      ctx.fillStyle = lookColor(world.roster.get(b)?.look ?? b);
      ctx.fillRect(x * T + off, y * T + off, s, s);
    }
    const dot = Math.max(2, Math.round(T / 5));
    if (view.own && view.own.trail.length) {
      const c = lookColor(world.roster.get(view.me)?.look ?? view.me);
      for (const i of view.own.trail) {
        if (world.trail[i] === view.me) continue;
        const x = i % w;
        const y = (i - x) / w;
        ctx.globalAlpha = 0.5;
        ctx.fillStyle = c;
        ctx.fillRect(x * T, y * T, T, T);
        ctx.globalAlpha = 1;
        ctx.fillStyle = shade(c, 0.35);
        ctx.fillRect(x * T + (T - dot) / 2, y * T + (T - dot) / 2, dot, dot);
      }
    }

    // Map edge.
    ctx.strokeStyle = 'rgba(255,90,90,0.6)';
    ctx.lineWidth = 2;
    ctx.strokeRect(-1, -1, w * T + 2, h * T + 2);

    const tWave = performance.now();
    // Card objects (grenades, snares, walls, mirages), each drawn by its card's module.
    const objs = world.objs.get(Math.floor(r)) ?? world.objs.get(world.appliedTick) ?? [];
    for (const o of objs) {
      if (o.x < x0 - 3 || o.x > x1 + 3 || o.y < y0 - 3 || o.y > y1 + 3) continue;
      const ts = performance.now();
      FX[o.kind]?.drawObject?.(ctx, T, o, now, world, r, this.prefs.reducedMotion);
      const took = performance.now() - ts;
      if (took > 8) this.slowObj = `${o.kind} ${took.toFixed(1)}ms`;
    }

    // The mode's own marks (a hill, the closing border), under the heads.
    const mv = world.round ? MODE_VIEWS[world.round.mode] : undefined;
    if (mv?.drawWorld) {
      ctx.save();
      mv.drawWorld(ctx, world, T, now);
      ctx.restore();
    }
    const tObjs = performance.now();
    // Heads.
    ctx.textAlign = 'center';
    ctx.textBaseline = 'bottom';
    ctx.font = `600 ${Math.max(11, Math.round(T * 0.5))}px system-ui, sans-serif`;
    const ids = world.headIds(r);
    if (view.own) ids.add(view.me);
    for (const id of ids) {
      const hd = id === view.me && view.own ? view.own.head : world.headAt(id, r);
      if (!hd) continue;
      if (hd.x < x0 - 2 || hd.x > x1 + 2 || hd.y < y0 - 2 || hd.y > y1 + 2) continue;
      const info = world.roster.get(id);
      const look = info?.look ?? id;
      const c = lookColor(look);
      const px = hd.x * T;
      const py = hd.y * T;
      const pad = T * 0.06;
      const fl = hd.flags;
      // A turn makes the own head pop a little (game-feel: immediate, visible acknowledgement of input).
      const popping = id === view.me && now < this.popUntil && !this.prefs.reducedMotion;
      if (popping) {
        const k = 1 + 0.18 * ((this.popUntil - now) / POP_MS);
        ctx.save();
        ctx.translate(px + T / 2, py + T / 2);
        ctx.scale(k, k);
        ctx.translate(-(px + T / 2), -(py + T / 2));
      }
      // Card effects others can see, from the head flags (shared/codec.js).
      if (fl & 16) {
        // Dash: streaks behind.
        const bx = -[0, 1, 0, -1][hd.dir] * T;
        const by = -[-1, 0, 1, 0][hd.dir] * T;
        ctx.fillStyle = 'rgba(255,255,255,0.35)';
        for (let k = 1; k <= 3; k++) ctx.fillRect(px + bx * k * 0.5 + T * 0.3, py + by * k * 0.5 + T * 0.3, T * 0.4, T * 0.4);
      }
      if (fl & 8) ctx.globalAlpha = 0.45; // Phase: see-through
      ctx.fillStyle = shade(c, -0.35);
      ctx.fillRect(px + pad, py + pad, T - 2 * pad, T - 2 * pad);
      ctx.fillStyle = c;
      ctx.fillRect(px + pad * 3, py + pad * 3, T - 6 * pad, T - 6 * pad);
      ctx.lineWidth = id === view.me ? 3 : 1.5;
      ctx.strokeStyle = id === view.me ? '#ffffff' : 'rgba(255,255,255,0.7)';
      ctx.strokeRect(px + pad, py + pad, T - 2 * pad, T - 2 * pad);
      // Eyes look where the head is going; a stopped head shows a bar.
      ctx.fillStyle = '#111';
      if (hd.flags & FLAG_STOPPED || hd.dir < 0) ctx.fillRect(px + T * 0.3, py + T * 0.45, T * 0.4, T * 0.1);
      else {
        const ex = [0, 1, 0, -1][hd.dir] * T * 0.16;
        const ey = [-1, 0, 1, 0][hd.dir] * T * 0.16;
        const e = Math.max(2, T * 0.12);
        const sxp = hd.dir % 2 === 0 ? T * 0.17 : 0;
        const syp = hd.dir % 2 === 1 ? T * 0.17 : 0;
        ctx.fillRect(px + T / 2 + ex - sxp - e / 2, py + T / 2 + ey - syp - e / 2, e, e);
        ctx.fillRect(px + T / 2 + ex + sxp - e / 2, py + T / 2 + ey + syp - e / 2, e, e);
      }
      ctx.globalAlpha = 1;
      if (popping) ctx.restore();
      if (fl & 256) {
        // Spawn protection: a faint dashed bubble.
        ctx.strokeStyle = 'rgba(255,255,255,0.55)';
        ctx.lineWidth = Math.max(1.5, T * 0.06);
        ctx.setLineDash([T * 0.2, T * 0.15]);
        ctx.beginPath();
        ctx.arc(px + T / 2, py + T / 2, T * 0.8, 0, Math.PI * 2);
        ctx.stroke();
        ctx.setLineDash([]);
      }
      if (fl & 4) {
        // Shield: a bubble.
        ctx.strokeStyle = 'rgba(86,180,233,0.9)';
        ctx.lineWidth = Math.max(2, T * 0.1);
        ctx.beginPath();
        ctx.arc(px + T / 2, py + T / 2, T * 0.85, 0, Math.PI * 2);
        ctx.stroke();
      }
      if (fl & 32) {
        // Stunned: stars circling.
        ctx.fillStyle = '#F0E442';
        for (let k = 0; k < 3; k++) {
          const a = now / 200 + (k * Math.PI * 2) / 3;
          ctx.fillRect(px + T / 2 + Math.cos(a) * T * 0.6 - 2, py + T * 0.1 + Math.sin(a) * T * 0.2 - 2, 4, 4);
        }
      }
      if (fl & 128) {
        // EMP'd: a crackle.
        ctx.strokeStyle = '#CC79A7';
        ctx.lineWidth = 2;
        ctx.beginPath();
        for (let k = 0; k <= 6; k++) ctx.lineTo(px + (k / 6) * T, py - T * 0.15 + ((k + Math.floor(now / 80)) % 2) * T * 0.15);
        ctx.stroke();
      }
      if (fl & 64) {
        // Second Wind ready: a halo.
        ctx.strokeStyle = 'rgba(255,255,255,0.8)';
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.ellipse(px + T / 2, py - T * 0.05, T * 0.35, T * 0.1, 0, 0, Math.PI * 2);
        ctx.stroke();
      }
      if (info && (this.prefs.names || id === view.me)) {
        const label = info.bot ? `${info.name} · bot` : info.name;
        ctx.fillStyle = 'rgba(0,0,0,0.55)';
        ctx.fillText(label, px + T / 2 + 1, py - 2 + 1);
        ctx.fillStyle = info.bot ? '#c9d1e0' : '#ffffff';
        ctx.fillText(label, px + T / 2, py - 2);
      }
    }

    const tHeads = performance.now();
    // Particles.
    for (const p of this.particles) {
      p.life += view.dt;
      const k = view.dt / 1000;
      p.x += p.vx * k;
      p.y += p.vy * k;
      p.vx *= 0.96;
      p.vy *= 0.96;
      const a = 1 - p.life / p.max;
      if (a <= 0) continue;
      ctx.globalAlpha = a;
      ctx.fillStyle = p.color;
      const s = T * 0.35 * a + 2;
      ctx.fillRect(p.x * T - s / 2, p.y * T - s / 2, s, s);
    }
    ctx.globalAlpha = 1;
    this.particles = this.particles.filter((p) => p.life < p.max);

    // Rings and floating labels from card effects.
    const tnow = performance.now();
    for (const g of this.rings) {
      const k = (tnow - g.t0) / g.ms;
      if (k >= 1) continue;
      ctx.globalAlpha = 1 - k;
      ctx.strokeStyle = g.color;
      ctx.lineWidth = Math.max(2, T * 0.12);
      ctx.beginPath();
      ctx.arc(g.x * T, g.y * T, Math.max(1, g.r * T * k), 0, Math.PI * 2);
      ctx.stroke();
    }
    this.rings = this.rings.filter((g) => tnow - g.t0 < g.ms);
    ctx.font = `700 ${Math.max(12, Math.round(T * 0.55))}px system-ui, sans-serif`;
    for (const f of this.texts) {
      const k = (tnow - f.t0) / 1100;
      if (k >= 1) continue;
      ctx.globalAlpha = 1 - k;
      ctx.fillStyle = 'rgba(0,0,0,0.6)';
      ctx.fillText(f.text, f.x * T + 1, (f.y - k * 1.2) * T + 1);
      ctx.fillStyle = f.color;
      ctx.fillText(f.text, f.x * T, (f.y - k * 1.2) * T);
    }
    this.texts = this.texts.filter((f) => tnow - f.t0 < 1100);
    ctx.globalAlpha = 1;
    this.frameMs = performance.now() - t0;
    // Where this frame's draw went (ms), for the hiccup log.
    this.parts = {
      tiles: +(tTiles - t0).toFixed(1),
      waveMs: +(tWave - tTiles).toFixed(1),
      objs: +(tObjs - tWave).toFixed(1),
      heads: +(tHeads - tObjs).toFixed(1),
      fx: +(performance.now() - tHeads).toFixed(1),
      partial: this.partial,
      ...(this.slowObj ? { slowObj: this.slowObj } : {}),
    };
    this.slowObj = '';
    this.view = { x0, y0, x1, y1 };
  }
}
