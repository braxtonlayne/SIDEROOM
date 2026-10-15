import { useEffect, useRef } from 'react';
import { isStaffRevoked } from '../lounge/shell/platformStaff';
import { onDispatch } from '../store/events';

/** Runs `onRevoked` when the server pushes the end of this account's appointment (ADM-STF-005). */
export function useStaffRevoked(onRevoked: () => void) {
  const latest = useRef(onRevoked);
  latest.current = onRevoked;
  useEffect(
    () =>
      onDispatch((event, data) => {
        if (isStaffRevoked(event, data)) latest.current();
      }),
    [],
  );
}
