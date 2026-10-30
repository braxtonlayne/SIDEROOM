import { createRoute, type AnyRoute, type RouterHistory } from '@tanstack/react-router';
import { Landing } from '../../routes/entrance/Landing';
import { createSurfaceRouter } from '../surface';

/** The landing surface: `/` and `/join/$code`. Nothing else (public-surfaces.json). */
export const createLandingRouter = (history?: RouterHistory) =>
  createSurfaceRouter((root, route) => {
    const join: AnyRoute = createRoute({
      getParentRoute: () => root,
      path: '/join/$code',
      component: function Join() {
        const { code } = join.useParams() as { code: string };
        return <Landing initialCode={code} />;
      },
    });
    return [route('/', () => <Landing />), join];
  }, history);
