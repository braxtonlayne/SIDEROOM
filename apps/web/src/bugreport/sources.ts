import { deliveredFingerprints, buildCommit } from '../crash/report';
import { detectCapabilities, detectEngine } from '../crash/capabilities';
import { currentRouteName } from '../crash/routeName';
import { getLocale } from '../i18n';
import { applyPrefs, usePrefsStore } from '../settings/user/prefs';
import { getGateway, useStore } from '../store';
import { useVoiceSession, isInVoice } from '../voice/session';
import type { BundleSources } from './bundle';
import type { BugSwState } from './contract';

/** The service worker's state, read once (it is async, so the dialog reads it before it builds the bundle). */
export async function readServiceWorkerState(): Promise<BugSwState> {
  try {
    const reg = await navigator.serviceWorker?.getRegistration();
    if (!reg) return 'none';
    if (reg.installing) return 'installing';
    if (reg.waiting) return 'waiting';
    if (reg.active) return 'active';
  } catch {
    /* a browser without one, or a blocked read: reported as none */
  }
  return 'none';
}

/** The applied theme id (the saved choice, with "sync with computer" resolved). */
function appliedThemeId(): string {
  const dark =
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-color-scheme: dark)').matches;
  return applyPrefs(usePrefsStore.getState().prefs, dark).userGlobal;
}

/**
 * The live readers `buildBundle` is given. Every one returns what it has and the bundle keeps only what has the right
 * shape; nothing here reads a message, a URL, an id or a token.
 */
export function liveSources(
  router: Parameters<typeof currentRouteName>[0],
  sw: BugSwState,
): BundleSources {
  return {
    commit: () => buildCommit(),
    route: () => currentRouteName(router),
    viewport: () => ({
      w: window.innerWidth,
      h: window.innerHeight,
      dpr: window.devicePixelRatio,
    }),
    caps: () => detectCapabilities(),
    engine: () => detectEngine(),
    locale: () => getLocale(),
    theme: appliedThemeId,
    online: () => navigator.onLine,
    // The status lives in the store; the client keeps the reconnect count and the last heartbeat round trip (D-665).
    gateway: () => {
      const gw = getGateway();
      return {
        status: useStore.getState().connection,
        ...(gw ? { reconnects: gw.reconnects } : {}),
        ...(gw?.heartbeatMs != null ? { heartbeat_ms: gw.heartbeatMs } : {}),
      };
    },
    sw: () => sw,
    voiceConnected: () => isInVoice(useVoiceSession.getState()),
    resources: () => performance.getEntriesByType('resource'),
    perfNow: () => performance.now(),
    origin: () => location.origin,
    crashes: () => deliveredFingerprints(),
  };
}
