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
