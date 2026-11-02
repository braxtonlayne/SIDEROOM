import { ToastProvider } from '@sideroom/ui';
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';

const StorageBrowser = lazyNamed(() => import('./StorageBrowser'), 'StorageBrowser');
// The viewer's saved theme, like the Lounge (XC-VIS-001); a chunk of its own, so the first paint does not grow.
const LoungeThemeProvider = lazyNamed(
  () => import('../settings/user/LoungeThemeProvider'),
  'LoungeThemeProvider',
);

/** /storage/{mount}?prefix=: the bucket browser (BYOS-UI-001). The prefix travels in the search string. */
export function createStorageRoutes(root: AnyRoute) {
  const route = createRoute({
    getParentRoute: () => root,
    path: '/storage/$mountId',
    validateSearch: (s: Record<string, unknown>): { prefix: string } => ({
      prefix: typeof s.prefix === 'string' || typeof s.prefix === 'number' ? String(s.prefix) : '',
    }),
    component: function StorageRoute() {
      const { mountId } = route.useParams();
      const { prefix } = route.useSearch();
      return (
        <LoungeThemeProvider>
          <ToastProvider>
            <StorageBrowser key={mountId} mountId={mountId} prefix={prefix} />
          </ToastProvider>
        </LoungeThemeProvider>
      );
    },
  });
  return [route];
}
