// @ts-check
// render/: the math agrees with the simulation's conventions, the meshes are closed and face outwards, the
// backend choice falls back to WebGL2 without navigator.gpu, and both backends run their whole upload/draw path
// against recording fakes. Node has no GPU, so pixels are not checked here; the replay page and the demo are
// the browser checks.
import assert from 'node:assert/strict';
import { mat4, quat, vec3, unitsToRad } from '../render/math.js';
import { FLOATS_PER_VERTEX, boxMesh, capsuleMesh, bounds } from '../render/meshes.js';
import { backendOrder, createRenderer } from '../render/renderer.js';
import { sinCos } from '../sim/dmath.js';

const close = (/** @type {number} */ a, /** @type {number} */ b, eps = 1e-5) => Math.abs(a - b) < eps;

test('the camera looks where the simulation walks', () => {
  for (const units of [0, 5000, 16384, -12000, 30000]) {
    const [s, c] = sinCos(units);
    const forward = /** @type {[number, number, number]} */ ([-s, 0, -c]); // sim/world.js's forward
    const eye = /** @type {[number, number, number]} */ ([3, 1.6, -2]);
    const view = mat4.fpsView(eye, unitsToRad(units), 0);
    const ahead = mat4.transformPoint(view, vec3.add(eye, forward));
    assert.ok(close(ahead[0], 0) && close(ahead[1], 0) && close(ahead[2], -1), `yaw ${units}: ${ahead}`);
  }
  // Positive pitch looks up.
  const up = mat4.transformPoint(mat4.fpsView([0, 0, 0], 0, 0.5), [0, Math.sin(0.5), -Math.cos(0.5)]);
  assert.ok(close(up[0], 0) && close(up[1], 0) && close(up[2], -1));
});

test('deterministic sin/cos match Math to 1e-9 over the whole circle', () => {
  for (let u = -70000; u <= 70000; u += 37) {
    const [s, c] = sinCos(u);
    const a = (u / 65536) * Math.PI * 2;
    assert.ok(Math.abs(s - Math.sin(a)) < 1e-9 && Math.abs(c - Math.cos(a)) < 1e-9, `units ${u}`);
  }
});

test('perspective maps near and far to 0..1 (WebGPU) and -1..1 (WebGL)', () => {
  for (const zeroToOne of [true, false]) {
    const p = mat4.perspective(1, 1.5, 0.1, 100, zeroToOne);
    const n = mat4.transformPoint(p, [0, 0, -0.1]);
    const f = mat4.transformPoint(p, [0, 0, -100]);
    assert.ok(close(n[2] / n[3], zeroToOne ? 0 : -1, 1e-4));
    assert.ok(close(f[2] / f[3], 1, 1e-4));
  }
});

test('TRS matrices and quaternions agree', () => {
  const q = quat.fromAxisAngle([0, 1, 0], Math.PI / 2);
  const m = mat4.fromTRS([1, 2, 3], q, [2, 2, 2]);
  const p = mat4.transformPoint(m, [1, 0, 0]);
  const r = vec3.add(quat.rotate(q, [2, 0, 0]), [1, 2, 3]);
  assert.ok(close(p[0], r[0]) && close(p[1], r[1]) && close(p[2], r[2]));
  const id = mat4.multiply(mat4.identity(), m);
  assert.deepEqual([...id], [...m]);
});

test('box and capsule meshes face outwards and have the expected size', () => {
  for (const [mesh, centre] of /** @type {[import('../render/meshes.js').MeshData, number[]][]} */ ([[boxMesh(), [0, 0, 0]], [capsuleMesh(0.35, 0.55), [0, 0.9, 0]]])) {
    const v = mesh.vertices;
    for (let t = 0; t < mesh.indices.length; t += 3) {
      const o = mesh.indices[t] * FLOATS_PER_VERTEX;
      const toFace = [v[o] - centre[0], v[o + 1] - centre[1], v[o + 2] - centre[2]];
      const dot = toFace[0] * v[o + 3] + toFace[1] * v[o + 4] + toFace[2] * v[o + 5];
      assert.ok(dot > 0, `triangle ${t / 3} faces inwards`);
    }
  }
  const b = bounds(capsuleMesh(0.35, 0.55));
  assert.ok(close(b.min[1], 0) && close(b.max[1], 1.8));
  assert.deepEqual(bounds(boxMesh()), { min: [-1, -1, -1], max: [1, 1, 1] });
});

test('without navigator.gpu the renderer picks WebGL2', () => {
  assert.deepEqual(backendOrder({ hasGpu: false }), ['webgl2']);
  assert.deepEqual(backendOrder({ hasGpu: true }), ['webgpu', 'webgl2']);
  assert.deepEqual(backendOrder({ hasGpu: true, prefer: 'webgl2' }), ['webgl2']);
});

/**
 * An object that answers every property with a callable that records its call and returns another such object
 * (or `ret[name]`). Enough to run a backend's code paths without a GPU.
 * @param {string[]} calls @param {Record<string, any>} [ret]
 */
function recorder(calls, ret = {}) {
  /** @type {any} */
  const handler = {
    get(/** @type {any} */ _t, /** @type {string | symbol} */ name) {
      if (typeof name === 'symbol') return undefined;
      if (name === 'then') return undefined; // not a thenable
      if (name in ret && typeof ret[name] !== 'function') return ret[name];
      return new Proxy(function () {}, {
        apply: (_f, _this, args) => {
          calls.push(name);
          if (typeof ret[name] === 'function') return ret[name](...args);
          return recorder(calls, ret);
        },
        get: handler.get,
      });
    },
  };
  return new Proxy({}, handler);
}

const frameParams = () => ({
  view: mat4.fpsView([0, 1.6, 5], 0, 0), eye: /** @type {[number, number, number]} */ ([0, 1.6, 5]), fovY: 1.2, near: 0.05, far: 200,
  lightDir: /** @type {[number, number, number]} */ ([0.3, -1, 0.2]), lightColor: /** @type {[number, number, number]} */ ([1, 1, 1]),
  ambient: /** @type {[number, number, number]} */ ([0.3, 0.3, 0.35]), fogColor: /** @type {[number, number, number]} */ ([0.6, 0.7, 0.8]), fogDensity: 0.01,
});

test('the WebGL2 backend uploads and draws through the GL API', async () => {
  /** @type {string[]} */
  const calls = [];
  const gl = recorder(calls, { getShaderParameter: () => true, getProgramParameter: () => true, TRIANGLES: 4, UNSIGNED_INT: 5125 });
  const canvas = /** @type {any} */ ({ width: 640, height: 360, getContext: (/** @type {string} */ k) => (k === 'webgl2' ? gl : null) });
  const r = await createRenderer(canvas, { nav: {} });
  assert.equal(r.kind, 'webgl2');
  const cube = r.upload(boxMesh());
  assert.equal(cube.count, 36);
  r.resize(800, 450);
  assert.equal(canvas.width, 800);
  r.frame(frameParams(), [
    { mesh: cube, model: mat4.identity(), tint: [1, 0, 0] },
    { mesh: cube, model: mat4.identity(), tint: [0, 1, 0], grid: 1 },
  ]);
  assert.equal(calls.filter((c) => c === 'drawElements').length, 2);
  r.destroy();
});

test('the WebGPU backend builds its pipeline and draws with dynamic offsets', async () => {
  /** @type {string[]} */
  const calls = [];
  /** @type {any[][]} */
  const offsets = [];
  const G = /** @type {any} */ (globalThis);
  G.GPUBufferUsage = { MAP_READ: 1, MAP_WRITE: 2, COPY_SRC: 4, COPY_DST: 8, INDEX: 16, VERTEX: 32, UNIFORM: 64 };
  G.GPUTextureUsage = { COPY_SRC: 1, COPY_DST: 2, TEXTURE_BINDING: 4, STORAGE_BINDING: 8, RENDER_ATTACHMENT: 16 };
  G.GPUShaderStage = { VERTEX: 1, FRAGMENT: 2, COMPUTE: 4 };
  const device = recorder(calls, {
    setBindGroup: (/** @type {number} */ g, /** @type {any} */ _bg, /** @type {any[]} */ off) => (g === 0 ? offsets.push(off) : undefined, undefined),
    lost: new Promise(() => {}),
    popErrorScope: async () => null,
  });
  const gpu = {
    requestAdapter: async () => ({ requestDevice: async () => device }),
    getPreferredCanvasFormat: () => 'bgra8unorm',
  };
  const ctx = recorder(calls);
  const canvas = /** @type {any} */ ({ width: 640, height: 360, getContext: (/** @type {string} */ k) => (k === 'webgpu' ? ctx : null) });
  const r = await createRenderer(canvas, { nav: { gpu } });
  assert.equal(r.kind, 'webgpu');
  assert.ok(calls.indexOf('pushErrorScope') < calls.indexOf('configure'), 'the pipeline is validated before the canvas is claimed');
  const cube = r.upload(boxMesh());
  const cap = r.upload(capsuleMesh(0.35, 0.55));
  r.frame(frameParams(), [
    { mesh: cube, model: mat4.identity(), tint: [1, 1, 1] },
    { mesh: cap, model: mat4.identity(), tint: [1, 0.5, 0] },
    { mesh: cube, model: mat4.identity(), tint: [0, 0, 1] },
  ]);
  assert.equal(calls.filter((c) => c === 'drawIndexed').length, 3);
  assert.deepEqual(offsets, [[0], [256], [512]]);
  assert.ok(calls.includes('createRenderPipeline') && calls.includes('submit'));
  delete G.GPUBufferUsage;
  delete G.GPUTextureUsage;
  delete G.GPUShaderStage;
});

test('if WebGPU gives no adapter the renderer falls back to WebGL2', async () => {
  /** @type {string[]} */
  const calls = [];
  const gl = recorder(calls, { getShaderParameter: () => true, getProgramParameter: () => true });
  const canvas = /** @type {any} */ ({ width: 1, height: 1, getContext: (/** @type {string} */ k) => (k === 'webgl2' ? gl : null) });
  const r = await createRenderer(canvas, { nav: { gpu: { requestAdapter: async () => null } } });
  assert.equal(r.kind, 'webgl2');
  await assert.rejects(createRenderer(/** @type {any} */ ({ getContext: () => null }), { nav: {} }), /No WebGPU or WebGL2/);
});
