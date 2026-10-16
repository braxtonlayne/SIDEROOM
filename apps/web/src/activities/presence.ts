// The person's own presence report over the gateway (op 3). The idle reporter and an Activity's `setActivity`
// share it: both send the whole state (status, afk, activities), and the server takes the activities of the
// most recently updated session, so neither may send a partial report that wipes the other's part.
import { Op } from '../gateway/protocol';
import { useSelfPresence } from '../lounge/shell/selfPresence';
import { getGateway } from '../store';

let afk = false;
let activities: unknown[] = [];

/** Send the current state, changing `afk` and/or the Activity's presence activity first. */
export function reportPresence(next: { afk?: boolean; activity?: unknown | null } = {}): void {
  if (next.afk !== undefined) afk = next.afk;
  if (next.activity !== undefined) activities = next.activity === null ? [] : [next.activity];
  const status = useSelfPresence.getState().status;
  getGateway()?.send(Op.PRESENCE_UPDATE, { status, afk, since: null, activities });
}

/** The activities the next report would carry (for tests). */
export function currentActivities(): unknown[] {
  return activities;
}
