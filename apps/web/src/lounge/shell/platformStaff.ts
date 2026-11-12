/**
 * Whether the signed-in person is platform staff (ADM-STF-011). `/admin/me` answers 200 for staff and 401/403
 * otherwise, so that answer is read once per session. A check that could not be made (offline, a 5xx, a timeout)
 * is not remembered as "not staff": the next use tries again (SEC-AUD-137a). It only decides whether the
 * "Platform Admin" entry is shown; the admin API is the security boundary.
 */
import { useEffect } from 'react';
import { create } from 'zustand';
import { ApiError, apiFetch } from '../../api/client';
import { onDispatch } from '../../store/events';

interface PlatformStaffState {
  staff: boolean;
  loaded: boolean;
  load(): Promise<void>;
}

let inflight: Promise<void> | null = null;
/** When the last check failed without an answer; a retry waits a few seconds so a broken network is not hammered. */
let failedAt = 0;
const RETRY_AFTER_MS = 3000;

export const usePlatformStaff = create<PlatformStaffState>()((set, get) => ({
  staff: false,
  loaded: false,
  load() {
    if (get().loaded) return Promise.resolve();
    if (!inflight && failedAt && Date.now() - failedAt < RETRY_AFTER_MS) return Promise.resolve();
    inflight ??= apiFetch<{ user_id: string }>('/admin/me', { retries: 0 })
      .then(() => {
        failedAt = 0;
        set({ staff: true, loaded: true });
      })
      .catch((e: unknown) => {
        // 401/403 is the server's answer "not staff"; anything else is no answer and is asked again.
        if (e instanceof ApiError && (e.status === 401 || e.status === 403)) {
          failedAt = 0;
          set({ staff: false, loaded: true });
        } else failedAt = Date.now();
      })
      .finally(() => {
        inflight = null;
      });
    return inflight;
  },
}));

/** Forget the check and its retry cooldown (sign-out, tests): the next use asks the server again. */
export function resetPlatformStaffCheck(): void {
  failedAt = 0;
  inflight = null;
  usePlatformStaff.setState({ staff: false, loaded: false });
}

/** True for platform staff; triggers the one-time check on first use. */
export function useIsPlatformStaff(): boolean {
  const staff = usePlatformStaff((s) => s.staff);
  const load = usePlatformStaff((s) => s.load);
  // On mount (each use of the menu), not on every render: a failed check is asked again the next time.
  useEffect(() => {
    void load();
  }, [load]);
  return staff;
}

/** True for a gateway `SR_STAFF_ALERT` that says the staff appointment of this account ended. */
export const isStaffRevoked = (event: string, data: unknown): boolean =>
  event === 'SR_STAFF_ALERT' && (data as { revoked?: unknown } | null)?.revoked === true;

// The server pushes the end of an appointment (ADM-STF-005): the Platform Admin entries disappear at once.
onDispatch((event, data) => {
  if (isStaffRevoked(event, data)) usePlatformStaff.setState({ staff: false, loaded: true });
});
