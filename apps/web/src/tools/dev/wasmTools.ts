/**
 * UTL-DEV-001: the typed face of the dice, diff and hash functions in `sr-tools-wasm` (the facade's
 * `loadTools().dev`, typed as `ToolsDevApi` in `@sideroom/wasm`).
 * Tests replace the loader with `setDevWasmForTests`.
 */

export interface DevWasmApi {
  diceRoll(expression: string, seedHi: number, seedLo: number): string;
  diceFlip(seedHi: number, seedLo: number): boolean;
  dicePickWeighted(weights: Uint32Array, seedHi: number, seedLo: number): number;
  diffText(a: string, b: string, mode: 'lines' | 'words' | 'chars'): string;
  hashMd5(data: Uint8Array): string;
  hashCrc32(data: Uint8Array): string;
}

type Loader = () => Promise<DevWasmApi>;

const real: Loader = async () => {
  const m = await import('@sideroom/wasm');
  return (await m.loadTools()).dev;
};

let loader: Loader = real;
let cached: Promise<DevWasmApi> | null = null;

export function devWasm(): Promise<DevWasmApi> {
  cached ??= loader().catch((e: unknown) => {
    cached = null;
    throw e;
  });
  return cached;
}

/** Tests only. */
export function setDevWasmForTests(next: Loader | null): void {
  loader = next ?? real;
  cached = null;
}

/** 64 random bits from the browser's CSPRNG as the two halves the wasm functions take. */
export function randomSeed(): [number, number] {
  const a = new Uint32Array(2);
  crypto.getRandomValues(a);
  return [a[0]!, a[1]!];
}
