import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { at, baseHandlers, calls, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const channels = {
  c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', position: 1 },
  c5: { id: 'c5', type: 5, guild_id: 'g1', name: 'news', position: 2 },
  c6: { id: 'c6', type: 5, guild_id: 'g1', name: 'other-news', position: 3 },
};

describe('G-30 announcement channels: Follow button and Updates badge', () => {
  beforeEach(() => {
    seedLounge({ messages: [], channels });
    // G-76: the guild object carries the channel; no separate request is made for it.
    useStore.setState((s) => ({
      guilds: { ...s.guilds, g1: { ...s.guilds.g1!, public_updates_channel_id: 'c5' } },
    }));
    baseHandlers();
    server.use(
      http.get('*/api/v1/guilds/g1/sr/community', () => {
        calls.push({ key: 'GET /guilds/g1/sr/community', body: null });
        return HttpResponse.json({ community_enabled: true, public_updates_channel_id: 'c5' });
      }),
      http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([{ id: 'g1', name: 'Club' }])),
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(Object.values(channels))),
      http.post('*/api/v1/channels/c5/followers', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json({ channel_id: 'c5', webhook_id: '77' });
      }),
    );
  });

  it('badges the public updates channel as Updates and offers Follow', async () => {
    at('/channels/g1/c5');
    expect(await screen.findByText('Updates')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Follow' })).toBeInTheDocument();
  });

  it('reads the channel from the guild object and never fetches the community settings', async () => {
    at('/channels/g1/c5');
    await screen.findByText('Updates');
    expect(calls.some((c) => c.key === 'GET /guilds/g1/sr/community')).toBe(false);
  });

  it('keeps the plain Announcements badge on other announcement channels', async () => {
    at('/channels/g1/c6');
    expect(await screen.findByText('Announcements')).toBeInTheDocument();
    expect(screen.queryByText('Updates')).toBeNull();
  });

  it('has no Follow button in a text channel', async () => {
    at('/channels/g1/c1');
    await screen.findByRole('heading', { name: 'general' });
    expect(screen.queryByRole('button', { name: 'Follow' })).toBeNull();
  });

  it('follows into the chosen channel', async () => {
    at('/channels/g1/c5');
    await userEvent.click(await screen.findByRole('button', { name: 'Follow' }));
    const dialog = await screen.findByRole('dialog', { name: 'Follow this channel' });
    const select = await within(dialog).findByRole('combobox');
    await userEvent.selectOptions(select, 'c1');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Follow' }));
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'POST /channels/c5/followers')?.body).toEqual({
        webhook_channel_id: 'c1',
      }),
    );
    expect(await within(dialog).findByText(/Announcements will appear in #general/)).toBeTruthy();
  });
});
