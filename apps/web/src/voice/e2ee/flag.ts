import { detectTransformMode, type TransformMode } from './support';

/**
 * `voice.e2ee` on the client (D-053, D-249): off by default. Everything is built (MLS session,
 * media worker, identity keys, shield), but the media path has not run in a real browser yet;
 * D-249 turns it on by default once it has. Set VITE_SR_VOICE_E2EE=1 at build time, together with
 * the server's `voice.e2ee` (which gates the op 101 relay), to use it.
 */
export function e2eeFlagEnabled(
  env: { VITE_SR_VOICE_E2EE?: string } = import.meta.env as { VITE_SR_VOICE_E2EE?: string },
): boolean {
  return env.VITE_SR_VOICE_E2EE === '1' && !e2eOptOut();
}

/**
 * Does this client declare `sr_e2ee: true` when it joins (D-263)? With the server's `voice.e2ee` on, a
 * join without it is refused (`e2ee_required`) and no token is issued. True when the build has the feature
 * on and the browser can run the media transform. The e2e observer (`sr.e2e.noVoiceE2ee`) keeps declaring
 * it: it only receives and looks at ciphertext, but it is still a client that is allowed in the call.
 */
export function e2eeDeclared(
  env: { VITE_SR_VOICE_E2EE?: string } = import.meta.env as { VITE_SR_VOICE_E2EE?: string },
  mode: TransformMode = detectTransformMode(),
): boolean {
  return env.VITE_SR_VOICE_E2EE === '1' && mode !== 'none';
}

/**
 * e2e builds only (VITE_SR_E2E=1): a browser test can join as a client without E2EE (localStorage
 * `sr.e2e.noVoiceE2ee` = 1), to look at what the SFU forwards. Never honoured in a normal build.
 */
function e2eOptOut(): boolean {
  if ((import.meta.env as { VITE_SR_E2E?: string }).VITE_SR_E2E !== '1') return false;
  try {
    return globalThis.localStorage?.getItem('sr.e2e.noVoiceE2ee') === '1';
  } catch {
    return false;
  }
}
