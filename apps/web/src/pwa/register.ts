import { setUpdateReady } from './state';

const HOUR = 60 * 60 * 1000;

/**
 * Register `/sw.js` in production builds only (XC-PWA-002). Under `vite dev` it does the opposite: it
 * removes any worker an earlier preview left on this origin so hot reload and e2e runs never see a cache.
 */
export function registerServiceWorker(): void {
  if (typeof navigator === 'undefined' || !('serviceWorker' in navigator)) return;

  if (!import.meta.env.PROD) {
    void navigator.serviceWorker
      .getRegistrations()
      .then((regs) => regs.forEach((r) => void r.unregister()))
      .catch(() => undefined);
    return;
  }

  const sw = navigator.serviceWorker;
  let reloading = false;
  sw.addEventListener('controllerchange', () => {
    // Only reload for an update the user accepted, never for the very first install.
    if (reloading || !updateRequested) return;
    reloading = true;
    window.location.reload();
  });
  let updateRequested = false;

  const offer = (reg: ServiceWorkerRegistration) => {
    // `controller` is null on the first ever visit: nothing to update from.
    if (!reg.waiting || !sw.controller) return;
    setUpdateReady(() => {
      updateRequested = true;
      reg.waiting?.postMessage({ type: 'SKIP_WAITING' });
    });
  };

  void sw
    .register('/sw.js', { scope: '/' })
    .then((reg) => {
      offer(reg);
      reg.addEventListener('updatefound', () => {
        const next = reg.installing;
        next?.addEventListener('statechange', () => {
          if (next.state === 'installed') offer(reg);
        });
      });
      const check = () => void reg.update().catch(() => undefined);
      setInterval(check, HOUR);
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'visible') check();
      });
    })
    .catch(() => undefined);
}
