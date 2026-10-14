import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';

const SharePage = lazyNamed(() => import('./SharePage'), 'SharePage');

/** /share: the Web Share Target (XC-PWA-006). */
export function createPwaRoutes(root: AnyRoute) {
  return [createRoute({ getParentRoute: () => root, path: '/share', component: SharePage })];
}
