// BRG-UI-001 (D-381): the channel banner, the member's opt-out (banner button and channel menu entry), the DISCORD
// tag's mapping and grouping, and the privacy page's list of bridged channels.
import { render, renderHook, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { BridgedChannels } from '../../../help/BridgedChannels';
import type { BridgedChannel } from '../../../settings/server/bridge/api';
import type * as S from '../../../store/types';
import { http, HttpResponse, server } from '../../../test/server';
import type { Message } from '../../data';
import { bridgeAuthor, toLoungeMessage } from '../../liveMap';
import { buildRows } from '../../messageRows';
import { BridgeBanner } from '../Banner';
import { useBridge } from '../store';
import { useBridgeMenuEntries } from '../useBridgeMenu';

const mapped = (over: Partial<BridgedChannel> = {}): BridgedChannel => ({
  channel_id: 'c1',
  guild_id: 'g1',
  channel_name: 'general',
  guild_name: 'Book Club',
  discord_guild_name: 'Friends',
  direction: 'both',
  opted_out: false,
  ...over,
});

const list = (rows: BridgedChannel[]) =>
  http.get('*/api/v1/users/@me/bridge/channels', () => HttpResponse.json(rows));

beforeEach(() => useBridge.getState().reset());

describe('the channel banner', () => {
  it('says the channel is mirrored and that Discord messages carry a DISCORD tag', async () => {
    server.use(list([mapped()]));
    render(<BridgeBanner channelId="c1" />);
    const banner = await screen.findByTestId('bridge-banner');
    expect(banner).toHaveTextContent(
      'Messages here are mirrored to Discord server Friends and back; messages from Discord are shown with a DISCORD tag.',
    );
  });

  it('is absent in a channel the bridge does not mirror, and when nothing is bridged', async () => {
    server.use(list([mapped({ channel_id: 'other' })]));
    render(<BridgeBanner channelId="c1" />);
    await waitFor(() => expect(useBridge.getState().loadedAt).not.toBeNull());
    expect(screen.queryByTestId('bridge-banner')).toBeNull();
  });

  it('is absent when the list cannot be read', async () => {
    server.use(http.get('*/api/v1/users/@me/bridge/channels', () => HttpResponse.error()));
    render(<BridgeBanner channelId="c1" />);
    await waitFor(() => expect(useBridge.getState().loadedAt).not.toBeNull());
    expect(screen.queryByTestId('bridge-banner')).toBeNull();
  });

  it('words a one-way channel for what it does', async () => {
    server.use(list([mapped({ direction: 'to_discord' })]));
    const { unmount } = render(<BridgeBanner channelId="c1" />);
    expect(
      await screen.findByText(/^Messages here are mirrored to Discord server Friends\.$/),
    ).toBeVisible();
    unmount();
    useBridge.getState().reset();
    server.use(list([mapped({ direction: 'from_discord' })]));
    render(<BridgeBanner channelId="c1" />);
    expect(
      await screen.findByText(/Messages from Discord server Friends are shown here/),
    ).toBeVisible();
    // Nothing of the member's goes out in this direction: no opt-out to offer.
    expect(screen.queryByRole('button')).toBeNull();
  });

  it("opts the member out and back in, sending the channel's opt-out call", async () => {
    const calls: string[] = [];
    server.use(
      list([mapped()]),
      http.put('*/api/v1/channels/c1/bridge/opt-out', () => {
        calls.push('PUT');
        return new HttpResponse(null, { status: 204 });
      }),
      http.delete('*/api/v1/channels/c1/bridge/opt-out', () => {
        calls.push('DELETE');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    render(<BridgeBanner channelId="c1" />);
    await userEvent.click(await screen.findByRole('button', { name: "Don't mirror my messages" }));
    expect(await screen.findByText(/Your messages are not mirrored\./)).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Mirror my messages again' }));
    await waitFor(() => expect(calls).toEqual(['PUT', 'DELETE']));
    expect(screen.queryByText(/Your messages are not mirrored\./)).toBeNull();
  });

  it('rolls the opt-out back and says so when the server refuses', async () => {
    server.use(
      list([mapped()]),
      http.put('*/api/v1/channels/c1/bridge/opt-out', () =>
        HttpResponse.json({ message: 'nope' }, { status: 500 }),
      ),
    );
    render(<BridgeBanner channelId="c1" />);
    await userEvent.click(await screen.findByRole('button', { name: "Don't mirror my messages" }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Could not save that change');
    expect(screen.getByRole('button', { name: "Don't mirror my messages" })).toBeVisible();
  });
});

describe('the channel menu entry', () => {
  const entries = (channelId = 'c1') =>
    renderHook(() => useBridgeMenuEntries(channelId, () => undefined)).result.current;

  it('offers "Don\'t mirror my messages" in a mirrored channel, and the way back once opted out', () => {
    useBridge.setState({ channels: [mapped()] });
    const e = entries();
    expect(e.map((x) => ('label' in x ? x.label : x.type))).toEqual([
      'separator',
      "Don't mirror my messages",
    ]);
    useBridge.setState({ channels: [mapped({ opted_out: true })] });
    expect(entries().map((x) => ('label' in x ? x.label : x.type))).toEqual([
      'separator',
      'Mirror my messages again',
    ]);
  });

  it('has no entry in a channel that is not mirrored or only brings Discord in', () => {
    useBridge.setState({ channels: [mapped({ channel_id: 'x' })] });
    expect(entries()).toEqual([]);
    useBridge.setState({ channels: [mapped({ direction: 'from_discord' })] });
    expect(entries()).toEqual([]);
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
  it('lists the mirrored channels with their Discord server, and empties when none is', async () => {
    server.use(list([mapped()]));
    const { unmount } = render(<BridgedChannels />);
    expect(
      await screen.findByText('#general in Book Club is mirrored with the Discord server Friends.'),
    ).toBeVisible();
    expect(
      screen.getByRole('button', { name: "Don't mirror my messages in #general" }),
    ).toBeVisible();
    unmount();
    useBridge.getState().reset();
    server.use(list([]));
    render(<BridgedChannels />);
    expect(
      await screen.findByText('None of your servers mirrors a channel to Discord.'),
    ).toBeVisible();
  });
});
