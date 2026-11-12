// SEC-AUD-137 (wave 61): View Server As Role no longer spins for ever; a module switch keeps unsaved values.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach } from 'vitest';
import { setPermEngine } from '../../../lounge/permEngine';
import {
  bits,
  capture,
  http,
  HttpResponse,
  MANAGE,
  openSettings,
  restRoles,
  seed,
  server,
} from './helpers';

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

describe('View Server As Role when the engine refuses the input (SEC-AUD-137b)', () => {
  afterEach(() => setPermEngine(null));

  it('a_malformed_role_shows_an_error_with_try_again_not_a_spinner', async () => {
    setPermEngine({
      compute: () => {
        throw new Error('malformed role');
      },
    });
    seed({ perms: [MANAGE.roles] });
    server.use(
      http.get('*/api/v1/guilds/g1/roles', () => HttpResponse.json(restRoles())),
      http.get('*/api/v1/guilds/g1/channels', () =>
        HttpResponse.json([ch('c1', 'general', 0, { position: 0 })]),
      ),
    );
    await openSettings('roles');
    await userEvent.click(await screen.findByRole('button', { name: 'View Server As Role' }));
    const dialog = await screen.findByRole('dialog', { name: 'View Server As Role' });
    expect(
      await within(dialog).findByText(/The channel list could not be worked out/),
    ).toBeVisible();
    expect(within(dialog).queryByText('Working it out')).toBeNull();
    expect(within(dialog).getByRole('button', { name: 'Preview in the Lounge' })).toBeDisabled();

    // The role is fixed (here: the engine answers again): Try again gets the list.
    setPermEngine({ compute: () => bits(10) });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Try again' }));
    const list = await within(dialog).findByRole('list', { name: 'Visible channels' });
    expect(within(list).getByText('general')).toBeInTheDocument();
    expect(within(dialog).queryByRole('button', { name: 'Try again' })).toBeNull();
  });
});

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

describe('Edit Channel > Modules switch (SEC-AUD-137c)', () => {
  it('a_toggle_keeps_values_typed_but_not_saved', async () => {
    seed({ perms: [MANAGE.channels] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () =>
        HttpResponse.json([
          ch('cat', 'Text', 4),
          ch('c1', 'general', 0, { parent_id: 'cat', position: 0 }),
        ]),
      ),
      http.get('*/api/v1/channels/c1/sr/features', () => HttpResponse.json([quotes()])),
      http.patch('*/api/v1/channels/c1/sr/features/lounge.message_quotes', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(quotes({ enabled: false, override_enabled: false }));
      }),
    );
    await openSettings('channels');
    await userEvent.click(await screen.findByRole('button', { name: '# general' }));
    await userEvent.click(await screen.findByRole('tab', { name: 'Modules' }));
    const input = await screen.findByLabelText('Previews per message');
    await userEvent.clear(input);
    await userEvent.type(input, '1');
    await userEvent.click(
      screen.getByRole('switch', { name: 'Message link quotes in this channel' }),
    );
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    // The switch sends only itself...
    expect(cap.calls[0]!.body).toEqual({ enabled: false });
    expect(await screen.findByText('This channel has its own switch.')).toBeInTheDocument();
    // ...and what was typed is still there, still waiting to be saved.
    expect(screen.getByLabelText('Previews per message')).toHaveValue(1);
    expect(
      screen.getByRole('button', { name: 'Save Message link quotes for this channel' }),
    ).toBeEnabled();
  });
});
