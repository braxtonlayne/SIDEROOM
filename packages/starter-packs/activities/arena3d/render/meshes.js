// @ts-check
// Procedural low-poly meshes. Every mesh uses one vertex layout, 11 floats per vertex:
//   position (3), normal (3), colour (3), uv (2)
// and 32-bit indices. `flat()` gives every triangle its own vertices with the face normal, which is the
// faceted low-poly look; the glTF loader (assets/gltf.js) produces the same layout.

export const FLOATS_PER_VERTEX = 11;

/**
 * @typedef {object} MeshData
 * @property {Float32Array} vertices  FLOATS_PER_VERTEX floats per vertex
 * @property {Uint32Array} indices
 */

/**
 * Triangles (given by positions and indices) as a flat-shaded mesh.
 * @param {number[][]} positions
 * @param {number[]} indices  three per triangle, counter-clockwise seen from outside
 * @param {(i: number) => [number, number, number]} colorOf  colour per triangle index
 * @param {number[][]} [uvs]  per position
 * @returns {MeshData}
 */
export function flat(positions, indices, colorOf, uvs) {
  const tris = indices.length / 3;
  const vertices = new Float32Array(tris * 3 * FLOATS_PER_VERTEX);
  const out = new Uint32Array(tris * 3);
  for (let t = 0; t < tris; t++) {
    const a = positions[indices[t * 3]];
    const b = positions[indices[t * 3 + 1]];
    const c = positions[indices[t * 3 + 2]];
    const ux = b[0] - a[0], uy = b[1] - a[1], uz = b[2] - a[2];
    const vx = c[0] - a[0], vy = c[1] - a[1], vz = c[2] - a[2];
    let nx = uy * vz - uz * vy, ny = uz * vx - ux * vz, nz = ux * vy - uy * vx;
    const l = Math.hypot(nx, ny, nz) || 1;
    nx /= l; ny /= l; nz /= l;
    const col = colorOf(t);
    for (let k = 0; k < 3; k++) {
      const idx = indices[t * 3 + k];
      const p = positions[idx];
      const uv = uvs?.[idx] ?? [0, 0];
      vertices.set([p[0], p[1], p[2], nx, ny, nz, col[0], col[1], col[2], uv[0], uv[1]], (t * 3 + k) * FLOATS_PER_VERTEX);
      out[t * 3 + k] = t * 3 + k;
    }
  }
  return { vertices, indices: out };
}

/** A cube from -1 to 1 on every axis (scale it by half extents), white, with 0..1 uvs per face. @returns {MeshData} */
export function boxMesh() {
  /** @type {number[][]} */
  const positions = [];
  /** @type {number[][]} */
  const uvs = [];
  /** @type {number[]} */
  const indices = [];
  // Each face: normal axis, sign; four corners in counter-clockwise order seen from outside.
  const faces = [
    [[1, -1, 1], [1, -1, -1], [1, 1, -1], [1, 1, 1]],
    [[-1, -1, -1], [-1, -1, 1], [-1, 1, 1], [-1, 1, -1]],
    [[-1, 1, 1], [1, 1, 1], [1, 1, -1], [-1, 1, -1]],
    [[-1, -1, -1], [1, -1, -1], [1, -1, 1], [-1, -1, 1]],
    [[-1, -1, 1], [1, -1, 1], [1, 1, 1], [-1, 1, 1]],
    [[1, -1, -1], [-1, -1, -1], [-1, 1, -1], [1, 1, -1]],
  ];
  for (const f of faces) {
    const base = positions.length;
    positions.push(...f);
    uvs.push([0, 1], [1, 1], [1, 0], [0, 0]);
    indices.push(base, base + 1, base + 2, base, base + 2, base + 3);
  }
  return flat(positions, indices, () => [1, 1, 1], uvs);
}

/**
 * A low-poly capsule standing on y = 0: cylinder half height `half`, radius `radius`, `seg` sides.
 * @param {number} radius @param {number} half @param {number} [seg] @returns {MeshData}
 */
export function capsuleMesh(radius, half, seg = 8) {
  /** @type {number[][]} */
  const positions = [];
  /** @type {number[]} */
  const indices = [];
  const rings = [];
  // Latitude rings: bottom cap, cylinder, top cap (two rings per cap plus the poles).
  const capSteps = 2;
  /** Latitude angle of cap ring i (1 = nearest the equator). @param {number} i */
  const phi = (i) => (i * Math.PI) / (2 * (capSteps + 1));
  const ys = [];
  for (let i = capSteps; i >= 1; i--) ys.push({ y: radius - radius * Math.sin(phi(i)), r: radius * Math.cos(phi(i)) });
  ys.push({ y: radius, r: radius }, { y: radius + 2 * half, r: radius });
  for (let i = 1; i <= capSteps; i++) ys.push({ y: radius + 2 * half + radius * Math.sin(phi(i)), r: radius * Math.cos(phi(i)) });
  for (const { y, r } of ys) {
    const ring = [];
    for (let s = 0; s < seg; s++) {
      const a = (s / seg) * Math.PI * 2;
      ring.push(positions.length);
      positions.push([Math.cos(a) * r, y, -Math.sin(a) * r]);
    }
    rings.push(ring);
  }
  const bottom = positions.length;
  positions.push([0, 0, 0]);
  const top = positions.length;
  positions.push([0, 2 * radius + 2 * half, 0]);
  for (let k = 0; k + 1 < rings.length; k++) {
    const a = rings[k], b = rings[k + 1];
    for (let s = 0; s < seg; s++) {
      const s2 = (s + 1) % seg;
      indices.push(a[s], a[s2], b[s2], a[s], b[s2], b[s]);
    }
  }
  const first = rings[0], last = rings[rings.length - 1];
  for (let s = 0; s < seg; s++) {
    const s2 = (s + 1) % seg;
    indices.push(bottom, first[s2], first[s]);
    indices.push(top, last[s], last[s2]);
  }
  return flat(positions, indices, () => [1, 1, 1]);
}

/** Bounds of a mesh's positions (for tests and culling). @param {MeshData} m */
export function bounds(m) {
  const min = [Infinity, Infinity, Infinity];
  const max = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < m.vertices.length; i += FLOATS_PER_VERTEX)
    for (let k = 0; k < 3; k++) {
      min[k] = Math.min(min[k], m.vertices[i + k]);
      max[k] = Math.max(max[k], m.vertices[i + k]);
    }
  return { min, max };
}
