// @ts-check
// Writes assets/models/beacon.json: the sample glTF 2.0 model the demo puts on the goal pad, made entirely here
// (procedural geometry, a generated PNG), so the starter shows the whole asset path without any third-party
// file. One .json with the buffer and the texture embedded as base64 data: URIs, because the bundle rules do not
// allow .glb. It exercises what assets/gltf.js supports: indexed and non-indexed primitives, vertex colours,
// normals or none, UVs, a node hierarchy with TRS transforms, baseColorFactor and baseColorTexture.
//   node assets/make-beacon.mjs

import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { crc32, deflateSync } from 'node:zlib';

/** A tiny RGB PNG. @param {number} w @param {number} h @param {(x: number, y: number) => [number, number, number]} px */
function png(w, h, px) {
  const raw = Buffer.alloc((w * 3 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 3 + 1)] = 0;
    for (let x = 0; x < w; x++) raw.set(px(x, y), y * (w * 3 + 1) + 1 + x * 3);
  }
  /** @param {string} type @param {Buffer} data */
  const chunk = (type, data) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body) >>> 0);
    return Buffer.concat([len, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // RGB
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

const parts = /** @type {Buffer[]} */ ([]);
let offset = 0;
const bufferViews = /** @type {any[]} */ ([]);
const accessors = /** @type {any[]} */ ([]);
/**
 * @param {Float32Array | Uint16Array} arr @param {string} type @param {number} componentType @param {number} [target]
 * @param {{ min?: number[], max?: number[] }} [mm]
 */
function add(arr, type, componentType, target, mm = {}) {
  const bytes = Buffer.from(arr.buffer, arr.byteOffset, arr.byteLength);
  const pad = (4 - (bytes.length % 4)) % 4;
  bufferViews.push({ buffer: 0, byteOffset: offset, byteLength: bytes.length, ...(target ? { target } : {}) });
  parts.push(bytes, Buffer.alloc(pad));
  offset += bytes.length + pad;
  const n = { SCALAR: 1, VEC2: 2, VEC3: 3 }[type] ?? 1;
  accessors.push({ bufferView: bufferViews.length - 1, componentType, count: arr.length / n, type, ...mm });
  return accessors.length - 1;
}

// Pedestal: an octagonal prism, indexed, vertex colours (dark base ring, bright top), no normals (flat shaded).
const SIDES = 8;
const ped = [];
const pedCol = [];
for (const [y, r, c] of /** @type {[number, number, number[]][]} */ ([[0, 0.9, [0.2, 0.25, 0.3]], [0.35, 0.75, [0.35, 0.4, 0.5]]])) {
  for (let i = 0; i < SIDES; i++) {
    const a = (i / SIDES) * Math.PI * 2;
    ped.push(Math.cos(a) * r, y, -Math.sin(a) * r);
    pedCol.push(...c);
  }
}
ped.push(0, 0.35, 0);
pedCol.push(0.5, 0.95, 0.6);
const pedIdx = [];
for (let i = 0; i < SIDES; i++) {
  const j = (i + 1) % SIDES;
  pedIdx.push(i, j, SIDES + j, i, SIDES + j, SIDES + i); // sides
  pedIdx.push(2 * SIDES, SIDES + i, SIDES + j); // top
}
const pedPos = add(new Float32Array(ped), 'VEC3', 5126, 34962, { min: [-0.9, 0, -0.9], max: [0.9, 0.35, 0.9] });
const pedColor = add(new Float32Array(pedCol), 'VEC3', 5126, 34962);
const pedIndex = add(new Uint16Array(pedIdx), 'SCALAR', 5123, 34963);

// Crystal: an octahedron, not indexed, with per-face normals and UVs for the striped texture.
const tips = [[0, 1, 0], [0, -1, 0]];
const ring = [[1, 0, 0], [0, 0, -1], [-1, 0, 0], [0, 0, 1]];
const cp = [];
const cn = [];
const cuv = [];
for (const tip of tips) {
  for (let i = 0; i < 4; i++) {
    const a = ring[i];
    const b = ring[(i + 1) % 4];
    const tri = tip[1] > 0 ? [a, b, tip] : [b, a, tip];
    const u = tri[1].map((v, k) => v - tri[0][k]);
    const v = tri[2].map((x, k) => x - tri[0][k]);
    const n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    const l = Math.hypot(...n);
    for (const p of tri) {
      cp.push(...p);
      cn.push(...n.map((x) => x / l));
    }
    cuv.push(0, 1, 1, 1, 0.5, 0);
  }
}
const crPos = add(new Float32Array(cp), 'VEC3', 5126, 34962, { min: [-1, -1, -1], max: [1, 1, 1] });
const crNrm = add(new Float32Array(cn), 'VEC3', 5126, 34962);
const crUv = add(new Float32Array(cuv), 'VEC2', 5126, 34962);

const stripes = png(16, 16, (x, y) => ((x + y) % 8 < 4 ? [120, 255, 170] : [40, 170, 110]));
const gltf = {
  asset: { version: '2.0', generator: 'SIDE ROOM arena3d assets/make-beacon.mjs' },
  scene: 0,
  scenes: [{ name: 'Beacon', nodes: [0] }],
  nodes: [
    { name: 'Beacon', children: [1, 2] },
    { name: 'Pedestal', mesh: 0 },
    { name: 'Crystal', mesh: 1, translation: [0, 1.25, 0], rotation: [0, 0.3826834, 0, 0.9238795], scale: [0.35, 0.6, 0.35] },
  ],
  meshes: [
    { name: 'Pedestal', primitives: [{ attributes: { POSITION: pedPos, COLOR_0: pedColor }, indices: pedIndex, material: 0 }] },
    { name: 'Crystal', primitives: [{ attributes: { POSITION: crPos, NORMAL: crNrm, TEXCOORD_0: crUv }, material: 1 }] },
  ],
  materials: [
    { name: 'Stone', pbrMetallicRoughness: { baseColorFactor: [1, 1, 1, 1], metallicFactor: 0, roughnessFactor: 1 } },
    { name: 'Crystal', pbrMetallicRoughness: { baseColorFactor: [0.9, 1, 0.95, 1], baseColorTexture: { index: 0 }, metallicFactor: 0, roughnessFactor: 0.4 } },
  ],
  textures: [{ source: 0, sampler: 0 }],
  samplers: [{ magFilter: 9729, minFilter: 9729 }],
  images: [{ name: 'stripes', mimeType: 'image/png', uri: `data:image/png;base64,${stripes.toString('base64')}` }],
  buffers: [{ byteLength: offset, uri: `data:application/octet-stream;base64,${Buffer.concat(parts).toString('base64')}` }],
  bufferViews,
  accessors,
};

const out = join(dirname(fileURLToPath(import.meta.url)), 'models', 'beacon.json');
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, JSON.stringify(gltf, null, 1) + '\n');
console.log(`wrote ${out} (${offset} buffer bytes, ${stripes.length} PNG bytes)`);
