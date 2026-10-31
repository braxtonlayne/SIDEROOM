// BRG-UI-001 (D-381): the channel banner (read from `sr_bridge` on the channel), the DISCORD tag's mapping and
// grouping, and the privacy page's list of bridged channels. There is no per-member opt-out (owner decision).
import { render, screen } from '@testing-library/react';
import { BridgedChannels } from '../../../help/BridgedChannels';
import { resetStore, useStore } from '../../../store';
import type * as S from '../../../store/types';
import type { Message } from '../../data';
import { bridgeAuthor, toLoungeMessage } from '../../liveMap';
import { buildRows } from '../../messageRows';
import { BridgeBanner } from '../Banner';

const seed = (channels: Record<string, Partial<S.Channel>>) => {
  resetStore();
  useStore.setState({
    guilds: { g1: { id: 'g1', name: 'Book Club' } } as never,
    channels: Object.fromEntries(
      Object.entries(channels).map(([id, c]) => [
        id,
        { id, type: 0, guild_id: 'g1', name: 'general', ...c },
      ]),
    ) as never,
  });
};

describe('the channel banner', () => {
  it('says the channel is mirrored and that Discord messages carry a DISCORD tag', () => {
    seed({ c1: { sr_bridge: { direction: 'both' } } });
    render(<BridgeBanner channelId="c1" />);
    expect(screen.getByTestId('bridge-banner')).toHaveTextContent(
      'Messages here are mirrored to Discord and back; messages from Discord are shown with a DISCORD tag. Everything said in this channel is mirrored.',
    );
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('is absent in a channel that is not mapped', () => {
    seed({ c1: {} });
    render(<BridgeBanner channelId="c1" />);
    expect(screen.queryByTestId('bridge-banner')).toBeNull();
  });

  it('words a one-way channel for what it does', () => {
    seed({ c1: { sr_bridge: { direction: 'to_discord' } } });
    const { unmount } = render(<BridgeBanner channelId="c1" />);
    expect(screen.getByTestId('bridge-banner')).toHaveTextContent(
      /^Messages here are mirrored to Discord\./,
    );
    unmount();
    seed({ c1: { sr_bridge: { direction: 'from_discord' } } });
    render(<BridgeBanner channelId="c1" />);
    expect(screen.getByTestId('bridge-banner')).toHaveTextContent(
      'Messages from Discord are shown here with a DISCORD tag.',
    );
  });
});

describe('the DISCORD tag', () => {
  const raw = (over: Partial<S.Message> = {}): S.Message => ({
    id: 'm1',
    channel_id: 'c1',
    author: { id: 'bridge', username: 'Discord bridge', bot: true },
    content: 'hello from Discord',
    timestamp: '2026-10-30T20:00:00.000Z',
    sr_bridge: { platform: 'discord', author_name: 'Rhea', author_avatar: '/cdn/external/abc' },
    ...over,
  });

  it("shows the Discord author's name and proxied avatar, marks the message, and opens no profile", () => {
    const m = toLoungeMessage(raw());
    expect(m.author).toMatchObject({
      displayName: 'Rhea',
      avatarUrl: '/cdn/external/abc',
      bridge: 'discord',
      webhook: true,
    });
  });

  it('never loads a Discord URL directly', () => {
    const a = bridgeAuthor({
      platform: 'discord',
      author_name: 'Rhea',
      author_avatar: 'https://cdn.discordapp.com/avatars/1/x.png',
    });
    expect(a.avatarUrl).toBeNull();
  });

  it('leaves an ordinary message alone', () => {
    const m = toLoungeMessage(raw({ sr_bridge: undefined }));
    expect(m.author.bridge).toBeUndefined();
    expect(m.author.displayName).toBe('Discord bridge');
  });

  it('does not group messages from two different Discord people under one header', () => {
    const msgs: Message[] = [
      toLoungeMessage(raw({ id: 'm1' })),
      toLoungeMessage(raw({ id: 'm2', timestamp: '2026-10-30T20:00:05.000Z' })),
      toLoungeMessage(
        raw({
          id: 'm3',
          timestamp: '2026-10-30T20:00:10.000Z',
          sr_bridge: { platform: 'discord', author_name: 'Imre' },
        }),
      ),
    ];
    const rows = buildRows(msgs, { atStart: true });
    const grouped = rows.flatMap((r) => (r.kind === 'message' ? [r.grouped] : []));
    expect(grouped).toEqual([false, true, false]);
  });
});

describe('the privacy page list', () => {
  it('lists the mirrored channels of the loaded servers, and empties when none is', () => {
    seed({ c1: { sr_bridge: { direction: 'both' } }, c2: { name: 'random' } });
    const { unmount } = render(<BridgedChannels />);
    expect(screen.getByText('#general in Book Club is mirrored with Discord.')).toBeVisible();
    expect(screen.queryByText(/random/)).toBeNull();
    expect(screen.queryByRole('button')).toBeNull();
    unmount();
    seed({ c1: {} });
    render(<BridgedChannels />);
    expect(screen.getByText('None of your servers mirrors a channel to Discord.')).toBeVisible();
  });
});
