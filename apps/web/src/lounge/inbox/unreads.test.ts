import { describe, expect, it } from 'vitest';
import type { Channel, ReadState } from '../../store';
import { unreadRows } from './unreads';

const ch = (id: string, over: Partial<Channel> = {}): Channel =>
  ({ id, type: 0, guild_id: 'g1', name: `chan-${id}`, last_message_id: '100', ...over }) as Channel;
const rs = (id: string, last: string | null, mentions = 0): ReadState => ({
  channel_id: id,
  last_message_id: last,
  mention_count: mentions,
});

describe('unreadRows', () => {
  it('lists unread channels, mentions first, skipping read, voice and category channels', () => {
    const channels = {
      a: ch('a'),
      b: ch('b'),
      c: ch('c'),
      v: ch('v', { type: 2 }),
      k: ch('k', { type: 4 }),
      d: ch('d', { guild_id: undefined, name: null, recipients: [{ id: '2', username: 'bob' }] }),
    };
    const read = {
      a: rs('a', '100'), // read
      b: rs('b', '50'), // newer message
      c: rs('c', '100', 2), // mentions
      v: rs('v', null, 1),
      k: rs('k', null, 1),
      d: rs('d', null),
    };
    const rows = unreadRows(channels, read);
    expect(rows.map((r) => r.channelId)).toEqual(['c', 'b', 'd']);
    expect(rows[0]).toMatchObject({ mentions: 2, name: 'chan-c', guildId: 'g1' });
    expect(rows[2]).toMatchObject({ name: 'bob', guildId: undefined });
  });
});
