import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { AdminPage } from './AdminApp';

/** Admin routes: /admin (first section the staff member may open) and /admin/:section. */
export function createAdminRoutes(root: AnyRoute) {
  const index = createRoute({ getParentRoute: () => root, path: '/admin', component: AdminPage });
  const section = createRoute({
    getParentRoute: () => root,
    path: '/admin/$section',
    component: AdminPage,
  });
  return [index, section];
}
