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

/**
 * Does this client declare `sr_e2ee: true` when it joins (D-263)? Every call is end-to-end encrypted
 * (OPS-IMM-001: there is no build flag and no server setting for it), so the question is only whether
 * this browser can run the media transform. The e2e observer (`sr.e2e.noVoiceE2ee`) keeps declaring it:
 * it only receives and looks at ciphertext, but it is still a client that is allowed in the call.
 */
export function e2eeDeclared(mode: TransformMode = detectTransformMode()): boolean {
  return mode !== 'none';
}

/**
 * e2e builds only (VITE_SR_E2E=1): a browser test can join as a client that runs no E2EE itself
 * (localStorage `sr.e2e.noVoiceE2ee` = 1), to look at what the SFU forwards. Never honoured in a normal
 * build. The SFU never forwards that client's own media (it sends no key package), so it can only listen.
 */
export function e2eOptOut(): boolean {
  if ((import.meta.env as { VITE_SR_E2E?: string }).VITE_SR_E2E !== '1') return false;
  try {
    return globalThis.localStorage?.getItem('sr.e2e.noVoiceE2ee') === '1';
  } catch {
    return false;
  }
}
