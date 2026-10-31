import { useClientFlags } from '../../../moderation/reports';

/** The platform id of the switch (GSV-PAGE-001). It does nothing but show the "Coming soon" page and tile. */
export const GAME_SERVERS_FLAG = 'platform.game_servers';

/** Whether the page, its nav entry and the Activities tile show. Only an explicit `false` hides them (before the flags load they show). */
export function useGameServersOn(): boolean {
  return useClientFlags((s) => s.flags[GAME_SERVERS_FLAG] !== false);
}
