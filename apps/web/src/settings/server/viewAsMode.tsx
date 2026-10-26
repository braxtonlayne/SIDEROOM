// View Server As Role as a Lounge-wide mode (RDY-PAR-005, LNG-SSET-036). The dialog in Server Settings > Roles works
// out which channels the chosen roles would see (the shared `sr-permissions` engine, `viewAs.ts`) and starts this
// mode: the channel list of that server shows only those channels until the banner's "Exit preview". Nothing is
// saved or sent; the server stays the security boundary and a reload ends the preview.
import { Button } from '@sideroom/ui';
import { useNavigate } from '@tanstack/react-router';
import { useMemo } from 'react';
import { create } from 'zustand';
import { useT } from '../../i18n';
import type { Channel } from '../../lounge/data';

export interface ViewAsSession {
  guildId: string;
  guildName: string;
  /** Names of the chosen roles (empty: only @everyone). */
  roleNames: string[];
  /** Ids of the channels (and categories) the chosen roles can see. */
  visibleIds: ReadonlySet<string>;
}

interface ViewAsState {
  session: ViewAsSession | null;
  start: (s: ViewAsSession) => void;
  exit: () => void;
}

export const useViewAs = create<ViewAsState>()((set) => ({
  session: null,
  start: (session) => set({ session }),
  exit: () => set({ session: null }),
}));

/** The channels of `guildId` as the previewed roles would see them; the list itself while no preview runs. */
export function useViewAsChannels(guildId: string | undefined, channels: Channel[]): Channel[] {
  const session = useViewAs((s) => s.session);
  return useMemo(
    () =>
      session && session.guildId === guildId
        ? channels.filter((c) => session.visibleIds.has(c.id))
        : channels,
    [session, guildId, channels],
  );
}

/** The exit banner across the top of the Lounge while a preview runs. */
export function ViewAsBanner() {
  const t = useT();
  const session = useViewAs((s) => s.session);
  const exit = useViewAs((s) => s.exit);
  const navigate = useNavigate();
  if (!session) return null;
  const roles = session.roleNames.length
    ? session.roleNames.join(', ')
    : t('settings.viewAs.everyoneOnly');
  return (
    <div role="region" aria-label={t('settings.viewAs.bannerRegion')}>
      <div className="lng-banner" data-tone="info" data-testid="view-as-banner">
        <span>{t('settings.viewAs.banner', { server: session.guildName, roles })}</span>
        <Button
          size="sm"
          variant="secondary"
          onClick={() => {
            const guildId = session.guildId;
            exit();
            void navigate({
              to: '/settings/server/$guildId/$section',
              params: { guildId, section: 'roles' },
            });
          }}
        >
          {t('settings.viewAs.exit')}
        </Button>
      </div>
    </div>
  );
}
