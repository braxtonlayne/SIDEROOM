// BRG-UI-001, owner direction: the Discord half of a bridged server looks native. The sidebar group "On Discord", the
// popout, inbound authors, mentions both ways and Discord typers. Against MSW handlers shaped like
// settings/server/bridge/api.ts (bridge-gateway reconciles).
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { act } from 'react';
import type { BridgedMember } from '../../settings/server/bridge/api';
import { useStore } from '../../store';
import type * as S from '../../store/types';
import { http, HttpResponse, server } from '../../test/server';
import { useBridgedMembers } from '../bridge/members';
import { at, baseHandlers, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const rhea: BridgedMember = {
  id: '777',
  display_name: 'Rhea',
  avatar: '/cdn/external/rhea',
  role_name: 'Moderators',
  role_color: 0x3366ff,
  bot: false,
  presence: 'online',
};
const imre: BridgedMember = {
  ...rhea,
  id: '778',
  display_name: 'Imre',
  role_name: null,
  presence: null,
};
const botty: BridgedMember = {
  ...rhea,
  id: '779',
  display_name: 'Botty',
  bot: true,
  presence: 'idle',
};

const bridgedChannels = (direction: 'both' | 'to_discord' | 'from_discord' = 'both') => ({
  cat: { id: 'cat', type: 4, guild_id: 'g1', name: 'Text Channels', position: 0 },
  c1: {
    id: 'c1',
    type: 0,
    guild_id: 'g1',
    name: 'general',
    parent_id: 'cat',
    position: 1,
    sr_bridge: { direction },
  },
  c2: { id: 'c2', type: 0, guild_id: 'g1', name: 'random', parent_id: 'cat', position: 2 },
});

const discordMsg = (id: string, over: Partial<S.Message> = {}) =>
  msg(id, {
    author: { id: 'bridge', username: 'Discord bridge', bot: true },
    sr_bridge: {
      platform: 'discord',
      author_name: 'Rhea',
      author_avatar: '/cdn/external/rhea',
      member_id: '777',
    },
    ...over,
  });

let membersCalls = 0;
function setup(
  messages: S.Message[],
  opts: { direction?: 'both' | 'to_discord' | 'from_discord'; bridged?: boolean } = {},
) {
  baseHandlers();
  membersCalls = 0;
  useBridgedMembers.getState().reset();
  seedLounge({
    messages,
    channels: (opts.bridged === false
      ? { ...bridgedChannels(), c1: { ...bridgedChannels().c1, sr_bridge: undefined } }
      : bridgedChannels(opts.direction)) as Record<string, S.Channel>,
  });
  server.use(
    http.get('*/api/v1/guilds/g1/bridge/members', () => {
      membersCalls += 1;
      return HttpResponse.json([rhea, imre, botty]);
    }),
  );
  at('/channels/g1/c1');
}

describe('the member sidebar', () => {
  it('lists the Discord members in an "On Discord" group with tag, role colour and presence', async () => {
    setup([msg('101', { content: 'hi' })]);
    const row = await screen.findByRole('button', { name: 'Rhea, on Discord, online' });
    expect(screen.getByRole('heading', { name: 'On Discord — 3' })).toBeInTheDocument();
    expect(within(row).getByText('DISCORD')).toBeInTheDocument();
    expect(within(row).getByText('Rhea')).toHaveStyle({ color: 'rgb(51, 102, 255)' });
    expect(
      screen.getByRole('button', { name: 'Imre, on Discord, status unknown' }),
    ).toBeInTheDocument();
    // A Discord bot reads as an app, like ours.
    expect(
      within(screen.getByRole('button', { name: 'Botty, on Discord, idle' })).getByText('APP'),
    ).toBeInTheDocument();
  });

  it('is searchable', async () => {
    setup([msg('101', { content: 'hi' })]);
    await screen.findByRole('button', { name: 'Rhea, on Discord, online' });
    await userEvent.type(screen.getByRole('searchbox', { name: 'Search Discord members' }), 'im');
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: /^Rhea, on Discord/ })).toBeNull(),
    );
    expect(screen.getByRole('button', { name: /^Imre, on Discord/ })).toBeInTheDocument();
  });

  it('asks for nothing and shows nothing on a server with no mapped channel', async () => {
    setup([msg('101', { content: 'hi' })], { bridged: false });
    await screen.findByText('hi');
    await new Promise((r) => setTimeout(r, 200));
    expect(membersCalls).toBe(0);
    expect(screen.queryByText(/On Discord/)).toBeNull();
  });
});

describe('the Discord member popout', () => {
  it('opens from the sidebar with name, role and "On Discord via the bridge", and offers nothing to do', async () => {
    setup([msg('101', { content: 'hi' })]);
    await userEvent.click(await screen.findByRole('button', { name: 'Rhea, on Discord, online' }));
    const card = await screen.findByTestId('bridged-card');
    expect(within(card).getByRole('heading', { name: /Rhea/ })).toBeInTheDocument();
    expect(within(card).getByText('On Discord via the bridge')).toBeInTheDocument();
    expect(within(card).getByText('Moderators')).toBeInTheDocument();
    expect(within(card).getByText('DISCORD')).toBeInTheDocument();
    // No DM, note, friend or moderation controls.
    expect(within(card).queryByRole('textbox')).toBeNull();
    expect(within(card).queryByRole('button')).toBeNull();
  });

  it('opens from the author of an inbound message', async () => {
    setup([discordMsg('101', { content: 'hello from over there' })]);
    await screen.findByText('hello from over there');
    await screen.findByRole('button', { name: 'Rhea, on Discord, online' });
    await userEvent.click(screen.getAllByRole('button', { name: 'Open profile of Rhea' })[0]!);
    expect(await screen.findByTestId('bridged-card')).toBeInTheDocument();
  });
});

describe('inbound messages', () => {
  it('group by bridged member id, not by the shared bridge author', async () => {
    setup([
      discordMsg('101', { content: 'first' }),
      discordMsg('102', { content: 'second' }),
      discordMsg('103', {
        content: 'third',
        sr_bridge: { platform: 'discord', author_name: 'Imre', member_id: '778' },
      }),
    ]);
    await screen.findByText('third');
    expect(screen.getAllByTestId('bridge-tag')).toHaveLength(2);
  });

  it('render a mention of a Discord member as a normal mention chip, and it opens the popout', async () => {
    setup([msg('101', { content: 'ping <@777> please' })]);
    const chip = await screen.findByText('@Rhea');
    expect(chip).toHaveClass('sr-md-mention');
    await userEvent.click(chip);
    expect(await screen.findByTestId('bridged-card')).toBeInTheDocument();
  });
});

describe('typing', () => {
  it('shows a Discord typer by name in the existing indicator', async () => {
    setup([msg('101', { content: 'hi' })]);
    await screen.findByRole('button', { name: 'Rhea, on Discord, online' });
    act(() =>
      useStore.getState().applyDispatch('TYPING_START', { channel_id: 'c1', user_id: '777' }),
    );
    expect(await screen.findByText('Rhea is typing…')).toBeInTheDocument();
  });
});

describe('the composer', () => {
  const type = async (text: string) => {
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.click(box);
    await userEvent.type(box, text);
    return box;
  };

  it('offers Discord members after @ in a bridged channel and inserts the <@id> token', async () => {
    setup([msg('101', { content: 'hi' })]);
    await screen.findByRole('button', { name: 'Rhea, on Discord, online' });
    const box = await type('@Rh');
    const option = await screen.findByRole('option', { name: /Rhea/ });
    expect(option).toHaveTextContent('Discord');
    await userEvent.click(option);
    expect(box).toHaveValue('@Rhea ');
  });

  it('does not offer them where the channel is not bridged', async () => {
    setup([msg('101', { content: 'hi' })], { bridged: false });
    await type('@Rh');
    await new Promise((r) => setTimeout(r, 200));
    expect(screen.queryByRole('option', { name: /Rhea/ })).toBeNull();
  });
});
