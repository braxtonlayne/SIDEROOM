import { detectTransformMode, type TransformMode } from './support';

type FlagEnv = { VITE_SR_VOICE_E2EE?: string };

/**
 * `VITE_SR_VOICE_E2EE` at build time: on unless set to `0` or `false` (D-249). The media path passed
 * in Chromium and Firefox in CI (VOX-E2E-003), so calls are end-to-end encrypted by default.
 *
 * Two switches turn it off:
 * - the server's `voice.e2ee = false` (the usual one: every client then joins without E2EE, because
 *   `VOICE_SERVER_UPDATE` says `sr_e2ee: false` and the controller runs E2EE only when it is true);
 * - this build flag, `VITE_SR_VOICE_E2EE=0`: the build never runs or declares E2EE, so a server
 *   with `voice.e2ee` on refuses its joins (D-263). Use it only together with the server switch.
 */
function buildFlagOn(env: FlagEnv): boolean {
  const v = env.VITE_SR_VOICE_E2EE?.trim().toLowerCase();
  return v !== '0' && v !== 'false';
}

/** Does this build run E2EE in calls the server marks as encrypted? */
export function e2eeFlagEnabled(env: FlagEnv = import.meta.env as FlagEnv): boolean {
  return buildFlagOn(env) && !e2eOptOut();
}

/**
 * Does this client declare `sr_e2ee: true` when it joins (D-263)? With the server's `voice.e2ee` on, a
 * join without it is refused (`e2ee_required`) and no token is issued. True when the build has the feature
 * on and the browser can run the media transform. The e2e observer (`sr.e2e.noVoiceE2ee`) keeps declaring
 * it: it only receives and looks at ciphertext, but it is still a client that is allowed in the call.
 */
export function e2eeDeclared(
  env: FlagEnv = import.meta.env as FlagEnv,
  mode: TransformMode = detectTransformMode(),
): boolean {
  return buildFlagOn(env) && mode !== 'none';
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
