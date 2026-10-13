import { lazy, Suspense, type ComponentType } from 'react';

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
    load().then((m) => ({ default: m[name] as unknown as ComponentType<object> })),
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
