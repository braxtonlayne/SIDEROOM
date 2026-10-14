/**
 * Whether the signed-in person is platform staff (ADM-STF-011). `/admin/me` answers 200 for staff and 401/403
 * otherwise, so the answer is read once per session and any failure means "not staff". It only decides whether
 * the "Platform Admin" entry is shown; the admin API is the security boundary.
 */
import { create } from 'zustand';
import { apiFetch } from '../../api/client';

interface PlatformStaffState {
  staff: boolean;
  loaded: boolean;
  load(): Promise<void>;
}

let inflight: Promise<void> | null = null;

export const usePlatformStaff = create<PlatformStaffState>()((set, get) => ({
  staff: false,
  loaded: false,
  load() {
    if (get().loaded) return Promise.resolve();
    inflight ??= apiFetch<{ user_id: string }>('/admin/me', { retries: 0 })
      .then(() => set({ staff: true, loaded: true }))
      .catch(() => set({ staff: false, loaded: true }))
      .finally(() => {
        inflight = null;
      });
    return inflight;
  },
}));

/** True for platform staff; triggers the one-time check on first use. */
export function useIsPlatformStaff(): boolean {
  const staff = usePlatformStaff((s) => s.staff);
  const load = usePlatformStaff((s) => s.load);
  void load();
  return staff;
}
