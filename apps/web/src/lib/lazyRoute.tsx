import { lazy, Suspense, type ComponentType } from 'react';

/** The messages browsers give when a module's file is gone or stale (a new deploy, or the dev server re-bundling). */
const STALE_MODULE =
  /Failed to fetch dynamically imported module|error loading dynamically imported module|Importing a module script failed/i;
const RELOADED_AT = 'sr.lazyReloadAt';
/** At most one reload in this window, so a page whose code is really missing shows its error instead of looping. */
const RELOAD_WINDOW_MS = 10_000;

/**
 * A lazy page whose module cannot be fetched because its file moved reloads the page once, which picks up the
 * current files; any other failure, or a second one within the window, is thrown to the route's error boundary.
 */
export function reloadOnStaleModule(error: unknown, now = Date.now()): boolean {
  if (!(error instanceof Error) || !STALE_MODULE.test(error.message)) return false;
  try {
    const last = Number(sessionStorage.getItem(RELOADED_AT) ?? 0);
    if (now - last < RELOAD_WINDOW_MS) return false;
    sessionStorage.setItem(RELOADED_AT, String(now));
  } catch {
    return false;
  }
  window.location.reload();
  return true;
}

/**
 * Route-level code splitting (XC-PERF-001). `lazyNamed(() => import('./Foo'), 'Foo')` is a drop-in for a
 * named component import (same type): the module is fetched on first render of the route, behind a quiet
 * fallback. Keep entrance routes static (they are the first paint); use this for Lounge, Library, Admin and
 * Settings.
 */
export function lazyNamed<M extends object, K extends keyof M>(
  load: () => Promise<M>,
  name: K,
): M[K] {
  const Inner = lazy(() =>
    load().then(
      (m) => ({ default: m[name] as unknown as ComponentType<object> }),
      (error: unknown) => {
        // Reloading: keep the fallback up until the page goes away.
        if (reloadOnStaleModule(error)) return new Promise<never>(() => undefined);
        throw error;
      },
    ),
  );
  function LazyRoute(props: object) {
    return (
      <Suspense fallback={<RouteFallback />}>
        <Inner {...props} />
      </Suspense>
    );
  }
  LazyRoute.displayName = `Lazy(${String(name)})`;
  return LazyRoute as unknown as M[K];
}

/** Empty, announced politely. No spinner: chunks arrive in well under a second on a normal connection. */
export function RouteFallback() {
  return <div role="status" aria-busy="true" aria-live="polite" style={{ minHeight: '40vh' }} />;
}
