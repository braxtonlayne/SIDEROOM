// Mounted at the app root (main.tsx, beside the voice bridge). Loads the mini player code only once a Cinema item
// has been started, so none of the player ships with the entrance bundle.
import type { RouterHistory } from '@tanstack/react-router';
import { lazy, Suspense } from 'react';
import { useMiniPlayerActive } from './bridge';

const Host = lazy(() => import('./MiniPlayer').then((m) => ({ default: m.MiniPlayerHost })));

export function MiniPlayerMount({ history }: { history: RouterHistory }) {
  const active = useMiniPlayerActive((s) => s.active);
  if (!active) return null;
  return (
    <Suspense fallback={null}>
      <Host history={history} />
    </Suspense>
  );
}
