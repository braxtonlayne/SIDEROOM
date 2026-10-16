/**
 * VOX-E2E-003: how this browser can run the media transform. `RTCRtpScriptTransform` (Safari,
 * Firefox, Chrome 141+) first, Chrome's legacy `createEncodedStreams` second. Without either the
 * client cannot join an E2EE call (spec section 5.3): the caller shows a clear error instead.
 */
export type TransformMode = 'script-transform' | 'encoded-streams' | 'none';

export interface TransformGlobals {
  RTCRtpScriptTransform?: unknown;
  RTCRtpSender?: { prototype: object };
}

export function detectTransformMode(
  g: TransformGlobals = globalThis as unknown as TransformGlobals,
): TransformMode {
  if (typeof g.RTCRtpScriptTransform === 'function') return 'script-transform';
  const proto = g.RTCRtpSender?.prototype;
  if (proto && 'createEncodedStreams' in proto) return 'encoded-streams';
  return 'none';
}
