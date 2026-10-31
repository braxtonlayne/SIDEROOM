/**
 * The emulator's page policy (ACT-EMU-001). A core's glue script runs from a blob: URL, which the app's
 * Content-Security-Policy allows only on documents of the emulator route group: sr-server adds `blob:` to
 * `script-src` when the document itself was loaded at `/games` (crates/sr-server/src/web.rs, `emulator_csp`). The app
 * is a single page, so a member who reaches /games by an in-app link still has the policy of the page they first
 * loaded, and the core would be refused. This probe tells the Games page whether it has to load itself again.
 */

const RELOAD_MARK = 'sr.emu.policyReload';

/** Whether this document may run a blob: script. Resolves false on a refusal, true when the probe ran. */
export function blobScriptsAllowed(timeoutMs = 3000): Promise<boolean> {
  return new Promise((resolve) => {
    let url = '';
    try {
      url = URL.createObjectURL(new Blob([';'], { type: 'text/javascript' }));
    } catch {
      resolve(false);
      return;
    }
    const s = document.createElement('script');
    let done = false;
    const finish = (ok: boolean) => {
      if (done) return;
      done = true;
      window.clearTimeout(timer);
      s.remove();
      URL.revokeObjectURL(url);
      resolve(ok);
    };
    // No answer at all is not a refusal (a refused script fires `error` at once); let the loader report it then.
    const timer = window.setTimeout(() => finish(true), timeoutMs);
    s.onload = () => finish(true);
    s.onerror = () => finish(false);
    s.src = url;
    document.head.append(s);
  });
}

/**
 * Load /games again so the server sends the emulator policy, at most once a minute (a policy that still refuses
 * after a fresh load is reported instead of looping). Returns false when it did not reload.
 */
export function reloadForEmulatorPolicy(now = Date.now()): boolean {
  let last = 0;
  try {
    last = Number(sessionStorage.getItem(RELOAD_MARK) ?? 0) || 0;
  } catch {
    return false;
  }
  if (now - last < 60_000) return false;
  try {
    sessionStorage.setItem(RELOAD_MARK, String(now));
  } catch {
    return false;
  }
  window.location.assign('/games');
  return true;
}
