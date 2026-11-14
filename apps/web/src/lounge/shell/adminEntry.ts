/**
 * Whether the signed-in person is platform staff, for the one thing that asks: the "Platform Admin" entry of the
 * status menu (ADM-STF-011). The account's own READY carries the answer (`user.sr_platform_staff`, D-673); the
 * session store keeps it, so this is a store read and makes no request. `SR_STAFF_ALERT` with `revoked` clears
 * it, accepting an appointment offer sets it, and a new session reads it afresh. It only decides whether an
 * entry is shown: the admin API is the security boundary, and the staff door still asks `/admin/me` before it
 * loads staff code.
 */
import { useStore } from '../../store';
import { onDispatch } from '../../store/events';

/** True for platform staff (the READY flag). */
export function useIsPlatformStaff(): boolean {
  return useStore((s) => s.platformStaff);
}

/** Set by accepting an appointment offer; cleared by a revocation. */
export function setPlatformStaff(staff: boolean): void {
  if (useStore.getState().platformStaff !== staff) useStore.setState({ platformStaff: staff });
}

/** True for a gateway `SR_STAFF_ALERT` that says the staff appointment of this account ended. */
export const isStaffRevoked = (event: string, data: unknown): boolean =>
  event === 'SR_STAFF_ALERT' && (data as { revoked?: unknown } | null)?.revoked === true;

// The server pushes the end of an appointment (ADM-STF-005): the Platform Admin entries disappear at once.
onDispatch((event, data) => {
  if (isStaffRevoked(event, data)) setPlatformStaff(false);
});
