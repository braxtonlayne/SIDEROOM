import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { useState } from 'react';
import { lazyNamed } from '../lib/lazyRoute';

const StudyPage = lazyNamed(() => import('./StudyPage'), 'StudyPage');

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
          <ThemeProvider section="lounge">
            <ToastProvider>
              <StudyPage />
            </ToastProvider>
          </ThemeProvider>
        </QueryClientProvider>
      );
    },
  });
  return [route];
}
