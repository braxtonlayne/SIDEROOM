import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';

const StorageBrowser = lazyNamed(() => import('./StorageBrowser'), 'StorageBrowser');

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
        <ThemeProvider section="lounge">
          <ToastProvider>
            <StorageBrowser key={mountId} mountId={mountId} prefix={prefix} />
          </ToastProvider>
        </ThemeProvider>
      );
    },
  });
  return [route];
}
