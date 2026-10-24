import { useRouter } from '@tanstack/react-router';
import { useCallback } from 'react';
import { t } from '../../i18n';

export const USER_SETTINGS_SECTIONS = [
  {
    id: 'account',
    get label() {
      return t('settings.userNav.account');
    },
    get group() {
      return t('settings.userNav.groupUser');
    },
  },
  {
    id: 'profile',
    get label() {
      return t('settings.userNav.profile');
    },
    get group() {
      return t('settings.userNav.groupUser');
    },
  },
  {
    id: 'privacy',
    get label() {
      return t('settings.userNav.privacy');
    },
    get group() {
      return t('settings.userNav.groupUser');
    },
  },
  {
    id: 'activity',
    get label() {
      return t('settings.userNav.activity');
    },
    get group() {
      return t('settings.userNav.groupUser');
    },
  },
  {
    id: 'apps',
    get label() {
      return t('settings.userNav.apps');
    },
    get group() {
      return t('settings.userNav.groupUser');
    },
  },
  {
    id: 'modules',
    get label() {
      return t('settings.userNav.modules');
    },
    get group() {
      return t('settings.userNav.groupUser');
    },
  },
  {
    id: 'devices',
    get label() {
      return t('settings.userNav.devices');
    },
    get group() {
      return t('settings.userNav.groupUser');
    },
  },
  {
    id: 'connections',
    get label() {
      return t('settings.userNav.connections');
    },
    get group() {
      return t('settings.userNav.groupUser');
    },
  },
  {
    id: 'invites',
    get label() {
      return t('settings.userNav.invites');
    },
    get group() {
      return t('settings.userNav.groupUser');
    },
  },
  {
    id: 'support',
    get label() {
      return t('settings.userNav.support');
    },
    get group() {
      return t('settings.userNav.groupUser');
    },
  },
  {
    id: 'appearance',
    get label() {
      return t('settings.userNav.appearance');
    },
    get group() {
      return t('settings.userNav.groupApp');
    },
  },
  {
    id: 'accessibility',
    get label() {
      return t('settings.userNav.accessibility');
    },
    get group() {
      return t('settings.userNav.groupApp');
    },
  },
  {
    id: 'text',
    get label() {
      return t('settings.userNav.text');
    },
    get group() {
      return t('settings.userNav.groupApp');
    },
  },
  {
    id: 'notifications',
    get label() {
      return t('settings.userNav.notifications');
    },
    get group() {
      return t('settings.userNav.groupApp');
    },
  },
  {
    id: 'voice',
    get label() {
      return t('settings.userNav.voice');
    },
    get group() {
      return t('settings.userNav.groupApp');
    },
  },
  {
    id: 'keybinds',
    get label() {
      return t('settings.userNav.keybinds');
    },
    get group() {
      return t('settings.userNav.groupApp');
    },
  },
  {
    id: 'language',
    get label() {
      return t('settings.userNav.language');
    },
    get group() {
      return t('settings.userNav.groupApp');
    },
  },
  {
    id: 'streamer',
    get label() {
      return t('settings.userNav.streamer');
    },
    get group() {
      return t('settings.userNav.groupApp');
    },
  },
  {
    id: 'advanced',
    get label() {
      return t('settings.userNav.advanced');
    },
    get group() {
      return t('settings.userNav.groupApp');
    },
  },
] as const;

export type UserSettingsSectionId = (typeof USER_SETTINGS_SECTIONS)[number]['id'];

export const isUserSettingsSection = (v: string): v is UserSettingsSectionId =>
  USER_SETTINGS_SECTIONS.some((s) => s.id === v);

export const USER_SETTINGS_PATH = '/settings/user';
const FALLBACK = '/channels';
let returnTo: string | null = null;
/** The accessible name of the control that had focus when Settings opened (RDY-A11Y-003, flow 8). */
let openerName: string | null = null;

/** Remembers where ESC / Close should go back to (an in-app path), and which control had focus. */
export function setSettingsReturn(href: string | null) {
  returnTo = href && href.startsWith('/') && !href.startsWith(USER_SETTINGS_PATH) ? href : null;
  openerName =
    (typeof document !== 'undefined' &&
      (document.activeElement as HTMLElement | null)?.getAttribute?.('aria-label')) ||
    null;
}

export function takeSettingsReturn(): string {
  const href = returnTo ?? FALLBACK;
  returnTo = null;
  const name = openerName;
  openerName = null;
  restoreFocusAfterNavigation(name);
  return href;
}

/**
 * Settings is a separate full-screen route, so the page that opened it is gone when it closes and focus falls to
 * <body>. Once the page is back, focus the control that opened Settings (by its accessible name), or else the
 * page's main region. Gives up quietly after about a second.
 */
export function restoreFocusAfterNavigation(name: string | null, tries = 0): void {
  if (typeof document === 'undefined' || typeof requestAnimationFrame === 'undefined') return;
  const opener = name
    ? Array.from(document.querySelectorAll<HTMLElement>('[aria-label]')).find(
        (el) => el.getAttribute('aria-label') === name && !el.closest('[role="dialog"]'),
      )
    : undefined;
  // While the URL is still Settings, <main> is Settings' own: keep waiting for the page we came from.
  const left = !location.pathname.startsWith(USER_SETTINGS_PATH);
  const target = opener ?? (left && tries >= 6 ? document.querySelector<HTMLElement>('main') : null);
  if (target) {
    if (!opener && !target.hasAttribute('tabindex')) target.setAttribute('tabindex', '-1');
    target.focus({ preventScroll: true });
    return;
  }
  if (tries < 90) requestAnimationFrame(() => restoreFocusAfterNavigation(name, tries + 1));
}

/** Opens the full-screen User Settings, remembering the current location so ESC returns to it. */
export function useOpenUserSettings() {
  const router = useRouter();
  return useCallback(
    (section: UserSettingsSectionId = 'account') => {
      setSettingsReturn(router.state.location.href);
      void router.navigate({ to: '/settings/user/$section', params: { section } });
    },
    [router],
  );
}
