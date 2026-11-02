import { ToastProvider } from '@sideroom/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { useState } from 'react';
import { lazyNamed } from '../lib/lazyRoute';

const StudyPage = lazyNamed(() => import('./StudyPage'), 'StudyPage');
// The viewer's saved theme, like the Lounge (XC-VIS-001); a chunk of its own, so the first paint does not grow.
const LoungeThemeProvider = lazyNamed(
  () => import('../settings/user/LoungeThemeProvider'),
  'LoungeThemeProvider',
);

/**
 * `/study`: The Study, the shelves by source (STU-SHF-001). The navigation rail that links here belongs to the
 * navigation work (41-A); this registers the page only.
 */
export function createStudyRoutes(root: AnyRoute) {
  const route = createRoute({
    getParentRoute: () => root,
    path: '/study',
    component: function StudyRoute() {
      const [client] = useState(() => new QueryClient());
      return (
        <QueryClientProvider client={client}>
          <LoungeThemeProvider>
            <ToastProvider>
              <StudyPage />
            </ToastProvider>
          </LoungeThemeProvider>
        </QueryClientProvider>
      );
    },
  });
  return [route];
}
