import { useRouter } from '@tanstack/react-router';
import { useCallback } from 'react';

export const USER_SETTINGS_SECTIONS = [
  { id: 'account', label: 'My Account', group: 'User Settings' },
  { id: 'profile', label: 'Profiles', group: 'User Settings' },
  { id: 'privacy', label: 'Privacy & Safety', group: 'User Settings' },
  { id: 'activity', label: 'Activity Privacy', group: 'User Settings' },
  { id: 'apps', label: 'Authorized Apps', group: 'User Settings' },
  { id: 'devices', label: 'Devices', group: 'User Settings' },
  { id: 'connections', label: 'Connections', group: 'User Settings' },
  { id: 'support', label: 'Support / Patron', group: 'User Settings' },
  { id: 'appearance', label: 'Appearance', group: 'App Settings' },
  { id: 'accessibility', label: 'Accessibility', group: 'App Settings' },
  { id: 'text', label: 'Text & Images', group: 'App Settings' },
  { id: 'notifications', label: 'Notifications', group: 'App Settings' },
  { id: 'voice', label: 'Voice & Video', group: 'App Settings' },
  { id: 'keybinds', label: 'Keybinds', group: 'App Settings' },
  { id: 'language', label: 'Language', group: 'App Settings' },
  { id: 'advanced', label: 'Advanced', group: 'App Settings' },
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
