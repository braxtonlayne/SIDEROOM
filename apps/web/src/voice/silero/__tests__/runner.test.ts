import { describe, expect, it } from 'vitest';
import { CONTEXT_SAMPLES, FRAME_SAMPLES } from '../constants';
import { SileroRunner, STATE_SIZE, type SessionLike, type TensorCtor } from '../runner';

class FakeTensor {
  constructor(
    readonly type: string,
    readonly data: ArrayLike<number> & { slice?: (a: number, b: number) => unknown },
    readonly dims: readonly number[],
  ) {}
}

function setup(probs: number[], clock = { t: 0 }) {
  const calls: {
    input: Float32Array;
    state: Float32Array;
    sr: bigint;
    dims: Record<string, readonly number[]>;
  }[] = [];
  let n = 0;
  const session: SessionLike = {
    async run(feeds) {
      const f = feeds as Record<string, FakeTensor>;
      calls.push({
        input: Float32Array.from(f.input!.data as Float32Array),
        state: Float32Array.from(f.state!.data as Float32Array),
        sr: (f.sr!.data as BigInt64Array)[0]!,
        dims: { input: f.input!.dims, state: f.state!.dims, sr: f.sr!.dims },
      });
      const p = probs[n++];
      if (p === undefined) throw new Error('model failed');
      clock.t += 2;
      return {
        output: { data: [p] },
        stateN: { data: new Float32Array(STATE_SIZE).fill(n) },
      };
    },
  };
  const runner = new SileroRunner(session, FakeTensor as unknown as TensorCtor, () => clock.t);
  return { runner, calls };
}

const frame = (v: number) => new Float32Array(FRAME_SAMPLES).fill(v);

describe('SileroRunner (D-1050)', () => {
  it('feeds [1, 64 + 512] float32, the zero state [2, 1, 128] and the int64 rate 16000', async () => {
    const { runner, calls } = setup([0.9]);
    const r = await runner.process(frame(0.1));
    expect(r.prob).toBe(0.9);
    const c = calls[0]!;
    expect(c.dims.input).toEqual([1, CONTEXT_SAMPLES + FRAME_SAMPLES]);
    expect(c.dims.state).toEqual([2, 1, 128]);
    expect(c.dims.sr).toEqual([]);
    expect(c.sr).toBe(16000n);
    expect(c.state.every((v) => v === 0)).toBe(true);
    expect(c.input.slice(0, CONTEXT_SAMPLES).every((v) => v === 0)).toBe(true);
    expect(c.input[CONTEXT_SAMPLES]).toBeCloseTo(0.1, 6);
  });

  it('carries the returned state and the last 64 samples of the step before', async () => {
    const { runner, calls } = setup([0.1, 0.2]);
    const a = frame(0.1);
    a[FRAME_SAMPLES - 1] = 0.7;
    await runner.process(a);
    await runner.process(frame(0.3));
    const second = calls[1]!;
    expect(second.state[0]).toBe(1); // stateN of the first call
    expect(second.input[CONTEXT_SAMPLES - 1]).toBeCloseTo(0.7, 6);
    expect(second.input[0]).toBeCloseTo(0.1, 6);
    expect(second.input[CONTEXT_SAMPLES]).toBeCloseTo(0.3, 6);
  });

  it('runs frames one at a time in order and keeps the mean and worst time', async () => {
    const { runner } = setup([0.1, 0.2, 0.3]);
    const out = await Promise.all([
      runner.process(frame(0)),
      runner.process(frame(0)),
      runner.process(frame(0)),
    ]);
    expect(out.map((o) => o.prob)).toEqual([0.1, 0.2, 0.3]);
    expect(runner.stats).toEqual({ meanMs: 2, maxMs: 2 });
  });

  it('rejects when the model fails, and the next frame still runs', async () => {
    const { runner } = setup([0.4]);
    await runner.process(frame(0));
    await expect(runner.process(frame(0))).rejects.toThrow();
    await expect(runner.process(frame(0))).rejects.toThrow();
  });

  it('reset zeroes the state and the context', async () => {
    const { runner, calls } = setup([0.5, 0.5]);
    await runner.process(frame(0.9));
    runner.reset();
    await runner.process(frame(0.2));
    expect(calls[1]!.state.every((v) => v === 0)).toBe(true);
    expect(calls[1]!.input[0]).toBe(0);
  });
});
