import { createRoute, type AnyRoute, type RouterHistory } from '@tanstack/react-router';
import { createSurfaceRouter } from '../surface';
import { PublicProfile } from './PublicProfile';

/** The profile surface: `/~handle`. Nothing else (public-surfaces.json). */
export const createProfileRouter = (history?: RouterHistory) =>
  createSurfaceRouter((root) => {
    const page: AnyRoute = createRoute({
      getParentRoute: () => root,
      path: '/~{$handle}',
      component: function ProfileRoute() {
        const { handle } = page.useParams() as { handle: string };
        return <PublicProfile handle={handle} />;
      },
    });
    return [page];
  }, history);
