import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';

const ProfilePageRoute = lazyNamed(() => import('./ProfilePageView'), 'ProfilePageView');
const ProfileEditorRoute = lazyNamed(() => import('./ProfileEditor'), 'ProfileEditor');

/**
 * Customised profile pages (EXT-PRF):
 *   /u/:username          the page of a person
 *   /profile-page/edit    my own editor
 */
export function createProfilePageRoutes(root: AnyRoute) {
  const page = createRoute({
    getParentRoute: () => root,
    path: '/u/$username',
    component: function ProfilePageRouteComponent() {
      return <ProfilePageRoute username={page.useParams().username} />;
    },
  });
  const edit = createRoute({
    getParentRoute: () => root,
    path: '/profile-page/edit',
    component: ProfileEditorRoute,
  });
  return [page, edit];
}
