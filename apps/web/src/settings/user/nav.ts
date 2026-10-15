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

/** Remembers where ESC / Close should go back to (an in-app path). */
export function setSettingsReturn(href: string | null) {
  returnTo = href && href.startsWith('/') && !href.startsWith(USER_SETTINGS_PATH) ? href : null;
}

export function takeSettingsReturn(): string {
  const href = returnTo ?? FALLBACK;
  returnTo = null;
  return href;
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
