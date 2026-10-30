import { describe, expect, it } from 'vitest';
import {
  createEqChain,
  EQ_Q,
  type EqContext,
  type EqFilter,
  type EqGain,
  type EqNode,
} from './eqGraph';
import {
  EQ_FREQS,
  EQ_MAX_DB,
  EQ_PRESETS,
  EQ_PRESET_IDS,
  clampDb,
  dbToGain,
  matchPreset,
  normalizeGains,
  preampDb,
} from './presets';

class FakeNode implements EqNode {
  to: EqNode[] = [];
  disconnected = false;
  connect(d: EqNode) {
    this.to.push(d);
    return d;
  }
  disconnect() {
    this.disconnected = true;
  }
}

class FakeParam {
  value = 1;
  targets: { target: number; at: number; tc: number }[] = [];
  setTargetAtTime(target: number, at: number, tc: number) {
    this.targets.push({ target, at, tc });
    this.value = target;
  }
}

class FakeFilter extends FakeNode implements EqFilter {
  type = 'lowpass';
  frequency = new FakeParam();
  Q = new FakeParam();
  gain = new FakeParam();
}
class FakeGain extends FakeNode implements EqGain {
  gain = new FakeParam();
}

function fakeCtx(): EqContext & { filters: FakeFilter[]; gains: FakeGain[] } {
  const filters: FakeFilter[] = [];
  const gains: FakeGain[] = [];
  return {
    currentTime: 1.5,
    filters,
    gains,
    createBiquadFilter() {
      const f = new FakeFilter();
      filters.push(f);
      return f;
    },
    createGain() {
      const g = new FakeGain();
      gains.push(g);
      return g;
    },
  };
}

describe('the EQ chain', () => {
  it('is preamp, low shelf, eight peaking bands, high shelf, wired in order', () => {
    const ctx = fakeCtx();
    const chain = createEqChain(ctx);
    expect(ctx.filters).toHaveLength(10);
    expect(chain.bands.map((b) => b.type)).toEqual([
      'lowshelf',
      ...Array(8).fill('peaking'),
      'highshelf',
    ]);
    expect(chain.bands.map((b) => b.frequency.value)).toEqual([...EQ_FREQS]);
    expect(chain.bands.every((b) => b.Q.value === EQ_Q)).toBe(true);
    expect(chain.input).toBe(chain.preamp);
    expect(chain.output).toBe(chain.bands[9]);
    // input -> band 0 -> band 1 ... -> band 9, and the last is left open for the caller.
    expect((chain.input as unknown as FakeNode).to).toEqual([chain.bands[0]]);
    for (let i = 0; i < 9; i++)
      expect((chain.bands[i] as unknown as FakeNode).to).toEqual([chain.bands[i + 1]]);
    expect((chain.output as unknown as FakeNode).to).toEqual([]);
  });

  it('applies gains through a short glide and clamps every band to the range', () => {
    const chain = createEqChain(fakeCtx());
    chain.setGains([20, -20, 3, Number.NaN, 'x' as unknown as number, 0, 0, 0, 0, 5]);
    expect(chain.bands.map((b) => b.gain.value)).toEqual([
      EQ_MAX_DB,
      -EQ_MAX_DB,
      3,
      0,
      0,
      0,
      0,
      0,
      0,
      5,
    ]);
    const t = (chain.bands[0]!.gain as FakeParam).targets.at(-1)!;
    expect(t.at).toBe(1.5);
    expect(t.tc).toBeGreaterThan(0);
  });

  it('cuts the preamp by the largest boost so boosts do not clip', () => {
    const chain = createEqChain(fakeCtx());
    chain.setGains(EQ_PRESETS.bassBoost);
    expect(chain.preamp.gain.value).toBeCloseTo(dbToGain(-7), 6);
    chain.setGains(EQ_PRESETS.flat);
    expect(chain.preamp.gain.value).toBeCloseTo(1, 6);
    // Only cuts: nothing to compensate.
    chain.setGains([-3, -3, -3, -3, -3, -3, -3, -3, -3, -3]);
    expect(chain.preamp.gain.value).toBeCloseTo(1, 6);
  });

  it('a disabled EQ is flat whatever the sliders say, and enabling restores them', () => {
    const chain = createEqChain(fakeCtx());
    chain.setGains(EQ_PRESETS.rock, false);
    expect(chain.bands.every((b) => b.gain.value === 0)).toBe(true);
    expect(chain.preamp.gain.value).toBeCloseTo(1, 6);
    chain.setGains(EQ_PRESETS.rock, true);
    expect(chain.bands[0]!.gain.value).toBe(EQ_PRESETS.rock[0]);
  });

  it('falls back to a direct write when the param cannot glide', () => {
    const ctx = fakeCtx();
    ctx.createBiquadFilter = () => {
      const f = new FakeFilter();
      (f.gain as { setTargetAtTime?: unknown }).setTargetAtTime = undefined;
      return f;
    };
    const chain = createEqChain(ctx);
    chain.setGains([4, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect(chain.bands[0]!.gain.value).toBe(4);
  });

  it('disposes every node', () => {
    const chain = createEqChain(fakeCtx());
    chain.dispose();
    expect((chain.preamp as unknown as FakeNode).disconnected).toBe(true);
    expect(chain.bands.every((b) => (b as unknown as FakeNode).disconnected)).toBe(true);
  });
});

describe('presets', () => {
  it('every preset has ten bands inside the range, and flat is flat', () => {
    for (const id of EQ_PRESET_IDS) {
      expect(EQ_PRESETS[id]).toHaveLength(10);
      expect(EQ_PRESETS[id].every((v) => Math.abs(v) <= EQ_MAX_DB)).toBe(true);
    }
    expect(EQ_PRESETS.flat.every((v) => v === 0)).toBe(true);
  });

  it('are distinct, so a preset can be recognised from its gains', () => {
    for (const id of EQ_PRESET_IDS) expect(matchPreset(EQ_PRESETS[id])).toBe(id);
    expect(matchPreset([1, 0, 0, 0, 0, 0, 0, 0, 0, 0])).toBe('custom');
  });

  it('normalise bad input', () => {
    expect(normalizeGains(undefined)).toEqual(Array(10).fill(0));
    expect(normalizeGains([1, 2])).toEqual([1, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect(clampDb(99)).toBe(EQ_MAX_DB);
    expect(clampDb(-99)).toBe(-EQ_MAX_DB);
    expect(clampDb(Number.POSITIVE_INFINITY)).toBe(0);
    expect(preampDb([0, 2, 9, -4, 0, 0, 0, 0, 0, 0])).toBe(-9);
    expect(dbToGain(0)).toBe(1);
  });
});
