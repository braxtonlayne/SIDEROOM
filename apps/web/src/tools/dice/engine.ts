/**
 * UTL-DEV-001: dice, coin and weighted picks, over the `sr-tools-wasm` functions. Randomness is 64 bits from the
 * browser's CSPRNG per call; the wasm does the rest deterministically from that seed.
 */
import { devWasm, randomSeed } from '../dev/wasmTools';

export interface Die {
  value: number;
  dropped: boolean;
  exploded: boolean;
  success: boolean;
}

export interface RollGroup {
  notation: string;
  total: number;
  dice: Die[];
}

export interface RollResult {
  expression: string;
  total: number;
  breakdown: string;
  groups: RollGroup[];
}

/** The grammar error codes the wasm throws, as the part before any `:`. */
export type RollErrorCode =
  | 'empty'
  | 'too-long'
  | 'syntax'
  | 'zero-dice'
  | 'too-many-dice'
  | 'bad-sides'
  | 'bad-modifier'
  | 'too-many-rolls'
  | 'too-deep'
  | 'overflow'
  | 'unavailable';

/** The catalog key of a roll error message (catalog keys have no hyphens). */
export const rollErrorKey = (code: RollErrorCode): string =>
  `tools.dice.error.${code.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase())}`;

export type RollOutcome =
  { ok: true; result: RollResult } | { ok: false; code: RollErrorCode; at?: number };

const CODES: readonly string[] = [
  'empty',
  'too-long',
  'syntax',
  'zero-dice',
  'too-many-dice',
  'bad-sides',
  'bad-modifier',
  'too-many-rolls',
  'too-deep',
  'overflow',
];

/** Parse an error thrown by the wasm (`syntax:7`, `too-many-dice`). */
export function parseRollError(message: string): { code: RollErrorCode; at?: number } {
  const [head = '', tail] = message.replace(/^Error:\s*/, '').split(':');
  if (!CODES.includes(head)) return { code: 'unavailable' };
  const at = tail !== undefined && /^\d+$/.test(tail) ? Number(tail) : undefined;
  return at === undefined ? { code: head as RollErrorCode } : { code: head as RollErrorCode, at };
}

export async function rollDice(expression: string): Promise<RollOutcome> {
  try {
    const w = await devWasm();
    const [hi, lo] = randomSeed();
    return { ok: true, result: JSON.parse(w.diceRoll(expression, hi, lo)) as RollResult };
  } catch (e) {
    return { ok: false, ...parseRollError(e instanceof Error ? e.message : String(e)) };
  }
}

/** Heads is `true`. */
export async function flipCoin(): Promise<boolean> {
  const w = await devWasm();
  const [hi, lo] = randomSeed();
  return w.diceFlip(hi, lo);
}

/** Index of a weighted pick, or `-1` when every weight is zero. */
export async function pickWeighted(weights: number[]): Promise<number> {
  const w = await devWasm();
  const [hi, lo] = randomSeed();
  return w.dicePickWeighted(Uint32Array.from(weights), hi, lo);
}

const render = (d: Die) =>
  d.dropped ? `~~${d.value}~~` : d.exploded ? `${d.value}!` : String(d.value);

/**
 * The message `/roll` posts, for example `**2d20kh1+5** = **17** (2d20kh1: [12, ~~5~~])`: the total, then each
 * group's dice (a dropped die struck through, an exploded die marked). Plain markdown, so it reads the same in
 * any client.
 */
export function formatRollMessage(r: RollResult): string {
  const detail = r.groups
    .map((g) => `${g.notation}: [${g.dice.map(render).join(', ')}]`)
    .join('  ');
  const head = `**${r.expression}** = **${r.total}**`;
  return r.groups.length === 0 ? head : `${head} (${detail})`;
}

/** What `/roll` does with its argument: roll it and give back the message, or an error code. */
export async function rollMessage(
  expression: string,
): Promise<{ ok: true; content: string } | { ok: false; code: RollErrorCode }> {
  const out = await rollDice(expression.trim() === '' ? 'd20' : expression);
  return out.ok
    ? { ok: true, content: formatRollMessage(out.result) }
    : { ok: false, code: out.code };
}
