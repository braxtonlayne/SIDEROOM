import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { createRoute, type AnyRoute } from '@tanstack/react-router';
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
      return (
        <ThemeProvider section="lounge">
          <ToastProvider>
            <StudyPage />
          </ToastProvider>
        </ThemeProvider>
      );
    },
  });
  return [route];
}
