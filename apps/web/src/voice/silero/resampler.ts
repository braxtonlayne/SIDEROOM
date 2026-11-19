/**
 * D-1050: a streaming area-average resampler to 16 kHz mono, and the framer behind the worklet.
 *
 * Each output sample is the mean of the input span it covers (fractional edges weighted). That is a box low-pass
 * filter plus decimation: it keeps the energy below 8 kHz that the detector reads and lessens (does not remove) what
 * would otherwise fold back as aliasing, which a plain linear pick would not. A detector listens to the 100 Hz to
 * 4 kHz speech band, so this is enough; it is not an audio-quality resampler and its output is never played or sent.
 * Pure and allocation-free per call (the frame buffer is handed on and replaced by the caller of `push`).
 */
export class FrameResampler {
  private readonly ratio: number;
  private acc = 0;
  private remain: number;
  private frame: Float32Array;
  private filled = 0;

  constructor(
    inputRate: number,
    outputRate: number,
    private readonly frameSamples: number,
    private readonly onFrame: (frame: Float32Array) => void,
  ) {
    this.ratio = inputRate / outputRate;
    this.remain = this.ratio;
    this.frame = new Float32Array(frameSamples);
  }

  /** Feed mono input samples; every complete frame goes to `onFrame` (which may transfer its buffer). */
  push(input: ArrayLike<number>): void {
    const n = input.length;
    for (let i = 0; i < n; i++) {
      const x = input[i]!;
      let w = 1;
      while (w > 1e-9) {
        const t = w < this.remain ? w : this.remain;
        this.acc += x * t;
        this.remain -= t;
        w -= t;
        if (this.remain <= 1e-9) {
          this.frame[this.filled++] = this.acc / this.ratio;
          this.acc = 0;
          this.remain = this.ratio;
          if (this.filled === this.frameSamples) {
            const out = this.frame;
            this.frame = new Float32Array(this.frameSamples);
            this.filled = 0;
            this.onFrame(out);
          }
        }
      }
    }
  }
}
