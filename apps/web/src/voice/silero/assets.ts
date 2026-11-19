// D-1049: the Silero VAD model and the runtime's wasm, as static files of the app's own output (`?url` makes the
// bundler emit each one with a hashed name and never inline it). Nothing here is fetched until vad's code uses a URL.
// Import this file only from the detector's lazy code, never from first paint or a public page.
// The runtime is `onnxruntime-web/wasm` (its bundle build holds its own glue, so no separate glue file ships); the
// plain one-thread CPU wasm is the only variant (no jsep, jspi or asyncify).
import modelUrl from '../../../vendor/silero-vad/silero_vad.onnx?url';
import wasmUrl from 'onnxruntime-web/ort-wasm-simd-threaded.wasm?url';

export const sileroAssets = { modelUrl, wasmUrl };
