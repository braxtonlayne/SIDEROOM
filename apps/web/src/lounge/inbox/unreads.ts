import type { Channel, ReadState } from '../../store';

export interface UnreadRow {
  channelId: string;
  guildId: string | undefined;
  name: string;
  mentions: number;
}

const newer = (a: string, b: string | null): boolean => {
  if (b === null) return true;
  try {
    return BigInt(a) > BigInt(b);
  } catch {
    return a > b;
  }
};

/**
 * Channels with something unread (LNG-NTF-006): unread mentions, or a newest message past the read
 * marker. Mentions first, then the most recent activity.
 */
export function unreadRows(
  channels: Record<string, Channel>,
  readStates: Record<string, ReadState>,
): UnreadRow[] {
  const rows: (UnreadRow & { last: string })[] = [];
  for (const rs of Object.values(readStates)) {
    const ch = channels[rs.channel_id];
    if (!ch || ch.type === 4 || ch.type === 2) continue;
    const last = ch.last_message_id ?? '';
    const unread = rs.mention_count > 0 || (last !== '' && newer(last, rs.last_message_id));
    if (!unread) continue;
    const dmName = ch.recipients?.map((u) => u.global_name ?? u.username).join(', ');
    rows.push({
      channelId: ch.id,
      guildId: ch.guild_id,
      name: ch.name ?? dmName ?? ch.id,
      mentions: rs.mention_count,
      last,
    });
  }
  rows.sort((a, b) => b.mentions - a.mentions || (newer(a.last, b.last) ? -1 : 1));
  return rows.map((r) => ({
    channelId: r.channelId,
    guildId: r.guildId,
    name: r.name,
    mentions: r.mentions,
  }));
}
