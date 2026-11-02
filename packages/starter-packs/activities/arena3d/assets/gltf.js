// @ts-check
// A minimal glTF 2.0 loader: meshes (triangle primitives), the node hierarchy with matrix or TRS transforms,
// and basic materials (baseColorFactor, baseColorTexture). Buffers and images may be embedded as base64 data:
// URIs (what the bundle rules favour: a .glb is not an allowed file type, so a model ships as one .json) or be
// relative paths read through an AssetSource. Skins, morph targets, animation, cameras, lights, sparse
// accessors, KTX2/Basis and Draco are out of scope.
//
// Output meshes use the renderer's vertex layout (render/meshes.js): position, normal, colour, uv. Missing
// normals are computed per face (the low-poly look); missing colours are white.

import { FLOATS_PER_VERTEX, flat } from '../render/meshes.js';
import { mat4 } from '../render/math.js';

/** @typedef {import('../render/meshes.js').MeshData} MeshData */
/** @typedef {import('./source.js').AssetSource} AssetSource */

/**
 * @typedef {object} Material
 * @property {string} name
 * @property {[number, number, number, number]} baseColor   linear RGBA factor
 * @property {number | null} baseColorImage                 index into GltfModel.images, or null
 * @property {boolean} doubleSided
 */

/**
 * @typedef {object} Primitive
 * @property {MeshData} mesh
 * @property {number} material   index into GltfModel.materials (-1: the default material)
 */

/**
 * @typedef {object} GltfModel
 * @property {{ name: string, primitives: Primitive[] }[]} meshes
 * @property {{ name: string, mesh: number | null, children: number[], local: Float32Array }[]} nodes
 * @property {number[]} roots          the default scene's root nodes
 * @property {Material[]} materials
 * @property {(ImageBitmap | null)[]} images   decoded images (null where decoding was skipped or failed)
 */

const COMPONENT = /** @type {Record<number, { size: number, read: (v: DataView, o: number) => number, norm: number }>} */ ({
  5120: { size: 1, read: (v, o) => v.getInt8(o), norm: 127 },
  5121: { size: 1, read: (v, o) => v.getUint8(o), norm: 255 },
  5122: { size: 2, read: (v, o) => v.getInt16(o, true), norm: 32767 },
  5123: { size: 2, read: (v, o) => v.getUint16(o, true), norm: 65535 },
  5125: { size: 4, read: (v, o) => v.getUint32(o, true), norm: 1 },
  5126: { size: 4, read: (v, o) => v.getFloat32(o, true), norm: 1 },
});
const COUNT = /** @type {Record<string, number>} */ ({ SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4, MAT4: 16 });

/** @param {string} b64 */
function decodeBase64(b64) {
  const s = atob(b64);
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
  return out;
}

/** A data: URI's bytes and mime type, or null. @param {string} uri */
export function parseDataUri(uri) {
  const m = /^data:([^;,]*)(;base64)?,(.*)$/s.exec(uri);
  if (!m) return null;
  return { mime: m[1] || 'application/octet-stream', bytes: m[2] ? decodeBase64(m[3]) : new TextEncoder().encode(decodeURIComponent(m[3])) };
}

/**
 * @param {any} json the parsed .gltf / .json document
 * @param {{ source?: AssetSource, basePath?: string,
 *   decodeImage?: ((bytes: Uint8Array, mime: string) => Promise<ImageBitmap>) | null }} [opt]
 *   source/basePath: for buffers and images given as relative paths (basePath is the model's folder);
 *   decodeImage: turns image bytes into an ImageBitmap (default: createImageBitmap on a Blob; null skips images)
 * @returns {Promise<GltfModel>}
 */
export async function loadGltf(json, opt = {}) {
  if (!json || typeof json !== 'object' || !String(json.asset?.version ?? '').startsWith('2')) throw new Error('not a glTF 2.0 document');
  const base = opt.basePath ? opt.basePath.replace(/\/?$/, '/') : '';
  /** @param {string} uri */
  const fetchUri = async (uri) => {
    const data = parseDataUri(uri);
    if (data) return data;
    if (!opt.source) throw new Error(`glTF names an external file (${uri}) but no asset source was given`);
    return { mime: '', bytes: await opt.source.bytes(base + uri) };
  };

  /** @type {Uint8Array[]} */
  const buffers = [];
  for (const b of json.buffers ?? []) {
    if (typeof b.uri !== 'string') throw new Error('a .glb binary chunk is not supported: embed the buffer as a data: URI');
    buffers.push((await fetchUri(b.uri)).bytes);
  }
  /** @param {number} i */
  const viewBytes = (i) => {
    const bv = json.bufferViews[i];
    const buf = buffers[bv.buffer];
    return { bytes: buf.subarray(bv.byteOffset ?? 0, (bv.byteOffset ?? 0) + bv.byteLength), stride: bv.byteStride ?? 0 };
  };
  /** An accessor as an array of numbers (normalised integers become 0..1 / -1..1). @param {number} i */
  const accessor = (i) => {
    const a = json.accessors[i];
    const comp = COMPONENT[a.componentType];
    const n = COUNT[a.type];
    if (!comp || !n) throw new Error(`accessor ${i}: unsupported type`);
    const out = new Float64Array(a.count * n);
    if (a.bufferView === undefined) return { data: out, n }; // all zeros, per the spec
    const { bytes, stride } = viewBytes(a.bufferView);
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const step = stride || comp.size * n;
    const start = a.byteOffset ?? 0;
    for (let e = 0; e < a.count; e++)
      for (let k = 0; k < n; k++) {
        const v = comp.read(view, start + e * step + k * comp.size);
        out[e * n + k] = a.normalized ? Math.max(v / comp.norm, -1) : v;
      }
    return { data: out, n };
  };

  /** @type {Material[]} */
  const materials = (json.materials ?? []).map((/** @type {any} */ m, /** @type {number} */ i) => {
    const pbr = m.pbrMetallicRoughness ?? {};
    const f = pbr.baseColorFactor ?? [1, 1, 1, 1];
    const tex = pbr.baseColorTexture ? json.textures?.[pbr.baseColorTexture.index] : undefined;
    return {
      name: String(m.name ?? `material ${i}`),
      baseColor: /** @type {[number, number, number, number]} */ ([f[0], f[1], f[2], f[3]]),
      baseColorImage: tex && typeof tex.source === 'number' ? tex.source : null,
      doubleSided: !!m.doubleSided,
    };
  });

  const meshes = (json.meshes ?? []).map((/** @type {any} */ m, /** @type {number} */ mi) => ({
    name: String(m.name ?? `mesh ${mi}`),
    primitives: (m.primitives ?? []).map((/** @type {any} */ p) => ({ mesh: primitiveMesh(p, accessor), material: p.material ?? -1 })),
  }));

  const nodes = (json.nodes ?? []).map((/** @type {any} */ n, /** @type {number} */ i) => ({
    name: String(n.name ?? `node ${i}`),
    mesh: typeof n.mesh === 'number' ? n.mesh : null,
    children: Array.isArray(n.children) ? n.children : [],
    local: n.matrix ? new Float32Array(n.matrix) : mat4.fromTRS(n.translation ?? [0, 0, 0], n.rotation ?? [0, 0, 0, 1], n.scale ?? [1, 1, 1]),
  }));
  const scene = json.scenes?.[json.scene ?? 0];
  const roots = scene?.nodes ?? nodes.map((/** @type {any} */ _n, /** @type {number} */ i) => i);

  const decode = opt.decodeImage === undefined
    ? (/** @type {Uint8Array} */ bytes, /** @type {string} */ mime) => createImageBitmap(new Blob([/** @type {any} */ (bytes)], { type: mime }))
    : opt.decodeImage;
  /** @type {(ImageBitmap | null)[]} */
  const images = [];
  for (const img of json.images ?? []) {
    if (!decode) {
      images.push(null);
      continue;
    }
    try {
      let got;
      if (typeof img.uri === 'string') got = await fetchUri(img.uri);
      else got = { bytes: viewBytes(img.bufferView).bytes, mime: '' };
      const mime = img.mimeType ?? got.mime;
      if (mime !== 'image/png' && mime !== 'image/jpeg') throw new Error(`image type ${mime || 'unknown'} (PNG or JPEG only; KTX2 needs a transcoder)`);
      images.push(await decode(got.bytes, mime));
    } catch (e) {
      console.warn('glTF image skipped:', e instanceof Error ? e.message : e);
      images.push(null);
    }
  }
  return { meshes, nodes, roots, materials, images };
}

/**
 * @param {any} p a glTF primitive
 * @param {(i: number) => { data: Float64Array, n: number }} accessor
 * @returns {MeshData}
 */
function primitiveMesh(p, accessor) {
  if ((p.mode ?? 4) !== 4) throw new Error('only triangle lists (mode 4) are supported');
  const pos = accessor(p.attributes.POSITION);
  const count = pos.data.length / 3;
  const nrm = p.attributes.NORMAL !== undefined ? accessor(p.attributes.NORMAL) : null;
  const col = p.attributes.COLOR_0 !== undefined ? accessor(p.attributes.COLOR_0) : null;
  const uv = p.attributes.TEXCOORD_0 !== undefined ? accessor(p.attributes.TEXCOORD_0) : null;
  const idx = p.indices !== undefined ? Array.from(accessor(p.indices).data) : Array.from({ length: count }, (_, i) => i);
  if (!nrm) {
    const positions = Array.from({ length: count }, (_, i) => [pos.data[i * 3], pos.data[i * 3 + 1], pos.data[i * 3 + 2]]);
    const uvs = uv ? Array.from({ length: count }, (_, i) => [uv.data[i * 2], uv.data[i * 2 + 1]]) : undefined;
    return flat(positions, idx, (t) => {
      if (!col) return [1, 1, 1];
      const v = idx[t * 3];
      return [col.data[v * col.n], col.data[v * col.n + 1], col.data[v * col.n + 2]];
    }, uvs);
  }
  const vertices = new Float32Array(count * FLOATS_PER_VERTEX);
  for (let i = 0; i < count; i++) {
    const o = i * FLOATS_PER_VERTEX;
    vertices[o] = pos.data[i * 3];
    vertices[o + 1] = pos.data[i * 3 + 1];
    vertices[o + 2] = pos.data[i * 3 + 2];
    vertices[o + 3] = nrm.data[i * 3];
    vertices[o + 4] = nrm.data[i * 3 + 1];
    vertices[o + 5] = nrm.data[i * 3 + 2];
    vertices[o + 6] = col ? col.data[i * col.n] : 1;
    vertices[o + 7] = col ? col.data[i * col.n + 1] : 1;
    vertices[o + 8] = col ? col.data[i * col.n + 2] : 1;
    vertices[o + 9] = uv ? uv.data[i * 2] : 0;
    vertices[o + 10] = uv ? uv.data[i * 2 + 1] : 0;
  }
  return { vertices, indices: Uint32Array.from(idx) };
}

/**
 * Every mesh primitive of the default scene with its world matrix (parent * child), depth first.
 * @param {GltfModel} model
 * @param {Float32Array} [root] a matrix to put the whole model under
 * @returns {{ node: number, primitive: Primitive, world: Float32Array }[]}
 */
export function flattenScene(model, root = mat4.identity()) {
  /** @type {{ node: number, primitive: Primitive, world: Float32Array }[]} */
  const out = [];
  /** @param {number} i @param {Float32Array} parent @param {number} depth */
  const visit = (i, parent, depth) => {
    if (depth > 64) throw new Error('glTF node hierarchy is too deep (or has a cycle)');
    const node = model.nodes[i];
    if (!node) return;
    const world = mat4.multiply(parent, node.local);
    if (node.mesh !== null) for (const primitive of model.meshes[node.mesh]?.primitives ?? []) out.push({ node: i, primitive, world });
    for (const c of node.children) visit(c, world, depth + 1);
  };
  for (const r of model.roots) visit(r, root, 0);
  return out;
}
