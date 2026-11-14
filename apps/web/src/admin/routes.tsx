import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';

// The door is its own small lazy chunk: it brings the Admin API client, which the first paint must not carry.
const StaffDoor = lazyNamed(() => import('./StaffDoor'), 'StaffDoor');

/**
 * Admin routes: /admin (first section the staff member may open) and /admin/:section. Both enter through the staff
 * door (SEC-AUD-172), the one place that imports the Admin app; this file stays tiny because the router imports it
 * statically.
 */
export function createAdminRoutes(root: AnyRoute) {
  const index = createRoute({ getParentRoute: () => root, path: '/admin', component: StaffDoor });
  const section = createRoute({
    getParentRoute: () => root,
    path: '/admin/$section',
    component: StaffDoor,
  });
  return [index, section];
}
