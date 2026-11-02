// @ts-check
// Losing the GPU mid-match (render/renderer.js): a WebGPU device lost, a WebGL context lost, rendering that keeps
// throwing. The renderer rebuilds itself, uploads every mesh and texture again under the same handles, falls back
// from WebGPU to WebGL2 on a fresh canvas when it must, and says so through its callbacks. Node has no GPU, so
// this runs the real backends against recording fakes (the same technique as test/render.test.js).
import assert from 'node:assert/strict';
import { mat4 } from '../render/math.js';
import { boxMesh, capsuleMesh } from '../render/meshes.js';
import { createRenderer } from '../render/renderer.js';

/**
 * An object that answers every property with a callable that records its call (in `calls`) and returns another
 * such object, or `ret[name]`.
 * @param {string[]} calls @param {Record<string, any>} [ret]
 */
function recorder(calls, ret = {}) {
  /** @type {any} */
  const handler = {
    get(/** @type {any} */ _t, /** @type {string | symbol} */ name) {
      if (typeof name === 'symbol') return undefined;
      if (name === 'then') return undefined;
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

const G = /** @type {any} */ (globalThis);
G.GPUBufferUsage = { MAP_READ: 1, MAP_WRITE: 2, COPY_SRC: 4, COPY_DST: 8, INDEX: 16, VERTEX: 32, UNIFORM: 64 };
G.GPUTextureUsage = { COPY_SRC: 1, COPY_DST: 2, TEXTURE_BINDING: 4, STORAGE_BINDING: 8, RENDER_ATTACHMENT: 16 };
G.GPUShaderStage = { VERTEX: 1, FRAGMENT: 2, COMPUTE: 4 };

/** A WebGPU device whose loss the test triggers. */
/** @param {Record<string, any>} [extra] */
function makeDevice(extra = {}) {
  /** @type {string[]} */
  const calls = [];
  /** @type {(info: any) => void} */
  let resolveLost = () => {};
  const lost = new Promise((r) => (resolveLost = r));
  const device = recorder(calls, {
    lost,
    ...extra,
    popErrorScope: async () => null,
    destroy: () => resolveLost({ reason: 'destroyed', message: 'destroyed' }),
  });
  return { device, calls, lose: (/** @type {string} */ message) => resolveLost({ reason: 'unknown', message }) };
}

/** @param {{ devices?: ReturnType<typeof makeDevice>[], failFrom?: number }} [opt] */
function makeGpu(opt = {}) {
  const devices = opt.devices ?? [];
  let n = 0;
  return {
    devices,
    gpu: {
      requestAdapter: async () => ({
        requestDevice: async () => {
          const d = devices[n++] ?? makeDevice();
          if (!devices.includes(d)) devices.push(d);
          if (opt.failFrom !== undefined && n > opt.failFrom) throw new Error('no device');
          return d.device;
        },
      }),
      getPreferredCanvasFormat: () => 'bgra8unorm',
    },
  };
}

/** @param {{ webgpu?: boolean, webgl2?: boolean }} [ctxs] */
function makeCanvas(ctxs = { webgpu: true, webgl2: true }) {
  /** @type {string[]} */
  const calls = [];
  /** @type {Record<string, (e: any) => void>} */
  const handlers = {};
  const gl = recorder(calls, { getShaderParameter: () => true, getProgramParameter: () => true });
  const gpuCtx = recorder(calls);
  const canvas = /** @type {any} */ ({
    width: 640,
    height: 360,
    calls,
    handlers,
    getContext: (/** @type {string} */ k) => (k === 'webgpu' && ctxs.webgpu ? gpuCtx : k === 'webgl2' && ctxs.webgl2 ? gl : null),
    addEventListener: (/** @type {string} */ t, /** @type {(e: any) => void} */ f) => void (handlers[t] = f),
    removeEventListener: (/** @type {string} */ t) => void delete handlers[t],
  });
  return canvas;
}

const frameParams = () => ({
  view: mat4.fpsView([0, 1.6, 5], 0, 0), eye: /** @type {[number, number, number]} */ ([0, 1.6, 5]), fovY: 1.2, near: 0.05, far: 200,
  lightDir: /** @type {[number, number, number]} */ ([0.3, -1, 0.2]), lightColor: /** @type {[number, number, number]} */ ([1, 1, 1]),
  ambient: /** @type {[number, number, number]} */ ([0.3, 0.3, 0.35]), fogColor: /** @type {[number, number, number]} */ ([0.6, 0.7, 0.8]), fogDensity: 0.01,
});
const settle = async () => {
  for (let i = 0; i < 12; i++) await new Promise((r) => setTimeout(r, 0));
};
/** @param {import('../render/renderer.js').Renderer} r @param {{ id: number, count: number }[]} meshes */
const draw = (r, meshes) => r.frame(frameParams(), meshes.map((mesh) => ({ mesh, model: mat4.identity(), tint: /** @type {[number, number, number]} */ ([1, 1, 1]) })));

/** Opens a WebGPU renderer with a cube and a capsule uploaded and a frame drawn. @param {Parameters<typeof createRenderer>[1]} extra */
async function start(extra = {}, gpuOpt = {}) {
  const g = makeGpu(gpuOpt);
  const canvas = makeCanvas();
  /** @type {string[]} */
  const events = [];
  const r = await createRenderer(canvas, {
    nav: { gpu: g.gpu },
    delay: () => Promise.resolve(),
    onLost: (why) => events.push(`lost:${why}`),
    onRecovered: (kind, was) => events.push(`recovered:${was}>${kind}`),
    onFailed: (why) => events.push(`failed:${why}`),
    ...extra,
  });
  const cube = r.upload(boxMesh());
  const cap = r.upload(capsuleMesh(0.35, 0.55));
  draw(r, [cube, cap]);
  return { g, canvas, r, cube, cap, events };
}

test('a lost WebGPU device is replaced on the same canvas and every upload comes back under the same handles', async () => {
  const { g, r, cube, cap, events } = await start();
  assert.equal(g.devices.length, 1);
  assert.equal(g.devices[0].calls.filter((c) => c === 'drawIndexed').length, 2);
  g.devices[0].lose('driver reset');
  await settle();
  assert.deepEqual(events, ['lost:driver reset', 'recovered:webgpu>webgpu']);
  assert.equal(r.state, 'ok');
  assert.equal(r.kind, 'webgpu');
  assert.equal(g.devices.length, 2, 'a new device');
  // Both meshes were uploaded again (a vertex and an index buffer each), and the old handles still draw.
  assert.equal(g.devices[1].calls.filter((c) => c === 'createBuffer').length >= 4, true);
  draw(r, [cube, cap]);
  assert.equal(g.devices[1].calls.filter((c) => c === 'drawIndexed').length, 2);
  // The lost device is not used again.
  assert.equal(g.devices[0].calls.filter((c) => c === 'drawIndexed').length, 2);
});

test('our own device.destroy() is not a loss', async () => {
  const { g, r, events } = await start();
  g.devices[0].device.destroy();
  await settle();
  assert.deepEqual(events, []);
  assert.equal(r.state, 'ok');
});

test('no new WebGPU device: it falls back to WebGL2 on a fresh canvas the page swaps in', async () => {
  const fresh = makeCanvas({ webgl2: true });
  /** @type {any[]} */
  const swaps = [];
  const { g, r, cube, cap, events, canvas } = await start({ replaceCanvas: () => fresh }, { failFrom: 1 });
  r.onCanvasChange?.((c) => swaps.push(c));
  assert.equal(r.canvas, canvas);
  g.devices[0].lose('gone');
  await settle();
  assert.deepEqual(events, ['lost:gone', 'recovered:webgpu>webgl2']);
  assert.equal(r.kind, 'webgl2');
  assert.equal(r.canvas, fresh);
  assert.deepEqual(swaps, [fresh]);
  r.resize(800, 450);
  assert.equal(fresh.width, 800);
  draw(r, [cube, cap]);
  assert.equal(fresh.calls.filter((c) => c === 'drawElements').length, 2, 'the old handles draw on the new backend');
});

test('a second WebGPU loss within 30 s goes straight to WebGL2', async () => {
  let t = 0;
  const fresh = makeCanvas({ webgl2: true });
  const { g, r, events } = await start({ now: () => t, replaceCanvas: () => fresh });
  g.devices[0].lose('first');
  await settle();
  assert.equal(r.kind, 'webgpu');
  t = 10_000;
  g.devices[1].lose('second');
  await settle();
  assert.deepEqual(events, ['lost:first', 'recovered:webgpu>webgpu', 'lost:second', 'recovered:webgpu>webgl2']);
  assert.equal(g.devices.length, 2, 'no third device');
  assert.equal(r.kind, 'webgl2');
});

test('a lost WebGL context is rebuilt when the browser restores it', async () => {
  const canvas = makeCanvas();
  /** @type {string[]} */
  const events = [];
  /** @type {(() => void)[]} */
  const timers = [];
  const r = await createRenderer(canvas, {
    nav: {},
    delay: (ms) => (ms >= 1000 ? new Promise((res) => timers.push(res)) : Promise.resolve()),
    onLost: (why) => events.push(`lost:${why}`),
    onRecovered: (kind) => events.push(`recovered:${kind}`),
    onFailed: (why) => events.push(`failed:${why}`),
  });
  const cube = r.upload(boxMesh());
  r.resize(1024, 576);
  draw(r, [cube]);
  const before = canvas.calls.filter((c) => c === 'drawElements').length;
  let prevented = false;
  canvas.handlers.webglcontextlost({ preventDefault: () => (prevented = true) });
  await settle();
  assert.equal(prevented, true, 'preventDefault lets the browser restore the context');
  assert.equal(r.state, 'recovering');
  draw(r, [cube]); // nothing is drawn while recovering
  assert.equal(canvas.calls.filter((c) => c === 'drawElements').length, before);
  canvas.handlers.webglcontextrestored({});
  await settle();
  assert.deepEqual(events, ['lost:WebGL context lost', 'recovered:webgl2']);
  assert.equal(r.state, 'ok');
  assert.equal(canvas.width, 1024, 'the size carries over');
  draw(r, [cube]);
  assert.equal(canvas.calls.filter((c) => c === 'drawElements').length, before + 1);
});

test('a WebGL context that never comes back: a fresh canvas, or a clear failure when there is none', async () => {
  for (const withFresh of [true, false]) {
    const canvas = makeCanvas();
    const fresh = makeCanvas({ webgl2: true });
    /** @type {string[]} */
    const events = [];
    /** @type {(() => void)[]} */
    const timers = [];
    const r = await createRenderer(canvas, {
      nav: {},
      delay: (ms) => (ms >= 1000 ? new Promise((res) => timers.push(res)) : Promise.resolve()),
      replaceCanvas: withFresh ? () => fresh : undefined,
      onLost: (why) => events.push(`lost:${why}`),
      onRecovered: (kind) => events.push(`recovered:${kind}`),
      onFailed: (why) => events.push('failed'),
    });
    r.upload(boxMesh());
    canvas.handlers.webglcontextlost({ preventDefault() {} });
    await settle();
    timers.forEach((t) => t()); // the 3 s wait runs out
    await settle();
    assert.deepEqual(events, withFresh ? ['lost:WebGL context lost', 'recovered:webgl2'] : ['lost:WebGL context lost', 'failed']);
    assert.equal(r.state, withFresh ? 'ok' : 'failed');
    assert.equal(r.canvas, withFresh ? fresh : canvas);
  }
});

test('three frames in a row that throw count as a loss', async () => {
  const canvas = makeCanvas();
  /** @type {string[]} */
  const events = [];
  let broken = false;
  // The first device's queue starts refusing work; the second is fine.
  const first = makeDevice({ queue: { writeBuffer() { if (broken) throw new Error('device is lost'); }, writeTexture() {}, submit() {}, copyExternalImageToTexture() {} } });
  const gpu = makeGpu({ devices: [first] });
  const r = await createRenderer(canvas, {
    nav: { gpu: gpu.gpu },
    delay: () => Promise.resolve(),
    onLost: (why) => events.push(`lost:${why}`),
    onRecovered: (kind) => events.push(`recovered:${kind}`),
  });
  const cube = r.upload(boxMesh());
  broken = false;
  draw(r, [cube]);
  broken = true;
  draw(r, [cube]);
  draw(r, [cube]);
  assert.deepEqual(events, []);
  draw(r, [cube]);
  await settle();
  broken = false;
  assert.deepEqual(events, ['lost:device is lost', 'recovered:webgpu']);
});

test('meshes uploaded while recovering are uploaded to the new backend too', async () => {
  const { g, r, cube, events } = await start();
  g.devices[0].lose('reset');
  const late = r.upload(boxMesh()); // before the microtasks that rebuild have run
  assert.equal(late.id, 3);
  await settle();
  assert.deepEqual(events, ['lost:reset', 'recovered:webgpu>webgpu']);
  draw(r, [cube, late]);
  assert.equal(g.devices[1].calls.filter((c) => c === 'drawIndexed').length, 2);
});
