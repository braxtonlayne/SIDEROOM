/**
 * Account switches stored as nested keys of the synced settings blob (`GET/PATCH /users/@me/settings`), read
 * by the server: `staff.show_badge` (profile badges, G-25) and `notifications.platform_dm` (announcement DMs,
 * D-192). Both default to on: only an explicit `false` turns them off, the same rule the server applies.
 * Saves are merge patches; the saved blob (and `SR_USER_SETTINGS_UPDATE` from other sessions) lands in the store.
 */
import { rest, useStore } from '../../store';

export type AccountSwitch = readonly [group: string, key: string];
export const STAFF_SHOW_BADGE: AccountSwitch = ['staff', 'show_badge'];
export const PLATFORM_DM: AccountSwitch = ['notifications', 'platform_dm'];

function stored(settings: Record<string, unknown> | undefined, [group, key]: AccountSwitch) {
  const g = settings?.[group];
  return g && typeof g === 'object' ? (g as Record<string, unknown>)[key] : undefined;
}

/** The switch as the server reads it: on unless the blob holds an explicit `false`. */
export const useAccountSwitch = (sw: AccountSwitch): boolean =>
  useStore((s) => stored(s.userSettings?.settings, sw) !== false);

/** True when the blob holds an explicit `false` (the user turned it off at some point). */
export const useAccountSwitchOff = (sw: AccountSwitch): boolean =>
  useStore((s) => stored(s.userSettings?.settings, sw) === false);

/** Saves the switch and records the new blob in the store. Rejects when the save fails. */
export async function setAccountSwitch([group, key]: AccountSwitch, on: boolean): Promise<void> {
  const saved = await rest.patchUserSettings({ [group]: { [key]: on } });
  useStore.getState().applyDispatch('SR_USER_SETTINGS_UPDATE', saved);
}
