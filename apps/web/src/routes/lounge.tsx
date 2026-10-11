import { createRoute, redirect, type AnyRoute } from '@tanstack/react-router';
import { HOME } from '../lounge/channels';
import { LoungeShell } from '../lounge/LoungeShell';
import { MockDataProvider } from '../lounge/mockData';
import { useLoungeUi } from '../lounge/store';

/**
 * Lounge routes (LNG-SHL-004):
 *   /channels                                         -> last location, else /channels/@me
 *   /channels/@me                                     Friends home
 *   /channels/@me/:dmId                               a DM
 *   /channels/:guildId                                -> remembered/first channel
 *   /channels/:guildId/:channelId[/:messageId]
 *   /channels/:guildId/:channelId/threads/:threadId
 * One layout route renders the whole shell; the shell reads the params. The child routes exist so the
 * router accepts and types these URLs.
 *
 * DATA SWAP: replace <MockDataProvider> with the real store provider (see lounge/data.ts).
 */
function LoungeRoot() {
  return (
    <MockDataProvider>
      <LoungeShell />
    </MockDataProvider>
  );
}

const nothing = () => null;

export function createLoungeRoutes(root: AnyRoute) {
  const layout = createRoute({
    getParentRoute: () => root,
    path: '/channels',
    component: LoungeRoot,
  });
  const index = createRoute({
    getParentRoute: () => layout,
    path: '/',
    beforeLoad: () => {
      const last = useLoungeUi.getState().lastLocation;
      if (last?.channelId)
        throw redirect({
          to: '/channels/$guildId/$channelId',
          params: { guildId: last.guildId, channelId: last.channelId },
          replace: true,
        });
      throw redirect({
        to: '/channels/$guildId',
        params: { guildId: last?.guildId ?? HOME },
        replace: true,
      });
    },
  });
  const guild = createRoute({ getParentRoute: () => layout, path: '$guildId', component: nothing });
  const channel = createRoute({
    getParentRoute: () => layout,
    path: '$guildId/$channelId',
    component: nothing,
  });
  const message = createRoute({
    getParentRoute: () => layout,
    path: '$guildId/$channelId/$messageId',
    component: nothing,
  });
  const thread = createRoute({
    getParentRoute: () => layout,
    path: '$guildId/$channelId/threads/$threadId',
    component: nothing,
  });
  return layout.addChildren([index, guild, channel, message, thread]);
}
