/**
 * D-1050: runs Silero VAD v5 frame by frame. Pure of the runtime (the session and the tensor constructor are
 * passed in) so it is unit-testable. Inputs and outputs are the model's own names (vendor/silero-vad/SOURCE.md):
 * `input` [1, 64 + 512] float32 (the last 64 samples of the step before, then the 512 new ones), `state`
 * [2, 1, 128] float32 (zeros to start, then the `stateN` it returned), `sr` int64 scalar; outputs `output`
 * [1, 1] (the speech probability) and `stateN`.
 */
import { CONTEXT_SAMPLES, FRAME_SAMPLES, SAMPLE_RATE } from './constants';

export interface TensorLike {
  readonly data: ArrayLike<number>;
  dispose?(): void;
}
export type TensorCtor = new (type: string, data: unknown, dims: readonly number[]) => unknown;
export interface SessionLike {
  run(feeds: Record<string, unknown>): Promise<Record<string, TensorLike | undefined>>;
}

export const STATE_SIZE = 2 * 128;

export class SileroRunner {
  private state = new Float32Array(STATE_SIZE);
  private readonly input = new Float32Array(CONTEXT_SAMPLES + FRAME_SAMPLES);
  private readonly sr = BigInt64Array.from([BigInt(SAMPLE_RATE)]);
  private chain: Promise<unknown> = Promise.resolve();
  private count = 0;
  private sumMs = 0;
  private max = 0;

  constructor(
    private readonly session: SessionLike,
    private readonly Tensor: TensorCtor,
    private readonly now: () => number = () => performance.now(),
  ) {}

  /** Mean and worst inference time so far, milliseconds. */
  get stats(): { meanMs: number; maxMs: number } {
    return { meanMs: this.count ? this.sumMs / this.count : 0, maxMs: this.max };
  }

  /** Frames are run one at a time, in order, whatever the caller does. Rejects when the model fails. */
  process(frame: Float32Array): Promise<{ prob: number; ms: number }> {
    const run = this.chain.then(() => this.run(frame));
    this.chain = run.catch(() => undefined);
    return run;
  }

  private async run(frame: Float32Array): Promise<{ prob: number; ms: number }> {
    const input = this.input;
    input.set(frame.subarray(0, FRAME_SAMPLES), CONTEXT_SAMPLES);
    const t0 = this.now();
    const out = await this.session.run({
      input: new this.Tensor('float32', input, [1, input.length]),
      state: new this.Tensor('float32', this.state, [2, 1, 128]),
      sr: new this.Tensor('int64', this.sr, []),
    });
    const ms = this.now() - t0;
    const prob = out.output?.data[0];
    const next = out.stateN?.data;
    if (typeof prob !== 'number' || !Number.isFinite(prob) || !next || next.length !== STATE_SIZE)
      throw new Error('unexpected model output');
    this.state = Float32Array.from(next);
    input.copyWithin(0, FRAME_SAMPLES, FRAME_SAMPLES + CONTEXT_SAMPLES);
    out.output?.dispose?.();
    out.stateN?.dispose?.();
    this.count++;
    this.sumMs += ms;
    if (ms > this.max) this.max = ms;
    return { prob, ms };
  }

  /** Forget the carried state and context (a new stretch of audio). */
  reset(): void {
    this.state = new Float32Array(STATE_SIZE);
    this.input.fill(0);
  }
}
