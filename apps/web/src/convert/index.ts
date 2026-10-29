// Public surface of the client conversion pipeline (BYOS-CNV-001, BYOS-CNV-002).
//
// Two decisions exist and they are named apart: `decideRemux(probe, file)` below is the synchronous remux-or-worker
// rule of the ffmpeg.wasm path; the WebCodecs decision table is `decide(info, caps)` in `./decide.ts`, reached
// lazily through `./webcodecsBridge` (`planWithWebCodecs`). The Convert step asks the table first and falls back
// to the remux rule.
import { createConverter } from './converter';
import { createWasmEngine } from './ffmpeg/engine';
import { planRemux } from './plan';
import type { Converter, Decision, ProbeResult } from './types';

export * from './types';
export {
  convertedKey,
  subtitleKey,
  needsConversion,
  extensionOf,
  planRemux,
  planSubtitles,
} from './plan';
export { parseProbeLog } from './probe';
export { createConverter, DEFAULT_MAX_MIB } from './converter';

let converter: Converter | null = null;

/** The shared converter (one ffmpeg.wasm worker for the page). */
export function getConverter(): Converter {
  converter ??= createConverter(createWasmEngine);
  return converter;
}

/** Replace the converter (tests). */
export function setConverter(c: Converter | null) {
  converter?.dispose();
  converter = c;
}

/** The remux-or-worker rule: remux when a `-c copy` plan exists, else the worker hint with the reason. */
export function decideRemux(probe: ProbeResult): Decision {
  const plan = planRemux(probe);
  return plan.container
    ? { route: 'remux', reason: 'remux-possible' }
    : { route: 'worker', reason: plan.reason ?? 'unsupported' };
}
