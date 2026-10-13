import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { ServerSettingsPage } from '../settings/server/ServerSettings';

/**
 * Server settings routes (LNG-SSET-001):
 *   /settings/server/:guildId            -> first section the viewer may open
 *   /settings/server/:guildId/:section   overview | roles | channels | emoji | members | invites | bans |
 *                                        integrations | moderation | audit-log | community | delete
 */
export function createServerSettingsRoutes(root: AnyRoute) {
  const guild = createRoute({
    getParentRoute: () => root,
    path: '/settings/server/$guildId',
    component: ServerSettingsPage,
  });
  const section = createRoute({
    getParentRoute: () => root,
    path: '/settings/server/$guildId/$section',
    component: ServerSettingsPage,
  });
  return [guild, section];
}
