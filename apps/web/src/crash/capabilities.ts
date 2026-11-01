import type { CapName, Engine } from './allowList';

// A minimal module with one SIMD instruction: it validates only where WebAssembly SIMD is available.
const SIMD_PROBE = new Uint8Array([
  0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253, 15,
  253, 98, 11,
]);

const has = (f: () => boolean): boolean => {
  try {
    return f();
  } catch {
    return false;
  }
};

/** The browser capability flags a crash report carries (booleans only, from the allow-list). */
export function detectCapabilities(): Partial<Record<CapName, boolean>> {
  const g = globalThis as Record<string, unknown>;
  const nav = (typeof navigator !== 'undefined' ? navigator : {}) as unknown as Record<
    string,
    unknown
  >;
  const wasm = typeof WebAssembly === 'object';
  return {
    webgpu: has(() => 'gpu' in nav && !!nav.gpu),
    wasm,
    wasm_simd: wasm && has(() => WebAssembly.validate(SIMD_PROBE)),
    shared_array_buffer: typeof g.SharedArrayBuffer === 'function',
    cross_origin_isolated: g.crossOriginIsolated === true,
    webrtc: typeof g.RTCPeerConnection === 'function',
    web_codecs: typeof g.VideoDecoder === 'function',
    service_worker: 'serviceWorker' in nav,
    offscreen_canvas: typeof g.OffscreenCanvas === 'function',
    web_audio: typeof g.AudioContext === 'function' || typeof g.webkitAudioContext === 'function',
    indexed_db: typeof g.indexedDB === 'object' && g.indexedDB !== null,
    touch: typeof nav.maxTouchPoints === 'number' && nav.maxTouchPoints > 0,
    secure_context: g.isSecureContext === true,
  };
}

/** The coarse browser engine: one of four words, never the user agent string. */
export function detectEngine(): Engine {
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  if (/firefox\//i.test(ua)) return 'firefox';
  if (/chrome\/|chromium\/|edg\//i.test(ua)) return 'chromium';
  if (/safari\//i.test(ua)) return 'webkit';
  return 'other';
}
