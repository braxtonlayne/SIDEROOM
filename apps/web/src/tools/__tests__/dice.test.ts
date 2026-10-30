import { afterEach, describe, expect, it } from 'vitest';
import { type DevWasmApi, setDevWasmForTests } from '../dev/wasmTools';
import {
  flipCoin,
  formatRollMessage,
  parseRollError,
  pickWeighted,
  rollDice,
  rollMessage,
} from '../dice/engine';
import { lootRowsToText, parseLootRows } from '../dice/lootApi';

const fake = (over: Partial<DevWasmApi> = {}): DevWasmApi => ({
  diceRoll: (expr) => {
    if (expr === 'boom') throw new Error('syntax:3');
    return JSON.stringify({
      expression: expr,
      total: 17,
      breakdown: '17',
      groups: [
        {
          notation: '2d20kh1',
          total: 17,
          dice: [
            { value: 17, dropped: false, exploded: false, success: false },
            { value: 5, dropped: true, exploded: false, success: false },
            { value: 4, dropped: false, exploded: true, success: false },
          ],
        },
      ],
    });
  },
  diceFlip: (_hi, lo) => lo % 2 === 0,
  dicePickWeighted: (w, _hi, lo) => (w.every((x) => x === 0) ? -1 : lo % w.length),
  diffText: () => '{}',
  hashMd5: () => '',
  hashCrc32: () => '',
  ...over,
});

afterEach(() => setDevWasmForTests(null));

describe('dice engine', () => {
  it('rolls through the wasm and parses the result', async () => {
    setDevWasmForTests(async () => fake());
    const out = await rollDice('2d20kh1');
    expect(out.ok && out.result.total).toBe(17);
    expect(out.ok && out.result.groups[0]!.dice).toHaveLength(3);
  });

  it('turns a thrown grammar error into a code and a position', async () => {
    setDevWasmForTests(async () => fake());
    expect(await rollDice('boom')).toEqual({ ok: false, code: 'syntax', at: 3 });
    expect(parseRollError('Error: too-many-dice')).toEqual({ code: 'too-many-dice' });
    expect(parseRollError('RuntimeError: unreachable')).toEqual({ code: 'unavailable' });
  });

  it('says so when the wasm cannot load', async () => {
    setDevWasmForTests(async () => {
      throw new Error('network');
    });
    expect(await rollDice('d20')).toEqual({ ok: false, code: 'unavailable' });
  });

  it('formats the /roll message with dropped and exploded dice marked', async () => {
    setDevWasmForTests(async () => fake());
    const out = await rollDice('2d20kh1');
    if (!out.ok) throw new Error('roll failed');
    expect(formatRollMessage(out.result)).toBe('**2d20kh1** = **17** (2d20kh1: [17, ~~5~~, 4!])');
    const m = await rollMessage('');
    expect(m.ok && m.content).toContain('= **17**');
  });

  it('coin and weighted pick pass randomness through', async () => {
    setDevWasmForTests(async () => fake());
    expect(typeof (await flipCoin())).toBe('boolean');
    expect(await pickWeighted([0, 0])).toBe(-1);
    const i = await pickWeighted([1, 2, 3]);
    expect(i).toBeGreaterThanOrEqual(0);
    expect(i).toBeLessThan(3);
  });
});

describe('loot rows', () => {
  it('reads "label, weight" lines with sensible defaults', () => {
    expect(parseLootRows('Sword, 3\nShield 2\nPotion\n\n  Gold coin ; 10  \nBig 99999')).toEqual([
      { label: 'Sword', weight: 3 },
      { label: 'Shield', weight: 2 },
      { label: 'Potion', weight: 1 },
      { label: 'Gold coin', weight: 10 },
      { label: 'Big', weight: 10_000 },
    ]);
    expect(parseLootRows('')).toEqual([]);
    expect(parseLootRows('7')).toEqual([{ label: '7', weight: 1 }]);
  });

  it('round-trips through text', () => {
    const rows = [
      { label: 'A', weight: 1 },
      { label: 'B c', weight: 20 },
    ];
    expect(parseLootRows(lootRowsToText(rows))).toEqual(rows);
  });
});
