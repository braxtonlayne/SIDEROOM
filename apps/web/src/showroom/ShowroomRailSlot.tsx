import { lazy, Suspense } from 'react';
import { useShowroomActive } from './state';

// The sample servers live in a lazy chunk: nothing of the fixtures is fetched until the preview is switched on.
const PreviewRailItems = lazy(() => import('./PreviewRailItems'));

/** Rendered at the end of the guild rail's list. Adds the sample servers below the real ones while the preview is on. */
export function ShowroomRailSlot() {
  const active = useShowroomActive();
  if (!active) return null;
  return (
    <Suspense fallback={null}>
      <PreviewRailItems />
    </Suspense>
  );
}
