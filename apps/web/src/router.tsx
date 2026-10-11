import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  type RouterHistory,
} from '@tanstack/react-router';
import * as P from './routes/pages';

const root = createRootRoute({ component: Outlet });
const r = (path: string, component: () => React.JSX.Element) =>
  createRoute({ getParentRoute: () => root, path, component });

const loginRoute = createRoute({
  getParentRoute: () => root,
  path: '/login',
  component: P.Login,
  validateSearch: (s: Record<string, unknown>): { next?: string } =>
    typeof s.next === 'string' ? { next: s.next } : {},
});

const routeTree = root.addChildren([
  r('/', P.Landing),
  loginRoute,
  r('/join/$code', P.Join),
  r('/invite/$code', P.Invite),
  r('/channels', P.Channels),
  r('/cinema', P.Cinema),
  r('/library', P.Library),
  r('/admin', P.Admin),
  r('/developers', P.Developers),
  r('/support', P.Support),
]);

export const createAppRouter = (history?: RouterHistory) =>
  createRouter({
    routeTree,
    history,
    defaultNotFoundComponent: () => <p>Page not found.</p>,
  });

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
