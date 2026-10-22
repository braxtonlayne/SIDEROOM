import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const ch = (id: string, name: string, type: number, extra: object = {}) => ({
  id,
  type,
  guild_id: 'g1',
  name,
  topic: null,
  position: 0,
  parent_id: null,
  nsfw: false,
  rate_limit_per_user: 0,
  permission_overwrites: [],
  ...extra,
});

const channels = () => [
  ch('cat', 'Text', 4),
  ch('c1', 'general', 0, { parent_id: 'cat', position: 0 }),
];

const quotes = (over: object = {}) => ({
  id: 'lounge.message_quotes',
  name: 'Message link quotes',
  description: 'Show a preview of the message behind a message link.',
  section: 'Lounge',
  guild_enabled: true,
  override_enabled: null,
  enabled: true,
  platform_disabled: false,
  settings: { max_quotes: 4 },
  inherited: { max_quotes: 4 },
  overrides: {},
  schema: {
    fields: [
      {
        key: 'max_quotes',
        label: 'Previews per message',
        type: 'int',
        min: 1,
        max: 5,
        default: 2,
        scopes: ['platform', 'guild', 'channel'],
      },
    ],
  },
  ...over,
});

async function openModules() {
  await openSettings('channels');
  await userEvent.click(await screen.findByRole('button', { name: '# general' }));
  await userEvent.click(await screen.findByRole('tab', { name: 'Modules' }));
}

describe('Edit Channel > Modules (LNG-CHN-017)', () => {
  it('switches a module off in this channel and keeps the server value as the baseline', async () => {
    seed({ perms: [MANAGE.channels] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())),
      http.get('*/api/v1/channels/c1/sr/features', () => HttpResponse.json([quotes()])),
      http.patch('*/api/v1/channels/c1/sr/features/lounge.message_quotes', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(quotes({ enabled: false, override_enabled: false }));
      }),
    );
    await openModules();
    const sw = await screen.findByRole('switch', { name: 'Message link quotes in this channel' });
    expect(sw).toBeChecked();
    expect(screen.getByText('This channel follows the server.')).toBeInTheDocument();
    // The server's 4 shows as the value and nothing is saved until the channel really changes it.
    expect(screen.getByLabelText('Previews per message')).toHaveValue(4);
    expect(
      screen.getByRole('button', { name: 'Save Message link quotes for this channel' }),
    ).toBeDisabled();
    await userEvent.click(sw);
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ enabled: false });
    expect(await screen.findByText('This channel has its own switch.')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Follow the server for Message link quotes' }),
    ).toBeInTheDocument();
  });

  it('saves only the value that differs from the server, and can follow the server again', async () => {
    seed({ perms: [MANAGE.channels] });
    const cap = capture();
    let state = quotes();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())),
      http.get('*/api/v1/channels/c1/sr/features', () => HttpResponse.json([state])),
      http.patch('*/api/v1/channels/c1/sr/features/lounge.message_quotes', async ({ request }) => {
        await cap.record(request);
        state = quotes({ settings: { max_quotes: 1 }, overrides: { max_quotes: 1 } });
        return HttpResponse.json(state);
      }),
      http.delete('*/api/v1/channels/c1/sr/features/lounge.message_quotes', async ({ request }) => {
        await cap.record(request);
        state = quotes();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openModules();
    const input = await screen.findByLabelText('Previews per message');
    await userEvent.clear(input);
    await userEvent.type(input, '1');
    await userEvent.click(
      screen.getByRole('button', { name: 'Save Message link quotes for this channel' }),
    );
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ settings: { max_quotes: 1 } });
    await userEvent.click(
      await screen.findByRole('button', { name: 'Follow the server for Message link quotes' }),
    );
    await waitFor(() => expect(cap.calls).toHaveLength(2));
    expect(cap.calls[1]!.method).toBe('DELETE');
    await waitFor(() => expect(screen.getByLabelText('Previews per message')).toHaveValue(4));
    expect(
      screen.queryByRole('button', { name: 'Follow the server for Message link quotes' }),
    ).toBeNull();
  });

  it('cannot switch on a module the server has off, and says why', async () => {
    seed({ perms: [MANAGE.channels] });
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())),
      http.get('*/api/v1/channels/c1/sr/features', () =>
        HttpResponse.json([quotes({ guild_enabled: false, enabled: false })]),
      ),
    );
    await openModules();
    const sw = await screen.findByRole('switch', { name: 'Message link quotes in this channel' });
    expect(sw).toBeDisabled();
    expect(screen.getByText(/off for the whole server/)).toBeInTheDocument();
  });

  it('says so when no module has channel settings, and is absent for categories', async () => {
    seed({ perms: [MANAGE.channels] });
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())),
      http.get('*/api/v1/channels/c1/sr/features', () => HttpResponse.json([])),
    );
    await openModules();
    expect(
      await screen.findByText('No module has settings for a single channel yet.'),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Text' }));
    await waitFor(() => expect(screen.queryByRole('tab', { name: 'Modules' })).toBeNull());
  });
});
