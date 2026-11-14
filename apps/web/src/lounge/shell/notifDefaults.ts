/**
 * The person's own default notification level (`/users/@me/notification-defaults`; D-732), the one step of the
 * push engine's order the open app used to skip (D-727): 0 all messages, 1 only mentions, 2 nothing, `null` the
 * person chose none, `undefined` not loaded yet (the desktop decision then runs without it, as before).
 *
 * Filled once when the Lounge boots and again on `SR_NOTIFICATION_DEFAULTS_UPDATE`; Settings > Notifications writes
 * through `setNotifDefault` so a save counts at once. A failed first read leaves `undefined` and is tried again
 * at the next boot (no retry loop). Lounge code only: nothing in first paint imports this file.
 */
import { create } from 'zustand';
import { onDispatch } from '../../store/events';
import { getNotificationDefaults } from '../../settings/user/api';

export interface NotifDefaultsState {
  level: number | null | undefined;
  load(): Promise<void>;
  /** The server's answer or push: the person's default is now `level`. */
  set(level: number | null): void;
  reset(): void;
}

/** Bumped by every `set`: a read that was in flight when a newer value arrived does not overwrite it. */
let version = 0;

export const useNotifDefaults = create<NotifDefaultsState>()((set) => ({
  level: undefined,
  async load() {
    const seen = version;
    try {
      const d = await getNotificationDefaults();
      if (seen === version) set({ level: d.message_notifications ?? null });
    } catch {
      /* stays unloaded; the next boot tries again */
    }
  },
  set(level) {
    version++;
    set({ level });
  },
  reset() {
    version++;
    set({ level: undefined });
  },
}));

/** Settings calls this after it saved, so the open app follows the new level before the push event arrives. */
export const setNotifDefault = (level: number | null): void =>
  useNotifDefaults.getState().set(level);

const levelOf = (data: unknown): number | null | undefined => {
  const v = (data as { message_notifications?: unknown } | null)?.message_notifications;
  if (v === null) return null;
  return typeof v === 'number' ? v : undefined;
};

/** Load the level and follow `SR_NOTIFICATION_DEFAULTS_UPDATE`. Returns the stop function. */
export function startNotifDefaultsLive(): () => void {
  const stop = onDispatch((event, data) => {
    if (event !== 'SR_NOTIFICATION_DEFAULTS_UPDATE') return;
    const level = levelOf(data);
    if (level !== undefined) useNotifDefaults.getState().set(level);
  });
  void useNotifDefaults.getState().load();
  return () => {
    stop();
    useNotifDefaults.getState().reset();
  };
}
