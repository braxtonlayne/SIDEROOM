import { nav } from '../../lib/enterApp';
import { createRoute, redirect, useRouter, type AnyRoute } from '@tanstack/react-router';
import { useCallback } from 'react';
import { resetStore } from '../../store';
import { isUserSettingsSection, takeSettingsReturn } from './nav';
import { lazyNamed } from '../../lib/lazyRoute';

const UserSettings = lazyNamed(() => import('./UserSettings'), 'UserSettings');

/**
 * /settings/user            -> /settings/user/account
 * /settings/user/:section   full-screen User Settings; ESC / Close go back to where it was opened from
 *                           (or /channels when opened by deep link).
 */
function UserSettingsPage() {
  const router = useRouter();
  const { section } = userSection.useParams();
  const onSelect = useCallback(
    (id: string) =>
      void router.navigate({
        to: '/settings/user/$section',
        params: { section: id },
        replace: true,
      }),
    [router],
  );
  const onClose = useCallback(() => router.history.push(takeSettingsReturn()), [router]);
  const onSignedOut = useCallback(() => {
    resetStore();
    nav.assign('/login');
  }, [router]);
  if (!isUserSettingsSection(section)) return null;
  return (
    <UserSettings
      section={section}
      onSelect={(s) => onSelect(s)}
      onClose={onClose}
      onSignedOut={onSignedOut}
    />
  );
}

let userSection: ReturnType<typeof makeSection>;
function makeSection(root: AnyRoute) {
  return createRoute({
    getParentRoute: () => root,
    path: '/settings/user/$section',
    beforeLoad: ({ params }) => {
      if (!isUserSettingsSection(params.section))
        throw redirect({
          to: '/settings/user/$section',
          params: { section: 'account' },
          replace: true,
        });
    },
    component: UserSettingsPage,
  });
}

export function createUserSettingsRoutes(root: AnyRoute) {
  const index = createRoute({
    getParentRoute: () => root,
    path: '/settings/user',
    beforeLoad: () => {
      throw redirect({
        to: '/settings/user/$section',
        params: { section: 'account' },
        replace: true,
      });
    },
  });
  userSection = makeSection(root);
  return [index, userSection];
}
