/**
 * UTL-CALC-001: the browser's handle on the Rust calculator (`sr-tools-wasm`: calc, units, percent). The raw
 * bindings return JSON text; this wraps them in typed values. Tests inject an engine with `setToolsEngine`, or
 * point the loader at the built wasm with `configureToolsWasm`.
 */
import { loadTools } from '@sideroom/wasm';

export interface CalcErrorInfo {
  /** A stable lowercase code (`divide_by_zero`); `errors.ts` maps it to a message. */
  code: string;
  /** A character index into the input. */
  pos: number;
  /** A name or digit some codes carry. */
  detail: string;
}
type Failure = { ok: false; error: CalcErrorInfo };

export type EvalResult = { ok: true; value: number; text: string } | Failure;

export interface IntViews {
  signed: string;
  unsigned: string;
  hex: string;
  oct: string;
  bin: string;
  bits: number;
}
export type IntResult = { ok: true; views: IntViews; text: string; wrapped: boolean } | Failure;
export type BaseResult = { ok: true; text: string } | Failure;

export interface UnitsListing {
  categories: { category: string; units: { id: string; symbol: string }[] }[];
}
export type ConvertResult = { ok: true; value: number; text: string; category: string } | Failure;
export type ConvertAllResult =
  { ok: true; category: string; rows: { id: string; value: number; text: string }[] } | Failure;
export type QueryResult =
  | {
      ok: true;
      value: number;
      from: string;
      to: string;
      result: number;
      text: string;
      category: string;
    }
  | Failure;

export type PercentKind = 'of' | 'whatpercent' | 'ofwhat' | 'add' | 'sub' | 'change';
export type PercentResult =
  | {
      ok: true;
      kind: PercentKind;
      a: number;
      b: number;
      value: number;
      text: string;
      /** The answer is itself a percent. */
      percent: boolean;
    }
  | Failure;

export interface ToolsEngine {
  calcEval(expr: string, degrees: boolean, ans: number | null): EvalResult;
  programmerEval(expr: string, bits: number, signed: boolean, base: number): IntResult;
  programmerConvertBase(
    text: string,
    from: number,
    to: number,
    bits: number,
    signed: boolean,
  ): BaseResult;
  unitsList(): UnitsListing;
  unitsConvert(value: number, from: string, to: string): ConvertResult;
  unitsConvertAll(value: number, from: string): ConvertAllResult;
  unitsQuery(text: string): QueryResult;
  percentPhrase(text: string): PercentResult;
  percentCompute(kind: PercentKind, a: number, b: number): PercentResult;
}

/** The raw wasm-bindgen exports this file uses. */
interface Raw {
  calcEval(expr: string, degrees: boolean, ans: number): string;
  programmerEval(expr: string, bits: number, signed: boolean, base: number): string;
  programmerConvertBase(
    text: string,
    from: number,
    to: number,
    bits: number,
    signed: boolean,
  ): string;
  unitsList(): string;
  unitsConvert(value: number, from: string, to: string): string;
  unitsConvertAll(value: number, from: string): string;
  unitsQuery(text: string): string;
  percentPhrase(text: string): string;
  percentCompute(kind: string, a: number, b: number): string;
}

function wrap(raw: Raw): ToolsEngine {
  const j = <T>(s: string) => JSON.parse(s) as T;
  return {
    calcEval: (expr, degrees, ans) => j(raw.calcEval(expr, degrees, ans ?? Number.NaN)),
    programmerEval: (expr, bits, signed, base) => j(raw.programmerEval(expr, bits, signed, base)),
    programmerConvertBase: (text, from, to, bits, signed) =>
      j(raw.programmerConvertBase(text, from, to, bits, signed)),
    unitsList: () => j(raw.unitsList()),
    unitsConvert: (value, from, to) => j(raw.unitsConvert(value, from, to)),
    unitsConvertAll: (value, from) => j(raw.unitsConvertAll(value, from)),
    unitsQuery: (text) => j(raw.unitsQuery(text)),
    percentPhrase: (text) => j(raw.percentPhrase(text)),
    percentCompute: (kind, a, b) => j(raw.percentCompute(kind, a, b)),
  };
}

let injected: ToolsEngine | Error | null = null;
let wasmSource: BufferSource | undefined;
let loading: Promise<ToolsEngine> | null = null;

/** Tests: use this engine instead of loading wasm, or make loading fail with an Error (`null` restores the loader). */
export function setToolsEngine(engine: ToolsEngine | Error | null): void {
  injected = engine;
}

/** Tests (node): hand the loader the wasm bytes instead of letting it fetch them. Call before first use. */
export function configureToolsWasm(source: BufferSource | undefined): void {
  wasmSource = source;
  loading = null;
}

export function getToolsEngine(): Promise<ToolsEngine> {
  if (injected)
    return injected instanceof Error ? Promise.reject(injected) : Promise.resolve(injected);
  loading ??= loadTools(wasmSource).then((m) => wrap(m as unknown as Raw));
  // A failed load (offline, blocked) must not be remembered forever.
  loading.catch(() => {
    loading = null;
  });
  return loading;
}
