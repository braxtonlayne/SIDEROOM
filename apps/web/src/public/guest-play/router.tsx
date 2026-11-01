import {
  createRoute,
  useRouterState,
  type AnyRoute,
  type RouterHistory,
} from '@tanstack/react-router';
import { createSurfaceRouter } from '../surface';
import { Runner } from './Runner';

/** The guest-play surface: `/play/<token>`. Nothing else (public-surfaces.json). */
export const createGuestPlayRouter = (history?: RouterHistory) =>
  createSurfaceRouter((root) => {
    const page: AnyRoute = createRoute({
      getParentRoute: () => root,
      path: '/play/$token',
      component: function GuestRoute() {
        const l = useRouterState({ select: (s) => s.location });
        // The fragment is read from the router's location, never sent anywhere: it carries the game key.
        return (
          <Runner
            loc={{
              pathname: l.pathname,
              hash: l.hash,
              protocol: window.location.protocol,
              host: window.location.host,
            }}
          />
        );
      },
    });
    return [page];
  }, history);
