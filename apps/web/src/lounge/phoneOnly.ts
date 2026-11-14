// SEC-AUD-206 (D-702): the rule for the phone glyph. Pure, so the store adapter and the tests can use it.
import type { ClientStatus, PresenceStatus } from '../store/types';

/** Online, with a phone session and no web or desktop session. A presence without `client_status` is never phone-only. */
export function phoneOnly(
  p: { status: PresenceStatus; client_status?: ClientStatus } | undefined,
): boolean {
  const c = p?.client_status;
  return p?.status === 'online' && !!c?.mobile && !c.web && !c.desktop;
}
