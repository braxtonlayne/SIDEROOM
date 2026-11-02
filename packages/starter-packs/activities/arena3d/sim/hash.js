// @ts-check
// A small, stable 64-bit state hash: two FNV-1a lanes with different offsets over a stream of 32-bit words.
// It is not cryptographic; it only has to change when the state does and be identical everywhere when it does
// not. Feed it integers (quantised values) or float32 bit patterns.

const F32 = new Float32Array(1);
const U32 = new Uint32Array(F32.buffer);

export class StateHasher {
  constructor() {
    this.a = 0x811c9dc5;
    this.b = 0xcbf29ce4;
  }

  /** Mix one 32-bit word (signed or unsigned). @param {number} word */
  u32(word) {
    let w = word >>> 0;
    for (let i = 0; i < 4; i++) {
      const byte = w & 0xff;
      this.a = Math.imul(this.a ^ byte, 0x01000193) >>> 0;
      this.b = Math.imul(this.b ^ byte, 0x5bd1e995) >>> 0; // odd, so no bits are lost
      w >>>= 8;
    }
    return this;
  }

  /** Mix a value rounded to a grid (`scale` steps per unit). @param {number} v @param {number} scale */
  q(v, scale) {
    return this.u32(Math.round(v * scale) | 0);
  }

  /** Mix the exact float32 bit pattern of a value (what Jolt stores). @param {number} v */
  f32(v) {
    F32[0] = v;
    return this.u32(U32[0]);
  }

  /** The hash as 16 lower-case hex digits. */
  hex() {
    return this.a.toString(16).padStart(8, '0') + this.b.toString(16).padStart(8, '0');
  }
}
