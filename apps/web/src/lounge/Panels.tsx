import { EmptyState } from '@sideroom/ui';
import { useT } from '../i18n';
import { isTextLike } from './channels';
import { useChannels } from './data';
import { DmSidePanel } from './shell/DmPanels';
import { ServerWelcome } from './shell/ServerWelcome';
import { FriendsPage } from './shell/FriendsPage';
import { MessageRequestsPage } from './shell/MessageRequestsPage';
import { useLoungeUi } from './store';

/** Right-hand panel of a DM: the person's profile, or the members of a group (LNG-DM-006, LNG-DM-007). */
export const DmProfilePanel = DmSidePanel;

/** Home with no conversation selected: the Friends page, or Message Requests when chosen in the sidebar. */
export function HomeFriends() {
  const view = useLoungeUi((s) => s.homeView);
  return view === 'requests' ? <MessageRequestsPage /> : <FriendsPage />;
}

export function PickChannel({ guildId }: { guildId?: string }) {
  const t = useT();
  const channels = useChannels(guildId);
  // A server with nowhere to talk yet shows the welcome checklist instead of an empty prompt.
  if (guildId && !channels.some(isTextLike)) return <ServerWelcome guildId={guildId} />;
  return (
    <section className="lng-channel-view" aria-label={t('lounge.panels.noChannelSelected')}>
      <EmptyState
        title={t('lounge.panels.pickChannelTitle')}
        description={t('lounge.panels.pickChannelBody')}
      />
    </section>
  );
}
