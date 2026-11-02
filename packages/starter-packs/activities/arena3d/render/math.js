// @ts-check
// The few vector, quaternion and matrix helpers the renderer needs, written here instead of pulling in a math
// library. Matrices are column-major Float32Array(16), as both WGSL and GLSL expect; vectors are plain arrays.
// This is render-side math (Math.sin is fine here); the simulation uses sim/dmath.js instead.

/** @typedef {Float32Array} Mat4 */
/** @typedef {[number, number, number]} Vec3 */
/** @typedef {[number, number, number, number]} Quat  x, y, z, w */

export const vec3 = {
  /** @param {Vec3} a @param {Vec3} b @returns {Vec3} */
  add: (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]],
  /** @param {Vec3} a @param {Vec3} b @returns {Vec3} */
  sub: (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]],
  /** @param {Vec3} a @param {number} s @returns {Vec3} */
  scale: (a, s) => [a[0] * s, a[1] * s, a[2] * s],
  /** @param {Vec3} a @param {Vec3} b */
  dot: (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2],
  /** @param {Vec3} a @param {Vec3} b @returns {Vec3} */
  cross: (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]],
  /** @param {Vec3} a */
  length: (a) => Math.hypot(a[0], a[1], a[2]),
  /** @param {Vec3} a @returns {Vec3} */
  normalize(a) {
    const l = Math.hypot(a[0], a[1], a[2]);
    return l > 0 ? [a[0] / l, a[1] / l, a[2] / l] : [0, 0, 0];
  },
};

export const quat = {
  /** @returns {Quat} */
  identity: () => [0, 0, 0, 1],
  /** @param {Vec3} axis unit axis @param {number} rad @returns {Quat} */
  fromAxisAngle(axis, rad) {
    const s = Math.sin(rad / 2);
    return [axis[0] * s, axis[1] * s, axis[2] * s, Math.cos(rad / 2)];
  },
  /** a then b is `multiply(b, a)`. @param {Quat} a @param {Quat} b @returns {Quat} */
  multiply(a, b) {
    const [ax, ay, az, aw] = a;
    const [bx, by, bz, bw] = b;
    return [
      aw * bx + ax * bw + ay * bz - az * by,
      aw * by - ax * bz + ay * bw + az * bx,
      aw * bz + ax * by - ay * bx + az * bw,
      aw * bw - ax * bx - ay * by - az * bz,
    ];
  },
  /** Rotate a vector. @param {Quat} q @param {Vec3} v @returns {Vec3} */
  rotate(q, v) {
    const [x, y, z, w] = q;
    const u = /** @type {Vec3} */ ([x, y, z]);
    const t = vec3.scale(vec3.cross(u, v), 2);
    return vec3.add(vec3.add(v, vec3.scale(t, w)), vec3.cross(u, t));
  },
};

export const mat4 = {
  /** @returns {Mat4} */
  identity() {
    const m = new Float32Array(16);
    m[0] = m[5] = m[10] = m[15] = 1;
    return m;
  },
  /** out = a * b. @param {Mat4} a @param {Mat4} b @param {Mat4} [out] */
  multiply(a, b, out = new Float32Array(16)) {
    const r = new Float32Array(16);
    for (let c = 0; c < 4; c++)
      for (let row = 0; row < 4; row++) {
        let s = 0;
        for (let k = 0; k < 4; k++) s += a[k * 4 + row] * b[c * 4 + k];
        r[c * 4 + row] = s;
      }
    out.set(r);
    return out;
  },
  /**
   * Translation * rotation * scale, the usual model matrix.
   * @param {ArrayLike<number>} t @param {ArrayLike<number>} q @param {ArrayLike<number>} s @param {Mat4} [out]
   */
  fromTRS(t, q, s, out = new Float32Array(16)) {
    const [x, y, z, w] = [q[0], q[1], q[2], q[3]];
    const x2 = x + x, y2 = y + y, z2 = z + z;
    const xx = x * x2, xy = x * y2, xz = x * z2, yy = y * y2, yz = y * z2, zz = z * z2;
    const wx = w * x2, wy = w * y2, wz = w * z2;
    out[0] = (1 - (yy + zz)) * s[0]; out[1] = (xy + wz) * s[0]; out[2] = (xz - wy) * s[0]; out[3] = 0;
    out[4] = (xy - wz) * s[1]; out[5] = (1 - (xx + zz)) * s[1]; out[6] = (yz + wx) * s[1]; out[7] = 0;
    out[8] = (xz + wy) * s[2]; out[9] = (yz - wx) * s[2]; out[10] = (1 - (xx + yy)) * s[2]; out[11] = 0;
    out[12] = t[0]; out[13] = t[1]; out[14] = t[2]; out[15] = 1;
    return out;
  },
  /**
   * A perspective projection. WebGPU's clip depth is 0..1 (`zeroToOne`), WebGL's is -1..1.
   * @param {number} fovY radians @param {number} aspect @param {number} near @param {number} far
   * @param {boolean} zeroToOne
   */
  perspective(fovY, aspect, near, far, zeroToOne) {
    const f = 1 / Math.tan(fovY / 2);
    const m = new Float32Array(16);
    m[0] = f / aspect;
    m[5] = f;
    m[11] = -1;
    if (zeroToOne) {
      m[10] = far / (near - far);
      m[14] = (near * far) / (near - far);
    } else {
      m[10] = (far + near) / (near - far);
      m[14] = (2 * far * near) / (near - far);
    }
    return m;
  },
  /**
   * A first-person view matrix: the inverse of translate(eye) * rotateY(yaw) * rotateX(pitch). Yaw 0 looks down
   * -Z and positive yaw turns left (counter-clockwise seen from above), matching sim/world.js.
   * @param {Vec3} eye @param {number} yaw radians @param {number} pitch radians, positive looks up
   */
  fpsView(eye, yaw, pitch) {
    const cy = Math.cos(yaw), sy = Math.sin(yaw), cp = Math.cos(pitch), sp = Math.sin(pitch);
    // Camera basis in world space.
    const right = /** @type {Vec3} */ ([cy, 0, -sy]);
    const up = /** @type {Vec3} */ ([sy * sp, cp, cy * sp]);
    const back = /** @type {Vec3} */ ([sy * cp, -sp, cy * cp]);
    const m = new Float32Array(16);
    m[0] = right[0]; m[4] = right[1]; m[8] = right[2];
    m[1] = up[0]; m[5] = up[1]; m[9] = up[2];
    m[2] = back[0]; m[6] = back[1]; m[10] = back[2];
    m[12] = -vec3.dot(right, eye);
    m[13] = -vec3.dot(up, eye);
    m[14] = -vec3.dot(back, eye);
    m[15] = 1;
    return m;
  },
  /** Apply to a point (w = 1), returning clip coordinates [x, y, z, w]. @param {Mat4} m @param {Vec3} p */
  transformPoint(m, p) {
    const out = [0, 0, 0, 0];
    for (let r = 0; r < 4; r++) out[r] = m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r];
    return out;
  },
};

/** Integer angle units (65536 per turn) to radians. @param {number} units */
export const unitsToRad = (units) => (units / 65536) * Math.PI * 2;
