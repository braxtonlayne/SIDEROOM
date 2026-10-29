// Public surface of the client conversion pipeline. OWNER: conv-remux.
//
// conv-webcodecs plugs in here, from its own files (it never edits this one):
//   import { setDecider, registerTranscoder, setCapabilities } from '../convert';
// `decide.ts` exports the decision table (type `Decider` of ./types) and `webcodecs/index.ts` a `Transcoder`;
// the app's startup (or the first import of `convert/webcodecs`) registers them. Until then the built-in decider
// below sends everything remux can do to remux and the rest to the worker hint.
import { createConverter } from './converter';
import { createWasmEngine } from './ffmpeg/engine';
import { planRemux } from './plan';
import type {
  ConvertCapabilities,
  Converter,
  Decider,
  Decision,
  FfmpegEngine,
  ProbeResult,
  Transcoder,
} from './types';

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

/** The engine factory a Transcoder gets, for code that needs the engine itself. */
export const engineFactory: { make: () => FfmpegEngine } = { make: createWasmEngine };

const builtinDecider: Decider = (_probe, plan) =>
  plan.container
    ? { route: 'remux', reason: 'remux-possible' }
    : { route: 'worker', reason: plan.reason ?? 'unsupported' };

let decider: Decider = builtinDecider;
let transcoder: Transcoder | null = null;
let capabilities: ConvertCapabilities = { webCodecs: false };

/** conv-webcodecs: install the decision table. */
export function setDecider(d: Decider | null) {
  decider = d ?? builtinDecider;
}
/** conv-webcodecs: install the WebCodecs transcoder (route `webcodecs`). */
export function registerTranscoder(t: Transcoder | null) {
  transcoder = t;
}
/** conv-webcodecs: the detected capabilities (detect once, at first use). */
export function setCapabilities(c: ConvertCapabilities) {
  capabilities = c;
}
export const getTranscoder = () => transcoder;
export const getCapabilities = () => capabilities;

/** Decides where a probed source should go. */
export function decide(probe: ProbeResult, file: File): Decision {
  return decider(probe, planRemux(probe), capabilities, file);
}
