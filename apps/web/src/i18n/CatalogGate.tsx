import type { ReactNode } from 'react';
import { useFullCatalogReady } from './react';

/**
 * Renders `children` only once the full English catalog is loaded. The entrance paints from the core
 * fragments alone; everything else (Lounge, Cinema, Library, Admin, Settings, global overlays) goes behind
 * this gate so a message key is never shown. The fallback is quiet and empty, like a lazy route's.
 */
export function CatalogGate({
  children,
  fallback = null,
}: {
  children: ReactNode;
  fallback?: ReactNode;
}) {
  return useFullCatalogReady() ? <>{children}</> : <>{fallback}</>;
}
