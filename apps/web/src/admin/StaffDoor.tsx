import { lazy, Suspense, useEffect, useState, type ComponentType } from 'react';
import { RouteFallback } from '../lib/lazyRoute';
import { getMe } from './api';

/**
 * SEC-AUD-172 (D-652): the only way into the Admin app. `/admin` asks `GET /admin/me` first and imports the Admin
 * chunk (served from `/app/staff/`, only to appointed staff) only when the answer says staff. Anyone else, and an
 * import the server refuses (an appointment that ended while the page was open), gets the app's not-found page, so a
 * member's network log never holds a refused staff file and the crash card never shows for it.
 *
 * Keep this file tiny and make it the ONLY importer of `./AdminApp`: `vite/staffChunks.ts` sorts the build by that
 * dynamic import.
 */
const NotFoundBody = lazy(() =>
  import('../app/NotFoundPage').then((m) => ({ default: m.NotFoundPage })),
);

type AdminPageComponent = ComponentType;
export interface StaffDoorProps {
  /** Tests inject these; the app uses the defaults. */
  ask?: () => Promise<unknown>;
  load?: () => Promise<AdminPageComponent>;
}

const askMe = () => getMe();
const loadAdmin = () => import('./AdminApp').then((m) => m.AdminPage as AdminPageComponent);

type State = { kind: 'wait' } | { kind: 'staff'; Page: AdminPageComponent } | { kind: 'none' };

export function StaffDoor({ ask = askMe, load = loadAdmin }: StaffDoorProps) {
  const [state, setState] = useState<State>({ kind: 'wait' });
  useEffect(() => {
    let live = true;
    ask()
      .then(() => load())
      // A function is a valid state value only when wrapped: `Page` is a component, not an updater.
      .then((Page) => live && setState({ kind: 'staff', Page }))
      .catch(() => live && setState({ kind: 'none' }));
    return () => {
      live = false;
    };
  }, [ask, load]);
  if (state.kind === 'wait') return <RouteFallback />;
  if (state.kind === 'staff') return <state.Page />;
  return (
    <Suspense fallback={<RouteFallback />}>
      <NotFoundBody />
    </Suspense>
  );
}
