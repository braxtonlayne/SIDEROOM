import type { CrashFrame } from './allowList';

const V8 = /^\s*at (?:(.+?) \()?(.+?):(\d+):(\d+)\)?\s*$/;
const GECKO = /^\s*(.*?)@(.+?):(\d+):(\d+)\s*$/;

/**
 * Stack frames from an `Error.stack` string (V8 `at fn (file:1:2)` and Firefox/Safari `fn@file:1:2`), newest first.
 * The raw file names are returned unstripped on purpose: `buildCrashCard` is the one place that strips them, so
 * there is no second code path that could forget to.
 */
export function parseStack(stack: unknown): CrashFrame[] {
  if (typeof stack !== 'string') return [];
  const out: CrashFrame[] = [];
  for (const line of stack.split('\n').slice(0, 64)) {
    const m = V8.exec(line) ?? GECKO.exec(line);
    if (!m) {
      // A WebAssembly frame without a position: `at fn (wasm://wasm/hash:wasm-function[12]:0x3f)`.
      const w = /^\s*at (?:(.+?) \()?(wasm:\/\/[^\s)]+?)\)?\s*$/.exec(line);
      if (w) out.push({ file: w[2] ?? '', line: 0, col: 0, ...(w[1] ? { fn: w[1] } : {}) });
      continue;
    }
    const fn = m[1]?.trim();
    out.push({
      file: m[2] ?? '',
      line: Number(m[3]),
      col: Number(m[4]),
      ...(fn ? { fn } : {}),
    });
  }
  return out;
}
