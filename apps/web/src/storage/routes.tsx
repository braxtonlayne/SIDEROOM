import { ToastProvider } from '@sideroom/ui';
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';

const StorageBrowser = lazyNamed(() => import('./StorageBrowser'), 'StorageBrowser');
// The viewer's saved theme, like the Lounge (XC-VIS-001); a chunk of its own, so the first paint does not grow.
const LoungeThemeProvider = lazyNamed(
  () => import('../settings/user/LoungeThemeProvider'),
  'LoungeThemeProvider',
);

/**
 * /storage/{mount}?prefix=&back=: the bucket browser (BYOS-UI-001). The prefix travels in the search string; `back`
 * is the settings page that opened it (XC-VIS-003 M5), used when there is no history to go back to.
 */
export function createStorageRoutes(root: AnyRoute) {
  const route = createRoute({
    getParentRoute: () => root,
    path: '/storage/$mountId',
    validateSearch: (s: Record<string, unknown>): { prefix: string; back?: string } => ({
      prefix: typeof s.prefix === 'string' || typeof s.prefix === 'number' ? String(s.prefix) : '',
      ...(typeof s.back === 'string' ? { back: s.back } : {}),
    }),
    component: function StorageRoute() {
      const { mountId } = route.useParams();
      const { prefix, back } = route.useSearch();
      return (
        <LoungeThemeProvider>
          <ToastProvider>
            <StorageBrowser key={mountId} mountId={mountId} prefix={prefix} back={back} />
          </ToastProvider>
        </LoungeThemeProvider>
      );
    },
  });
  return [route];
}
