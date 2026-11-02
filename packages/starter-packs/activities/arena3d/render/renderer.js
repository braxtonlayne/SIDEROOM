// @ts-check
// The renderer interface and the backend choice: WebGPU when `navigator.gpu` gives an adapter, WebGL2
// otherwise (headless CI and older browsers usually have no WebGPU). Both backends draw the same scene from
// the same calls, so the game never branches on which one it got. `createRenderer` also survives losing the GPU
// (see its comment).
//
//   const r = await createRenderer(canvas);          // r.kind is 'webgpu' or 'webgl2'
//   const cube = r.upload(boxMesh());
//   r.frame({ view, eye, fovY: 1.2, ... }, [{ mesh: cube, model, tint: [1, 0.5, 0.2] }]);

import { createWebGPURenderer } from './webgpu.js';
import { createWebGL2Renderer } from './webgl2.js';

/** @typedef {import('./meshes.js').MeshData} MeshData */
/** @typedef {import('./math.js').Mat4} Mat4 */
/** @typedef {import('./math.js').Vec3} Vec3 */

/** An uploaded mesh. @typedef {{ readonly count: number, readonly id: number }} MeshHandle */
/** An uploaded texture. @typedef {{ readonly id: number, readonly width: number, readonly height: number }} TextureHandle */

/**
 * @typedef {object} Draw
 * @property {MeshHandle} mesh
 * @property {Float32Array} model             column-major model matrix
 * @property {[number, number, number]} tint  linear RGB multiplier
 * @property {number} [grid]                  0..1, strength of the world-space floor grid
 * @property {TextureHandle | null} [texture] base colour texture (white when absent)
 */

/**
 * @typedef {object} FrameParams
 * @property {Mat4} view
 * @property {Vec3} eye
 * @property {number} fovY        radians
 * @property {number} near
 * @property {number} far
 * @property {Vec3} lightDir      direction the light travels (normalised)
 * @property {Vec3} lightColor
 * @property {Vec3} ambient
 * @property {Vec3} fogColor      also the clear colour
 * @property {number} fogDensity
 */

/**
 * @typedef {object} Renderer
 * @property {'webgpu' | 'webgl2'} kind
 * @property {(mesh: MeshData) => MeshHandle} upload
 * @property {(image: ImageBitmap | HTMLCanvasElement | OffscreenCanvas) => TextureHandle} texture
 * @property {(width: number, height: number) => void} resize   device pixels
 * @property {(params: FrameParams, draws: Draw[]) => void} frame
 * @property {() => void} destroy
 * @property {HTMLCanvasElement} [canvas]   the canvas drawn to now (it changes when recovery needs a fresh one)
 * @property {'ok' | 'recovering' | 'failed'} [state]
 * @property {(cb: (canvas: HTMLCanvasElement) => void) => () => void} [onCanvasChange]
 */

/**
 * Which backend to try first. Exported for tests.
 * @param {{ hasGpu: boolean, prefer?: 'auto' | 'webgpu' | 'webgl2' }} env
 * @returns {('webgpu' | 'webgl2')[]}
 */
export function backendOrder(env) {
  if (env.prefer === 'webgl2' || !env.hasGpu) return ['webgl2'];
  return ['webgpu', 'webgl2'];
}

/** How long a lost WebGL context gets to be restored before a fresh canvas is tried. */
const RESTORE_WAIT_MS = 3000;
/** A second WebGPU loss within this many ms stops trusting WebGPU and goes to WebGL2. */
const REPEAT_LOSS_MS = 30_000;
/** Rendering that throws this many frames in a row counts as a loss. */
const MAX_FRAME_ERRORS = 3;

/**
 * @typedef {object} RendererOptions
 * @property {'auto' | 'webgpu' | 'webgl2'} [prefer]
 * @property {any} [nav]                                       tests: a fake `navigator`
 * @property {(why: string) => void} [onLost]                  the GPU was lost; recovery starts
 * @property {(kind: 'webgpu' | 'webgl2', was: 'webgpu' | 'webgl2') => void} [onRecovered]   drawing again (maybe on the other backend)
 * @property {(why: string) => void} [onFailed]                nothing worked; the page can only ask for a reload
 * @property {() => HTMLCanvasElement | null} [replaceCanvas]  a fresh canvas in the old one's place (a canvas that
 *   ever had a 'webgpu' context cannot give a 'webgl2' one, so WebGPU -> WebGL2 needs a new element)
 * @property {(ms: number) => Promise<void>} [delay]           tests: a virtual clock
 * @property {() => number} [now]                              tests
 * @property {number} [restoreWaitMs]
 */

/**
 * The renderer the game draws with. It picks the backend like `backendOrder` says, and it survives losing the GPU:
 *
 *   - WebGPU `device.lost` (a driver reset, the GPU process crashed): a new device on the same canvas; if that
 *     fails, WebGL2 on a fresh canvas. A second loss within 30 s skips straight to WebGL2.
 *   - WebGL `webglcontextlost`: wait for `webglcontextrestored` (3 s) and rebuild on the same canvas; if it never
 *     comes, WebGL2 on a fresh canvas.
 *   - Rendering that throws on 3 frames in a row is treated as a loss.
 *
 * Every mesh and texture ever uploaded is remembered and uploaded again to the new backend, in the same order, so
 * the handles the scene holds stay valid (an id is the number of the upload, in both backends and here). While
 * recovering, `frame()` draws nothing and `upload()` still works. If every step fails, `onFailed` is called and
 * `state` is 'failed'.
 *
 * @param {HTMLCanvasElement} canvas
 * @param {RendererOptions} [opt]
 * @returns {Promise<Renderer>}
 */
export async function createRenderer(canvas, opt = {}) {
  const nav = opt.nav ?? (typeof navigator === 'undefined' ? undefined : navigator);
  const delay = opt.delay ?? ((/** @type {number} */ ms) => new Promise((r) => setTimeout(r, ms)));
  const now = opt.now ?? (() => Date.now());
  const restoreWaitMs = opt.restoreWaitMs ?? RESTORE_WAIT_MS;
  /** @type {HTMLCanvasElement} */
  let el = canvas;
  /** @type {Renderer | null} */
  let backend = null;
  /** Bumped by every open(): a callback from an older backend is stale. */
  let gen = 0;
  /** @type {'ok' | 'recovering' | 'failed' | 'destroyed'} */
  let state = 'ok';
  let frameErrors = 0;
  let lastGpuLoss = -Infinity;
  /** @type {(() => void) | null} wakes recover() when the lost WebGL context comes back */
  let restoredWake = null;
  /** Every upload so far, replayed on a rebuild. @type {({ kind: 'mesh', data: MeshData } | { kind: 'texture', data: ImageBitmap | HTMLCanvasElement | OffscreenCanvas })[]} */
  const records = [];
  /** @type {[number, number] | null} */
  let size = null;
  /** @type {Set<(c: HTMLCanvasElement) => void>} */
  const canvasWatchers = new Set();

  /**
   * One backend on `target`, or null/throws. Its loss callbacks are ignored once a newer backend exists.
   * @param {'webgpu' | 'webgl2'} kind @param {HTMLCanvasElement} target
   * @returns {Promise<Renderer | null>}
   */
  async function open(kind, target) {
    const mine = ++gen;
    if (kind === 'webgpu') {
      return createWebGPURenderer(target, nav?.gpu, (why) => {
        if (mine === gen) lost('webgpu', why);
      });
    }
    return createWebGL2Renderer(target, {
      onLost: (why) => {
        if (mine === gen) lost('webgl2', why);
      },
      onRestored: () => {
        if (mine === gen) restoredWake?.();
      },
    });
  }

  /** Upload everything again, in order; the ids must come out the same. @param {Renderer} r */
  function replay(r) {
    records.forEach((rec, i) => {
      const h = rec.kind === 'mesh' ? r.upload(rec.data) : r.texture(rec.data);
      if (h.id !== i + 1) throw new Error(`replayed upload ${i + 1} got id ${h.id}`);
    });
    if (size) r.resize(size[0], size[1]);
  }

  const gone = () => state === 'destroyed';

  /** @param {'webgpu' | 'webgl2'} kind @param {string} why */
  function lost(kind, why) {
    if (state !== 'ok') return;
    state = 'recovering';
    frameErrors = 0;
    opt.onLost?.(why);
    const t = now();
    const repeat = kind === 'webgpu' && t - lastGpuLoss < REPEAT_LOSS_MS;
    if (kind === 'webgpu') lastGpuLoss = t;
    void recover(kind, repeat);
  }

  /** @param {'webgpu' | 'webgl2'} was @param {boolean} repeat */
  async function recover(was, repeat) {
    const old = backend;
    backend = null;
    /** @type {string[]} */
    const why = [];
    // [kind, fresh canvas?, wait for a restored context first?]
    /** @type {[('webgpu' | 'webgl2'), boolean, boolean][]} */
    const steps =
      was === 'webgpu'
        ? repeat
          ? [['webgl2', true, false]]
          : [['webgpu', false, false], ['webgl2', true, false]]
        : [['webgl2', false, true], ['webgl2', true, false]];
    for (const [kind, freshCanvas, waitRestore] of steps) {
      if (gone()) return;
      let target = el;
      if (waitRestore) {
        const came = await new Promise((resolve) => {
          restoredWake = () => resolve(true);
          void delay(restoreWaitMs).then(() => resolve(false));
        });
        restoredWake = null;
        if (!came) {
          why.push('the WebGL context was not restored');
          continue;
        }
      } else await delay(0);
      if (freshCanvas) {
        const next = opt.replaceCanvas?.();
        if (!next) {
          why.push('no fresh canvas');
          continue;
        }
        target = next;
      }
      if (gone()) return;
      try {
        // The old backend's objects are gone with its device or context; release what can be released.
        if (old && !freshCanvas) {
          try {
            old.destroy();
          } catch {
            /* already lost */
          }
        }
        const r = await open(kind, target);
        if (!r) {
          why.push(`${kind}: not available`);
          continue;
        }
        replay(r);
        if (target !== el) {
          el = target;
          canvasWatchers.forEach((cb) => cb(target));
        }
        backend = r;
        state = 'ok';
        frameErrors = 0;
        opt.onRecovered?.(kind, was);
        return;
      } catch (e) {
        why.push(`${kind}: ${e instanceof Error ? e.message : String(e)}`);
      }
    }
    state = 'failed';
    opt.onFailed?.(why.join('; ') || 'could not restore the GPU');
  }

  // First backend, as before: WebGPU when there is an adapter, else WebGL2.
  const order = backendOrder({ hasGpu: !!nav?.gpu, prefer: opt.prefer });
  /** @type {string[]} */
  const why = [];
  for (const kind of order) {
    try {
      const r = await open(kind, el);
      if (r) {
        backend = r;
        break;
      }
      why.push(`${kind}: not available`);
    } catch (e) {
      why.push(`${kind}: ${e instanceof Error ? e.message : String(e)}`);
    }
  }
  if (!backend) throw new Error(`No WebGPU or WebGL2 on this device (${why.join('; ')}).`);
  /** The backend kind while there is none (recovering). */
  let lastKind = backend.kind;

  /** @type {Renderer} */
  const wrapper = {
    get kind() {
      if (backend) lastKind = backend.kind;
      return lastKind;
    },
    get canvas() {
      return el;
    },
    get state() {
      return state === 'destroyed' ? 'failed' : state;
    },
    onCanvasChange(cb) {
      canvasWatchers.add(cb);
      return () => canvasWatchers.delete(cb);
    },
    upload(mesh) {
      const handle = { id: records.length + 1, count: mesh.indices.length };
      records.push({ kind: 'mesh', data: mesh });
      if (backend) {
        try {
          if (backend.upload(mesh).id !== handle.id) throw new Error('upload ids out of step');
        } catch (e) {
          if (state === 'ok') {
            records.pop();
            throw e;
          }
        }
      }
      return handle;
    },
    texture(image) {
      const handle = { id: records.length + 1, width: image.width, height: image.height };
      records.push({ kind: 'texture', data: image });
      if (backend) {
        try {
          if (backend.texture(image).id !== handle.id) throw new Error('upload ids out of step');
        } catch (e) {
          if (state === 'ok') {
            records.pop();
            throw e;
          }
        }
      }
      return handle;
    },
    resize(width, height) {
      size = [width, height];
      backend?.resize(width, height);
    },
    frame(params, draws) {
      if (!backend) return;
      try {
        backend.frame(params, draws);
        frameErrors = 0;
      } catch (e) {
        if (++frameErrors >= MAX_FRAME_ERRORS) lost(backend.kind, e instanceof Error ? e.message : String(e));
      }
    },
    destroy() {
      state = 'destroyed';
      backend?.destroy();
      backend = null;
      canvasWatchers.clear();
    },
  };
  return wrapper;
}
