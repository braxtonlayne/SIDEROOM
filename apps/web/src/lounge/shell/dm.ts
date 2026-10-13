import { useCallback } from 'react';
import { useStore } from '../../store';
import { HOME } from '../channels';
import { useLoungeGoto } from '../nav';
import { createDm, type DmObject } from './api';

/** Puts a DM the server just returned into the store so it shows up without waiting for the gateway. */
export function rememberDm(dm: DmObject) {
  useStore.getState().applyDispatch('CHANNEL_CREATE', dm);
}

/** Open (or create) the DM with one person, or a group DM with several, and go there. */
export function useOpenDm() {
  const goto = useLoungeGoto();
  return useCallback(
    async (userIds: string[]) => {
      const dm = await createDm(userIds);
      rememberDm(dm);
      goto(HOME, dm.id);
      return dm;
    },
    [goto],
  );
}
