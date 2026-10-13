import type { LiveSnapshot } from '../live';
import type { Channel, Guild } from '../data';
import { isMuted, type GuildNotif } from './notifSettings';

/**
 * Overlays the person's mute settings on a snapshot: muted channels get `muted`, a muted server mutes all of
 * its channels, and a muted server shows neither an unread pill nor a mention badge on the rail.
 * Returns the same snapshot when nothing is muted.
 */
export function applyMutes(
  snap: LiveSnapshot,
  byGuild: Record<string, GuildNotif>,
  now = Date.now(),
): LiveSnapshot {
  if (!Object.keys(byGuild).length) return snap;
  const channelsByGuild = { ...snap.channelsByGuild };
  let guilds: Guild[] = snap.guilds;
  let changed = false;
  guilds = snap.guilds.map((g) => {
    const n = byGuild[g.id];
    if (!n) return g;
    const guildMuted = isMuted(n, now);
    const list = snap.channelsByGuild[g.id];
    if (list) {
      let any = false;
      const next = list.map((c): Channel => {
        const m =
          guildMuted ||
          isMuted(n.channels[c.id], now) ||
          (!!c.parentId && isMuted(n.channels[c.parentId], now));
        if (!m || c.muted) return c;
        any = true;
        return { ...c, muted: true };
      });
      if (any) channelsByGuild[g.id] = next;
    }
    if (!guildMuted) {
      if (channelsByGuild[g.id] === list) return g;
      changed = true;
      const chans = channelsByGuild[g.id] ?? [];
      return {
        ...g,
        unread: chans.some((c) => c.unread && !c.muted),
        mentionCount: chans.reduce((s, c) => s + (c.muted ? 0 : c.mentionCount), 0),
      };
    }
    changed = true;
    return { ...g, muted: true, unread: false, mentionCount: 0 };
  });
  return changed ? { ...snap, guilds, channelsByGuild } : snap;
}
