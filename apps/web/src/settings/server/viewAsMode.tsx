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
  /**
   * PAR-FU-001: whether a member holding `roleIds` can view `channelId` (the same `sr-permissions` engine, the
   * channel's overwrites). Absent: the member list is left alone.
   */
  canSeeChannel?: (channelId: string, roleIds: readonly string[]) => boolean;
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

/**
 * PAR-FU-001: the member list while a preview runs. A previewer who cannot see the open channel sees nobody; otherwise
 * only the members who can view it with their own roles (the owner always can). Without a preview the list itself.
 */
export function useViewAsMembers<M extends { user: { id: string }; roleIds: readonly string[] }>(
  guildId: string | undefined,
  channelId: string | undefined,
  ownerId: string | undefined,
  members: M[],
): M[] {
  const session = useViewAs((s) => s.session);
  return useMemo(() => {
    if (!session || session.guildId !== guildId || !channelId) return members;
    if (!session.visibleIds.has(channelId)) return [];
    const can = session.canSeeChannel;
    if (!can) return members;
    return members.filter((m) => m.user.id === ownerId || can(channelId, m.roleIds));
  }, [session, guildId, channelId, ownerId, members]);
}

/** PAR-FU-001: whether the threads of `parentId` show in the thread lists (all do without a preview). */
export function useViewAsThreadParent(guildId: string | undefined): (parentId: string) => boolean {
  const session = useViewAs((s) => s.session);
  return useMemo(
    () =>
      session && session.guildId === guildId
        ? (parentId) => session.visibleIds.has(parentId)
        : () => true,
    [session, guildId],
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
