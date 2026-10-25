// The platform first run (RDY-ONB-001): the account flag behind `GET /users/@me` (`first_run_done`) and
// `PUT /sr/auth/first-run`, and a tiny store so the help menu and /help can bring the welcome back at once.
import { create } from 'zustand';
import { apiFetch } from '../../api/client';

/** Remember that the first run is done (`true`) or ask for it to show again (`false`). */
export const saveFirstRunDone = (done: boolean) =>
  apiFetch<void>('/sr/auth/first-run', { method: 'PUT', json: { done }, retries: 0 });

/** `forced` opens the welcome in this tab without waiting for the next page load. */
export const useFirstRunUi = create<{ forced: boolean; show: () => void; hide: () => void }>(
  (set) => ({
    forced: false,
    show: () => set({ forced: true }),
    hide: () => set({ forced: false }),
  }),
);

/** Re-open the welcome from the help menu: stored as not done, then shown. A failed save still shows it. */
export async function reopenFirstRun(): Promise<void> {
  await saveFirstRunDone(false).catch(() => undefined);
  useFirstRunUi.getState().show();
}
