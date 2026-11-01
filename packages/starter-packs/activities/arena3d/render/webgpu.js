// @ts-check
// The WebGPU backend: one render pipeline, a frame uniform buffer, one uniform buffer holding every draw's
// uniforms at 256-byte steps (bound with a dynamic offset), a depth buffer, and a bind group per texture.
// TypeScript's DOM library has no WebGPU types (and @webgpu/types is not a dependency), so GPU objects are `any`.

import { WGSL } from './shaders.js';
import { mat4 } from './math.js';
import { FLOATS_PER_VERTEX } from './meshes.js';

/** @typedef {import('./renderer.js').Renderer} Renderer */

const DRAW_STRIDE = 256;
const DRAW_FLOATS = DRAW_STRIDE / 4;
const MAX_DRAWS = 1024;
const FRAME_BYTES = 144;
const DRAW_BYTES = 96;

/**
 * @param {HTMLCanvasElement} canvas
 * @param {any} gpu navigator.gpu
 * @param {(why: string) => void} [onLost]
 * @returns {Promise<Renderer | null>}
 */
export async function createWebGPURenderer(canvas, gpu, onLost) {
  if (!gpu) return null;
  const adapter = await gpu.requestAdapter();
  if (!adapter) return null;
  const device = await adapter.requestDevice();
  const G = /** @type {any} */ (globalThis);
  const BU = G.GPUBufferUsage;
  const TU = G.GPUTextureUsage;
  const SS = G.GPUShaderStage;
  const format = gpu.getPreferredCanvasFormat();
  device.onuncapturederror = (/** @type {any} */ e) => console.error('WebGPU error:', e?.error?.message ?? e);
  device.lost?.then((/** @type {any} */ info) => onLost?.(String(info?.message ?? 'device lost')));

  // Shader and pipeline errors arrive asynchronously; catch them here and give up (the caller falls back to WebGL2)
  // before the canvas is claimed by a 'webgpu' context, which would make getContext('webgl2') return null.
  device.pushErrorScope('validation');
  const module = device.createShaderModule({ code: WGSL });
  const frameBuf = device.createBuffer({ size: 256, usage: BU.UNIFORM | BU.COPY_DST });
  const drawBuf = device.createBuffer({ size: DRAW_STRIDE * MAX_DRAWS, usage: BU.UNIFORM | BU.COPY_DST });
  const bgl0 = device.createBindGroupLayout({
    entries: [
      { binding: 0, visibility: SS.VERTEX | SS.FRAGMENT, buffer: { type: 'uniform' } },
      { binding: 1, visibility: SS.VERTEX | SS.FRAGMENT, buffer: { type: 'uniform', hasDynamicOffset: true, minBindingSize: DRAW_BYTES } },
    ],
  });
  const bgl1 = device.createBindGroupLayout({
    entries: [
      { binding: 0, visibility: SS.FRAGMENT, sampler: { type: 'filtering' } },
      { binding: 1, visibility: SS.FRAGMENT, texture: { sampleType: 'float' } },
    ],
  });
  const pipeline = device.createRenderPipeline({
    layout: device.createPipelineLayout({ bindGroupLayouts: [bgl0, bgl1] }),
    vertex: {
      module,
      entryPoint: 'vs',
      buffers: [
        {
          arrayStride: FLOATS_PER_VERTEX * 4,
          attributes: [
            { shaderLocation: 0, offset: 0, format: 'float32x3' },
            { shaderLocation: 1, offset: 12, format: 'float32x3' },
            { shaderLocation: 2, offset: 24, format: 'float32x3' },
            { shaderLocation: 3, offset: 36, format: 'float32x2' },
          ],
        },
      ],
    },
    fragment: { module, entryPoint: 'fs', targets: [{ format }] },
    primitive: { topology: 'triangle-list', cullMode: 'back', frontFace: 'ccw' },
    depthStencil: { format: 'depth24plus', depthWriteEnabled: true, depthCompare: 'less' },
  });
  const bg0 = device.createBindGroup({
    layout: bgl0,
    entries: [
      { binding: 0, resource: { buffer: frameBuf, size: FRAME_BYTES } },
      { binding: 1, resource: { buffer: drawBuf, size: DRAW_BYTES } },
    ],
  });
  const invalid = await device.popErrorScope();
  if (invalid) {
    console.warn('WebGPU pipeline refused, falling back:', invalid.message);
    device.destroy();
    return null;
  }
  const ctx = /** @type {any} */ (canvas.getContext(/** @type {any} */ ('webgpu')));
  if (!ctx) {
    device.destroy();
    return null;
  }
  ctx.configure({ device, format, alphaMode: 'opaque' });
  const sampler = device.createSampler({ magFilter: 'linear', minFilter: 'linear', addressModeU: 'repeat', addressModeV: 'repeat' });

  /** @type {Map<number, { vb: any, ib: any, count: number }>} */
  const meshes = new Map();
  /** @type {Map<number, { tex: any, bg: any }>} */
  const textures = new Map();
  let nextId = 1;

  /** @param {any} tex */
  const textureBindGroup = (tex) =>
    device.createBindGroup({ layout: bgl1, entries: [{ binding: 0, resource: sampler }, { binding: 1, resource: tex.createView() }] });
  const white = device.createTexture({ size: [1, 1], format: 'rgba8unorm', usage: TU.TEXTURE_BINDING | TU.COPY_DST });
  device.queue.writeTexture({ texture: white }, new Uint8Array([255, 255, 255, 255]), { bytesPerRow: 4 }, [1, 1]);
  const whiteBg = textureBindGroup(white);

  let depth = /** @type {any} */ (null);
  let depthW = 0;
  let depthH = 0;
  const frameData = new Float32Array(FRAME_BYTES / 4);
  const drawData = new Float32Array(DRAW_FLOATS * MAX_DRAWS);

  /** @type {Renderer} */
  const r = {
    kind: 'webgpu',
    upload(mesh) {
      const vb = device.createBuffer({ size: mesh.vertices.byteLength, usage: BU.VERTEX | BU.COPY_DST });
      device.queue.writeBuffer(vb, 0, mesh.vertices);
      const ib = device.createBuffer({ size: Math.max(4, mesh.indices.byteLength), usage: BU.INDEX | BU.COPY_DST });
      device.queue.writeBuffer(ib, 0, mesh.indices);
      const id = nextId++;
      meshes.set(id, { vb, ib, count: mesh.indices.length });
      return { id, count: mesh.indices.length };
    },
    texture(image) {
      const width = image.width;
      const height = image.height;
      // Base-colour images are sRGB; the -srgb format decodes them to linear on sampling (the shader encodes once).
      const tex = device.createTexture({ size: [width, height], format: 'rgba8unorm-srgb', usage: TU.TEXTURE_BINDING | TU.COPY_DST | TU.RENDER_ATTACHMENT });
      device.queue.copyExternalImageToTexture({ source: image }, { texture: tex }, [width, height]);
      const id = nextId++;
      textures.set(id, { tex, bg: textureBindGroup(tex) });
      return { id, width, height };
    },
    resize(width, height) {
      canvas.width = Math.max(1, Math.floor(width));
      canvas.height = Math.max(1, Math.floor(height));
    },
    frame(p, draws) {
      const w = canvas.width;
      const h = canvas.height;
      if (!depth || depthW !== w || depthH !== h) {
        depth?.destroy();
        depth = device.createTexture({ size: [w, h], format: 'depth24plus', usage: TU.RENDER_ATTACHMENT });
        depthW = w;
        depthH = h;
      }
      const proj = mat4.perspective(p.fovY, w / h, p.near, p.far, true);
      frameData.set(mat4.multiply(proj, p.view), 0);
      frameData.set([p.eye[0], p.eye[1], p.eye[2], 1], 16);
      frameData.set([p.lightDir[0], p.lightDir[1], p.lightDir[2], 0], 20);
      frameData.set([p.lightColor[0], p.lightColor[1], p.lightColor[2], 1], 24);
      frameData.set([p.ambient[0], p.ambient[1], p.ambient[2], 1], 28);
      frameData.set([p.fogColor[0], p.fogColor[1], p.fogColor[2], p.fogDensity], 32);
      device.queue.writeBuffer(frameBuf, 0, frameData);
      const n = Math.min(draws.length, MAX_DRAWS);
      for (let i = 0; i < n; i++) {
        const d = draws[i];
        const o = i * DRAW_FLOATS;
        drawData.set(d.model, o);
        drawData[o + 16] = d.tint[0];
        drawData[o + 17] = d.tint[1];
        drawData[o + 18] = d.tint[2];
        drawData[o + 19] = 1;
        drawData[o + 20] = d.grid ?? 0;
      }
      if (n > 0) device.queue.writeBuffer(drawBuf, 0, drawData, 0, n * DRAW_FLOATS);
      const encoder = device.createCommandEncoder();
      const pass = encoder.beginRenderPass({
        colorAttachments: [
          {
            view: ctx.getCurrentTexture().createView(),
            clearValue: { r: p.fogColor[0], g: p.fogColor[1], b: p.fogColor[2], a: 1 },
            loadOp: 'clear',
            storeOp: 'store',
          },
        ],
        depthStencilAttachment: { view: depth.createView(), depthClearValue: 1, depthLoadOp: 'clear', depthStoreOp: 'store' },
      });
      pass.setPipeline(pipeline);
      for (let i = 0; i < n; i++) {
        const d = draws[i];
        const m = meshes.get(d.mesh.id);
        if (!m) continue;
        pass.setBindGroup(0, bg0, [i * DRAW_STRIDE]);
        pass.setBindGroup(1, (d.texture && textures.get(d.texture.id)?.bg) || whiteBg);
        pass.setVertexBuffer(0, m.vb);
        pass.setIndexBuffer(m.ib, 'uint32');
        pass.drawIndexed(m.count);
      }
      pass.end();
      device.queue.submit([encoder.finish()]);
    },
    destroy() {
      for (const m of meshes.values()) {
        m.vb.destroy();
        m.ib.destroy();
      }
      for (const t of textures.values()) t.tex.destroy();
      depth?.destroy();
      device.destroy();
    },
  };
  return r;
}
