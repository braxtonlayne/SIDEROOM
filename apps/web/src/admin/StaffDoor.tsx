import { lazy, Suspense, useEffect, useState, type ComponentType } from 'react';
import { startStaffCatalog } from '../i18n/staffCatalog';
import { RouteFallback } from '../lib/lazyRoute';
import { getMe } from './api';

/**
 * SEC-AUD-172 (D-652): the only way into the Admin app. `/admin` asks `GET /admin/me` first and imports the Admin
 * chunk (served from `/app/staff/`, only to appointed staff) only when the answer says staff. Anyone else, and an
 * import the server refuses (an appointment that ended while the page was open), gets the app's not-found page, so a
 * member's network log never holds a refused staff file and the crash card never shows for it.
 *
 * SEC-AUD-187 (D-663): the staff message catalog (`i18n/messages/en.staff.ts`, also served only to staff) loads here too,
 * after `/admin/me` said staff and together with the Admin chunk, and the Admin app renders only once it has
 * registered. If it cannot be loaded the app opens with message keys and the load is retried in the background
 * (`i18n/staffCatalog.ts`, the rest catalog's rhythm). A member never reaches that line: `ask` has refused.
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
  /** Loads the staff catalog; settles when it is registered or has failed twice (the app then shows keys). */
  loadCatalog?: () => Promise<void>;
}

const askMe = () => getMe();
const loadAdmin = () => import('./AdminApp').then((m) => m.AdminPage as AdminPageComponent);

type State = { kind: 'wait' } | { kind: 'staff'; Page: AdminPageComponent } | { kind: 'none' };

export function StaffDoor({
  ask = askMe,
  load = loadAdmin,
  loadCatalog = startStaffCatalog,
}: StaffDoorProps) {
  const [state, setState] = useState<State>({ kind: 'wait' });
  useEffect(() => {
    let live = true;
    ask()
      .then(() => Promise.all([load(), loadCatalog()]))
      // A function is a valid state value only when wrapped: `Page` is a component, not an updater.
      .then(([Page]) => live && setState({ kind: 'staff', Page }))
      .catch(() => live && setState({ kind: 'none' }));
    return () => {
      live = false;
    };
  }, [ask, load, loadCatalog]);
  if (state.kind === 'wait') return <RouteFallback />;
  if (state.kind === 'staff') return <state.Page />;
  return (
    <Suspense fallback={<RouteFallback />}>
      <NotFoundBody />
    </Suspense>
  );
}
