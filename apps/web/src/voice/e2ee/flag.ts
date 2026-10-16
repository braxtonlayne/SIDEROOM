/**
 * `voice.e2ee` on the client (D-053): off by default. The frame transform, worker, identity keys
 * and privacy-code UI are built, but the MLS key agreement (VOX-E2E-001) is not, so nothing can
 * supply real keys yet. Turning this on (VITE_SR_VOICE_E2EE=1 at build time) is for development
 * only; the server's `voice.e2ee` gates the op 101 relay separately.
 */
export function e2eeFlagEnabled(
  env: { VITE_SR_VOICE_E2EE?: string } = import.meta.env as { VITE_SR_VOICE_E2EE?: string },
): boolean {
  return env.VITE_SR_VOICE_E2EE === '1';
}
