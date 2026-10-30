import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';

const ProfilePageRoute = lazyNamed(() => import('./ProfilePageView'), 'ProfilePageView');
const ProfileEditorRoute = lazyNamed(() => import('./ProfileEditor'), 'ProfileEditor');

/**
 * Customised profile pages (EXT-PRF):
 *   /u/:username          the page of a person (also /~username)
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
  // `/~handle` is the same page under its short address (PRF-PRO-001). Signed out, `/~handle` is the public
  // surface (src/public/profile), a separate bundle; this alias serves signed-in members who open such a link.
  const alias = createRoute({
    getParentRoute: () => root,
    path: '/~{$username}',
    component: function ProfileAliasRouteComponent() {
      return <ProfilePageRoute username={alias.useParams().username} />;
    },
  });
  const edit = createRoute({
    getParentRoute: () => root,
    path: '/profile-page/edit',
    component: ProfileEditorRoute,
  });
  return [page, alias, edit];
}
