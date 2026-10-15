import type { ReactNode } from 'react';
import { useFullCatalogReady } from './react';

/**
 * Renders `children` only once the full English catalog is loaded. The entrance paints from the core
 * fragments alone; everything else (Lounge, Cinema, Library, Admin, Settings, global overlays) goes behind
 * this gate so a message key is never shown. The fallback is quiet and empty, like a lazy route's.
 *
 * `bypass` opens the gate early (entrance routes). It is a prop, not a different tree, so a route that
 * switches between bypassed and gated paths is not remounted.
 */
export function CatalogGate({
  children,
  fallback = null,
  bypass = false,
}: {
  children: ReactNode;
  fallback?: ReactNode;
  bypass?: boolean;
}) {
  const ready = useFullCatalogReady();
  return ready || bypass ? <>{children}</> : <>{fallback}</>;
}
