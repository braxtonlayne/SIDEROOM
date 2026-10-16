import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import {
  APP,
  BOT,
  http,
  HttpResponse,
  openAt,
  readJson,
  server,
  serveApp,
  signedIn,
  VERSION,
} from './helpers';

beforeEach(signedIn);

const wrongPassword = () =>
  HttpResponse.json({ code: 900004, message: 'Wrong password.' }, { status: 403 });

describe('applications list', () => {
  it('lists applications with their state', async () => {
    server.use(
      http.get('*/api/v1/applications', () =>
        HttpResponse.json([APP, { ...APP, id: '502', name: 'Pinger', release_state: 'released' }]),
      ),
    );
    openAt('/developers/applications');
    const list = await screen.findByRole('list', { name: 'Applications' });
    expect(within(list).getByText('Jukebox')).toBeInTheDocument();
    expect(within(list).getByText('Released')).toBeInTheDocument();
    expect(within(list).getByText('In development')).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('asks signed-out visitors to log in', async () => {
    server.use(
      http.get('*/api/v1/applications', () =>
        HttpResponse.json({ message: 'Unauthorized' }, { status: 401 }),
      ),
    );
    openAt('/developers/applications');
    expect(await screen.findByText('Sign in to build apps')).toBeInTheDocument();
  });

  it('says so when the server has no developer platform yet', async () => {
    server.use(
      http.get('*/api/v1/applications', () =>
        HttpResponse.json({ message: 'Not Found' }, { status: 404 }),
      ),
    );
    openAt('/developers/applications');
    expect(await screen.findByText('Not available yet')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'New application' })).not.toBeInTheDocument();
  });

  it('creates an application, shows its two secrets once, then opens it', async () => {
    const user = userEvent.setup();
    server.use(http.get('*/api/v1/applications', () => HttpResponse.json([])));
    let body: unknown;
    server.use(
      http.post('*/api/v1/applications', async ({ request }) => {
        body = await readJson(request);
        return HttpResponse.json(
          { application: APP, client_secret: 'secret-xyz', bot_token: 'bot.token.abc' },
          { status: 201 },
        );
      }),
    );
    serveApp();
    const { router } = openAt('/developers/applications');
    await user.click(await screen.findByRole('button', { name: 'New application' }));
    const dialog = await screen.findByRole('dialog', { name: 'New application' });
    const create = within(dialog).getByRole('button', { name: 'Create' });
    expect(create).toBeDisabled();
    await user.type(within(dialog).getByLabelText('Name'), 'Jukebox');
    expect(create).toBeDisabled(); // terms not accepted
    await user.click(within(dialog).getByRole('checkbox'));
    await user.click(create);

    const reveal = await screen.findByRole('dialog', { name: 'Save your credentials' });
    expect(within(reveal).getByLabelText('Client secret')).toHaveValue('secret-xyz');
    expect(within(reveal).getByLabelText('Bot token')).toHaveValue('bot.token.abc');
    expect(router.state.location.pathname).toBe('/developers/applications');
    await user.click(within(reveal).getByRole('button', { name: 'Done' }));

    await waitFor(() =>
      expect(router.state.location.pathname).toBe('/developers/applications/500'),
    );
    expect(body).toEqual({ name: 'Jukebox' });
    expect(await screen.findByRole('heading', { name: 'Jukebox' })).toBeInTheDocument();
    expect(document.body.textContent).not.toContain('secret-xyz');
    expect(document.body.textContent).not.toContain('bot.token.abc');
  });

  it('shows a server error in the dialog', async () => {
    const user = userEvent.setup();
    server.use(
      http.get('*/api/v1/applications', () => HttpResponse.json([])),
      http.post('*/api/v1/applications', () =>
        HttpResponse.json({ message: 'You have too many applications.' }, { status: 400 }),
      ),
    );
    openAt('/developers/applications');
    await user.click(await screen.findByRole('button', { name: 'New application' }));
    const dialog = await screen.findByRole('dialog');
    await user.type(within(dialog).getByLabelText('Name'), 'Jukebox');
    await user.click(within(dialog).getByRole('checkbox'));
    await user.click(within(dialog).getByRole('button', { name: 'Create' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('too many applications');
  });
});

describe('application: General', () => {
  it('saves the changed fields only', async () => {
    const user = userEvent.setup();
    const { patches } = serveApp();
    openAt('/developers/applications/500');
    const name = await screen.findByLabelText('Name');
    expect(screen.getByRole('button', { name: 'Save changes' })).toBeDisabled();
    await user.clear(name);
    await user.type(name, 'Jukebox 2');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(patches).toEqual([{ name: 'Jukebox 2' }]));
  });

  it('refuses a too-short name', async () => {
    const user = userEvent.setup();
    serveApp();
    openAt('/developers/applications/500');
    const name = await screen.findByLabelText('Name');
    await user.clear(name);
    await user.type(name, 'x');
    expect(screen.getByRole('button', { name: 'Save changes' })).toBeDisabled();
    expect(screen.getByText('Use 2 to 32 characters.')).toBeInTheDocument();
  });

  it('saves a privacy policy link and rejects a non-https one', async () => {
    const user = userEvent.setup();
    const { patches } = serveApp();
    openAt('/developers/applications/500');
    const privacy = await screen.findByLabelText('Privacy policy URL');
    await user.type(privacy, 'http://example.com/p');
    expect(await screen.findByText('Use an https URL.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save changes' })).toBeDisabled();
    await user.clear(privacy);
    await user.type(privacy, 'https://example.com/p');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(patches).toEqual([{ privacy_policy_url: 'https://example.com/p' }]));
  });

  it('toggles the public bot flag', async () => {
    const user = userEvent.setup();
    const { patches } = serveApp();
    openAt('/developers/applications/500');
    await user.click(await screen.findByRole('switch', { name: /Public bot/ }));
    await waitFor(() => expect(patches).toEqual([{ bot_public: true }]));
  });

  it('rejects an oversized icon without calling the server', async () => {
    const user = userEvent.setup();
    const { patches } = serveApp();
    openAt('/developers/applications/500');
    await screen.findByLabelText('Name');
    const file = new File([new Uint8Array(2 * 1024 * 1024 + 1)], 'big.png', { type: 'image/png' });
    await user.upload(screen.getByLabelText('Icon file'), file);
    expect(await screen.findByText(/larger than 2 MB/)).toBeInTheDocument();
    expect(patches).toEqual([]);
  });

  it('sends an icon as a data URI', async () => {
    const user = userEvent.setup();
    const { patches } = serveApp();
    openAt('/developers/applications/500');
    await screen.findByLabelText('Name');
    await user.upload(
      screen.getByLabelText('Icon file'),
      new File(['png-bytes'], 'icon.png', { type: 'image/png' }),
    );
    await user.click(await screen.findByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(patches).toHaveLength(1));
    expect((patches[0] as { icon: string }).icon).toMatch(/^data:image\/png;base64,/);
  });

  it('deletes only after the name and password are given', async () => {
    const user = userEvent.setup();
    serveApp();
    let body: unknown;
    server.use(
      http.delete('*/api/v1/applications/500', async ({ request }) => {
        body = await readJson(request);
        return new HttpResponse(null, { status: 204 });
      }),
      http.get('*/api/v1/applications', () => HttpResponse.json([])),
    );
    const { router } = openAt('/developers/applications/500');
    await user.click(await screen.findByRole('button', { name: 'Delete application' }));
    const dialog = await screen.findByRole('dialog');
    const confirm = within(dialog).getByRole('button', { name: 'Delete application' });
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByLabelText(/Type "Jukebox"/), 'Jukebox');
    expect(confirm).toBeDisabled(); // password still missing
    await user.type(within(dialog).getByLabelText('Your password'), 'hunter2');
    await user.click(confirm);
    await waitFor(() => expect(router.state.location.pathname).toBe('/developers/applications'));
    expect(body).toEqual({ password: 'hunter2' });
  });

  it('keeps the dialog open on a wrong password', async () => {
    const user = userEvent.setup();
    serveApp();
    server.use(http.delete('*/api/v1/applications/500', wrongPassword));
    openAt('/developers/applications/500');
    await user.click(await screen.findByRole('button', { name: 'Delete application' }));
    const dialog = await screen.findByRole('dialog');
    await user.type(within(dialog).getByLabelText(/Type "Jukebox"/), 'Jukebox');
    await user.type(within(dialog).getByLabelText('Your password'), 'nope');
    await user.click(within(dialog).getByRole('button', { name: 'Delete application' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent(
      'That password is not right.',
    );
  });

  it('reports an application that is gone', async () => {
    server.use(
      http.get('*/api/v1/applications/9', () =>
        HttpResponse.json({ message: 'Unknown Application' }, { status: 404 }),
      ),
    );
    openAt('/developers/applications/9');
    expect(await screen.findByText('Application not found')).toBeInTheDocument();
  });

  it('passes axe', async () => {
    serveApp();
    openAt('/developers/applications/500');
    await screen.findByLabelText('Name');
    expect(await axeViolations(document.body)).toEqual([]);
  });
});

describe('application: OAuth2', () => {
  const open = () => openAt('/developers/applications/500?tab=oauth2');

  it('shows the client id and only a hint of the secret', async () => {
    serveApp();
    open();
    expect(await screen.findByLabelText('Client ID')).toHaveValue('500');
    expect(screen.getByText(/ending in abcd is set/)).toBeInTheDocument();
  });

  it('resets the secret with the password, shows it once and clears it on close', async () => {
    const user = userEvent.setup();
    serveApp();
    let body: unknown;
    server.use(
      http.post('*/api/v1/applications/500/reset-secret', async ({ request }) => {
        body = await readJson(request);
        return HttpResponse.json({ client_secret: 'sek_live_123' });
      }),
    );
    open();
    await user.click(await screen.findByRole('button', { name: 'Reset secret' }));
    await user.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(body).toBeUndefined();

    await user.click(screen.getByRole('button', { name: 'Reset secret' }));
    const confirm = await screen.findByRole('dialog', { name: 'Reset the client secret?' });
    expect(within(confirm).getByRole('button', { name: 'Yes, reset it' })).toBeDisabled();
    await user.type(within(confirm).getByLabelText('Your password'), 'hunter2');
    await user.click(within(confirm).getByRole('button', { name: 'Yes, reset it' }));
    const reveal = await screen.findByRole('dialog', { name: 'New client secret' });
    expect(within(reveal).getByLabelText('Client secret')).toHaveValue('sek_live_123');
    expect(within(reveal).getByText('You will not see this again')).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);

    await user.click(within(reveal).getByRole('button', { name: 'Done' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(document.body.textContent).not.toContain('sek_live_123');
    expect(JSON.stringify({ ...localStorage, ...sessionStorage })).not.toContain('sek_live_123');
    expect(body).toEqual({ password: 'hunter2' });
  });

  it('says when the password is wrong and does not reveal anything', async () => {
    const user = userEvent.setup();
    serveApp();
    server.use(http.post('*/api/v1/applications/500/reset-secret', wrongPassword));
    open();
    await user.click(await screen.findByRole('button', { name: 'Reset secret' }));
    const confirm = await screen.findByRole('dialog');
    await user.type(within(confirm).getByLabelText('Your password'), 'nope');
    await user.click(within(confirm).getByRole('button', { name: 'Yes, reset it' }));
    expect(await within(confirm).findByRole('alert')).toHaveTextContent(
      'That password is not right.',
    );
    expect(screen.queryByRole('dialog', { name: 'New client secret' })).not.toBeInTheDocument();
  });

  it('copies the secret', async () => {
    const user = userEvent.setup();
    serveApp();
    server.use(
      http.post('*/api/v1/applications/500/reset-secret', () =>
        HttpResponse.json({ client_secret: 'abc' }),
      ),
    );
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: write },
      configurable: true,
    });
    open();
    await user.click(await screen.findByRole('button', { name: 'Reset secret' }));
    await user.type(await screen.findByLabelText('Your password'), 'pw');
    await user.click(screen.getByRole('button', { name: 'Yes, reset it' }));
    await user.click(await screen.findByRole('button', { name: 'Copy client secret' }));
    expect(write).toHaveBeenCalledWith('abc');
  });

  it('validates redirect URIs before adding them', async () => {
    const user = userEvent.setup();
    const { patches } = serveApp();
    open();
    const input = await screen.findByLabelText('Add a redirect URI');
    await user.type(input, 'http://example.com/cb');
    await user.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByText(/Plain http is only allowed/)).toBeInTheDocument();

    await user.clear(input);
    await user.type(input, 'myapp://callback');
    await user.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByText(/Only https URLs/)).toBeInTheDocument();

    await user.clear(input);
    await user.type(input, 'https://example.com/cb');
    await user.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByText('That URI is already in the list.')).toBeInTheDocument();

    await user.clear(input);
    await user.type(input, 'https://app.example.com/callback');
    await user.click(screen.getByRole('button', { name: 'Add' }));
    await user.click(screen.getByRole('button', { name: 'Remove https://example.com/cb' }));
    await user.click(screen.getByRole('button', { name: 'Save redirects' }));
    await waitFor(() =>
      expect(patches).toEqual([{ redirect_uris: ['https://app.example.com/callback'] }]),
    );
  });

  it('generates the authorize URL and the permissions integer', async () => {
    const user = userEvent.setup();
    serveApp();
    open();
    await screen.findByRole('heading', { name: 'URL generator' });
    const url = () => (screen.getByLabelText('Generated URL') as HTMLInputElement).value;
    expect(url()).toBe('');
    await user.click(screen.getByRole('checkbox', { name: /^identify/ }));
    await user.selectOptions(screen.getByLabelText('Redirect URI'), 'https://example.com/cb');
    let u = new URL(url());
    expect(u.pathname).toBe('/oauth2/authorize');
    expect(u.searchParams.get('client_id')).toBe('500');
    expect(u.searchParams.get('scope')).toBe('identify');
    expect(u.searchParams.get('redirect_uri')).toBe('https://example.com/cb');
    expect(u.searchParams.get('response_type')).toBe('code');

    await user.click(screen.getByRole('checkbox', { name: /^bot/ }));
    await user.click(await screen.findByRole('switch', { name: /^Send Messages(?! in)/ }));
    u = new URL(url());
    expect(u.searchParams.get('scope')).toBe('identify bot');
    expect(u.searchParams.get('permissions')).toBe(String(1n << 11n));
    expect(screen.getByText(String(1n << 11n))).toBeInTheDocument();
  });

  it('warns when Administrator is picked', async () => {
    const user = userEvent.setup();
    serveApp();
    open();
    await user.click(await screen.findByRole('checkbox', { name: /^bot/ }));
    await user.click(await screen.findByRole('switch', { name: /Administrator/ }));
    expect(screen.getByText(/bypasses every permission check/)).toBeInTheDocument();
    expect(screen.getByText('8')).toBeInTheDocument();
  });
});

describe('application: Bot', () => {
  const open = () => openAt('/developers/applications/500?tab=bot');

  it('resets the token with the password and shows it once', async () => {
    const user = userEvent.setup();
    serveApp();
    let body: unknown;
    server.use(
      http.post('*/api/v1/applications/500/bot/reset-token', async ({ request }) => {
        body = await readJson(request);
        return HttpResponse.json({ token: 'new.bot.token' });
      }),
    );
    open();
    await user.click(await screen.findByRole('button', { name: 'Reset token' }));
    const confirm = await screen.findByRole('dialog', { name: 'Reset the bot token?' });
    await user.click(within(confirm).getByRole('button', { name: 'Cancel' }));
    expect(body).toBeUndefined();
    await user.click(screen.getByRole('button', { name: 'Reset token' }));
    await user.type(await screen.findByLabelText('Your password'), 'hunter2');
    await user.click(screen.getByRole('button', { name: 'Yes, reset it' }));
    const reveal = await screen.findByRole('dialog', { name: 'Bot token' });
    expect(within(reveal).getByDisplayValue('new.bot.token')).toBeInTheDocument();
    await user.click(within(reveal).getByRole('button', { name: 'Done' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
    expect(document.body.textContent).not.toContain('new.bot.token');
    expect(body).toEqual({ password: 'hunter2' });
  });

  it('renames the bot', async () => {
    const user = userEvent.setup();
    serveApp();
    let body: unknown;
    server.use(
      http.patch('*/api/v1/applications/500/bot', async ({ request }) => {
        body = await readJson(request);
        return HttpResponse.json({ ...BOT, username: 'Jukebox Two' });
      }),
    );
    open();
    const name = await screen.findByLabelText('Bot name');
    expect(screen.getByRole('button', { name: 'Save bot name' })).toBeDisabled();
    await user.clear(name);
    await user.type(name, 'Jukebox Two');
    await user.click(screen.getByRole('button', { name: 'Save bot name' }));
    await waitFor(() => expect(body).toEqual({ username: 'Jukebox Two' }));
  });

  it('shows intent review status and requests an intent', async () => {
    const user = userEvent.setup();
    const { patches } = serveApp({
      ...APP,
      privileged_intents: {
        requested: ['GUILD_PRESENCES', 'GUILD_MEMBERS'],
        approved: ['GUILD_PRESENCES'],
      },
    });
    open();
    expect(await screen.findByText('Approved')).toBeInTheDocument();
    expect(screen.getByText('Waiting for approval')).toBeInTheDocument();
    await user.click(screen.getByRole('switch', { name: /Message content intent/ }));
    await waitFor(() =>
      expect(patches).toEqual([
        { privileged_intents: ['GUILD_PRESENCES', 'GUILD_MEMBERS', 'MESSAGE_CONTENT'] },
      ]),
    );
    await user.click(screen.getByRole('switch', { name: /Presence intent/ }));
    await waitFor(() => expect(patches).toHaveLength(2));
    expect((patches[1] as { privileged_intents: string[] }).privileged_intents).toEqual([
      'GUILD_MEMBERS',
      'MESSAGE_CONTENT',
    ]);
    expect(await axeViolations(document.body)).toEqual([]);
  });
});

describe('application: Installation', () => {
  it('starts from the default install settings and explains test servers', async () => {
    const user = userEvent.setup();
    serveApp({
      ...APP,
      install_params: { scopes: ['bot', 'applications.commands'], permissions: '2' },
    });
    server.use(http.get('*/api/v1/applications/500/installs', () => HttpResponse.json([])));
    openAt('/developers/applications/500?tab=installation');
    expect(await screen.findByText('Test servers only, for now')).toBeInTheDocument();
    expect(screen.getByText('Private bot')).toBeInTheDocument();
    const url = () => new URL((screen.getByLabelText('Generated URL') as HTMLInputElement).value);
    expect(url().searchParams.get('scope')).toBe('bot applications.commands');
    expect(url().searchParams.get('permissions')).toBe('2');
    await user.click(await screen.findByRole('switch', { name: /Ban Members/ }));
    expect(url().searchParams.get('permissions')).toBe('6');
    expect(await screen.findByText('Not installed in any server yet.')).toBeInTheDocument();
  });

  it('saves the default install settings', async () => {
    const user = userEvent.setup();
    const { patches } = serveApp();
    server.use(
      http.get('*/api/v1/applications/500/installs', () =>
        HttpResponse.json([
          {
            guild_id: '77',
            installed_by: '1',
            scopes: ['bot'],
            permissions: '0',
            created_at: '2026-10-03T00:00:00Z',
          },
        ]),
      ),
    );
    openAt('/developers/applications/500?tab=installation');
    await user.click(await screen.findByRole('switch', { name: /Kick Members/ }));
    await user.click(screen.getByRole('button', { name: 'Save as default install settings' }));
    await waitFor(() =>
      expect(patches).toEqual([
        { install_params: { scopes: ['bot', 'applications.commands'], permissions: '2' } },
      ]),
    );
    const installs = await screen.findByRole('list', { name: 'Installs' });
    expect(within(installs).getByText('77')).toBeInTheDocument();
  });
});

describe('application: Commands builder', () => {
  const open = () => {
    serveApp();
    return openAt('/developers/applications/500?tab=commands');
  };

  it('explains that commands use the bot token and has no fake list', async () => {
    open();
    expect(await screen.findByText('Commands are managed with the bot token')).toBeInTheDocument();
    expect(screen.queryByRole('list', { name: 'Commands' })).not.toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('checks the command and produces the request', async () => {
    const user = userEvent.setup();
    open();
    await user.type(await screen.findByLabelText('Name'), 'Ping');
    expect(screen.getByLabelText('Name')).toHaveValue('ping');
    await user.type(screen.getByLabelText('Description'), 'Pong');
    await user.click(screen.getByRole('button', { name: 'Add option' }));
    await user.type(screen.getByLabelText('Option 1 name'), 'text');
    await user.type(screen.getByLabelText('Option 1 description'), 'Say it');
    await user.click(screen.getByLabelText('Option 1 is required'));
    const body = screen.getAllByText(/"name": "ping"/)[0]!.textContent!;
    expect(JSON.parse(body)).toEqual({
      type: 1,
      name: 'ping',
      description: 'Pong',
      options: [{ type: 3, name: 'text', description: 'Say it', required: true }],
    });
    expect(screen.getByText(/\/api\/v1\/applications\/500\/commands/)).toBeInTheDocument();
  });

  it('targets a server when an id is given and rejects a bad one', async () => {
    const user = userEvent.setup();
    open();
    await user.type(await screen.findByLabelText('Name'), 'ping');
    await user.type(screen.getByLabelText('Description'), 'Pong');
    await user.type(screen.getByLabelText(/Server id/), '12x');
    expect(await screen.findByText('A server id is digits only.')).toBeInTheDocument();
    await user.clear(screen.getByLabelText(/Server id/));
    await user.type(screen.getByLabelText(/Server id/), '7');
    expect(screen.getByText(/applications\/500\/guilds\/7\/commands/)).toBeInTheDocument();
  });

  it('flags an option that breaks the ordering rule and withholds the request', async () => {
    const user = userEvent.setup();
    open();
    await user.type(await screen.findByLabelText('Name'), 'ping');
    await user.type(screen.getByLabelText('Description'), 'Pong');
    await user.click(screen.getByRole('button', { name: 'Add option' }));
    await user.click(screen.getByRole('button', { name: 'Add option' }));
    await user.type(screen.getByLabelText('Option 1 name'), 'a');
    await user.type(screen.getByLabelText('Option 1 description'), 'd');
    await user.type(screen.getByLabelText('Option 2 name'), 'b');
    await user.type(screen.getByLabelText('Option 2 description'), 'd');
    await user.click(screen.getByLabelText('Option 2 is required'));
    expect(await screen.findByText(/Required options must come before/)).toBeInTheDocument();
    expect(screen.queryByText('Request body')).not.toBeInTheDocument();
  });

  it('drops options and description for user commands', async () => {
    const user = userEvent.setup();
    open();
    await user.selectOptions(await screen.findByLabelText('Type'), '2');
    expect(screen.queryByLabelText('Description')).not.toBeInTheDocument();
    await user.type(screen.getByLabelText('Name'), 'Show profile');
    expect(JSON.parse(screen.getAllByText(/"Show profile"/)[0]!.textContent!)).toEqual({
      type: 2,
      name: 'Show profile',
    });
  });
});

describe('application: Review', () => {
  const open = () => openAt('/developers/applications/500?tab=review');

  it('lists versions with the reviewer message and what changed', async () => {
    serveApp();
    server.use(
      http.get('*/api/v1/applications/500/versions', () =>
        HttpResponse.json([
          {
            ...VERSION,
            id: '701',
            version: 2,
            state: 'changes_requested',
            reviewer_note: 'Please add a privacy policy.',
          },
          { ...VERSION, state: 'approved' },
        ]),
      ),
    );
    open();
    const list = await screen.findByRole('list', { name: 'Versions' });
    expect(within(list).getByText('Version 2')).toBeInTheDocument();
    expect(within(list).getByText('Changes requested')).toBeInTheDocument();
    expect(within(list).getByText('Please add a privacy policy.')).toBeInTheDocument();
    expect(within(list).getAllByText(/first release, scopes changed/).length).toBe(2);
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('submits a version with a note after confirming', async () => {
    const user = userEvent.setup();
    serveApp();
    let body: unknown;
    let versions: unknown[] = [];
    server.use(
      http.get('*/api/v1/applications/500/versions', () => HttpResponse.json(versions)),
      http.post('*/api/v1/applications/500/versions', async ({ request }) => {
        body = await readJson(request);
        versions = [VERSION];
        return HttpResponse.json(VERSION, { status: 201 });
      }),
    );
    open();
    await screen.findByText('No versions submitted yet.');
    await user.type(screen.getByLabelText(/Note for the reviewer/), 'Please review');
    await user.click(screen.getByRole('button', { name: 'Submit for review' }));
    await user.click(
      within(await screen.findByRole('dialog', { name: 'Submit for review?' })).getByRole(
        'button',
        {
          name: 'Submit',
        },
      ),
    );
    await waitFor(() => expect(body).toEqual({ note: 'Please review' }));
    expect(await screen.findByText('Version 1')).toBeInTheDocument();
    expect(screen.getByText('A version is already waiting for review.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Submit for review' })).toBeDisabled();
  });

  it('withdraws a pending version after confirming', async () => {
    const user = userEvent.setup();
    serveApp();
    let withdrawn = 0;
    server.use(
      http.get('*/api/v1/applications/500/versions', () => HttpResponse.json([VERSION])),
      http.post('*/api/v1/applications/500/versions/700/withdraw', () => {
        withdrawn++;
        return HttpResponse.json({ ...VERSION, state: 'withdrawn' });
      }),
    );
    open();
    await user.click(await screen.findByRole('button', { name: 'Withdraw version 1' }));
    const confirm = await screen.findByRole('dialog', { name: 'Withdraw version 1?' });
    await user.click(within(confirm).getByRole('button', { name: 'Cancel' }));
    expect(withdrawn).toBe(0);
    await user.click(screen.getByRole('button', { name: 'Withdraw version 1' }));
    await user.click(await screen.findByRole('button', { name: 'Withdraw' }));
    await waitFor(() => expect(withdrawn).toBe(1));
  });

  it('shows why a submission failed', async () => {
    const user = userEvent.setup();
    serveApp();
    server.use(
      http.get('*/api/v1/applications/500/versions', () => HttpResponse.json([])),
      http.post('*/api/v1/applications/500/versions', () =>
        HttpResponse.json(
          { code: 900010, message: 'A version is already pending.' },
          { status: 409 },
        ),
      ),
    );
    open();
    await user.click(await screen.findByRole('button', { name: 'Submit for review' }));
    await user.click(await screen.findByRole('button', { name: 'Submit' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('already pending');
  });

  it('warns about a suspended application and blocks submitting', async () => {
    serveApp({ ...APP, release_state: 'suspended' });
    server.use(http.get('*/api/v1/applications/500/versions', () => HttpResponse.json([])));
    open();
    expect(await screen.findByText('This application is suspended')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Submit for review' })).toBeDisabled();
  });
});

describe('overview and developer application', () => {
  const status = (body: Record<string, unknown>, code = 200) =>
    server.use(
      http.get('*/api/v1/developers/status', () => HttpResponse.json(body, { status: code })),
    );

  it('sends the application and then shows it as waiting', async () => {
    const user = userEvent.setup();
    let body: unknown;
    let pending: Record<string, unknown> | null = null;
    server.use(
      http.get('*/api/v1/developers/status', () =>
        HttpResponse.json({ is_developer: false, request: pending }),
      ),
      http.post('*/api/v1/developers/apply', async ({ request }) => {
        body = await readJson(request);
        pending = {
          id: '1',
          account_id: '1',
          state: 'pending',
          pitch: 'A jukebox',
          prior_work: '',
          created_at: '2026-10-04T00:00:00Z',
          reviewed_at: null,
          reviewer_note: null,
        };
        return HttpResponse.json(pending, { status: 201 });
      }),
    );
    openAt('/developers');
    expect(await screen.findByRole('heading', { name: 'Build on SIDE ROOM' })).toBeInTheDocument();
    const send = await screen.findByRole('button', { name: 'Send application' });
    expect(send).toBeDisabled();
    await user.type(screen.getByLabelText('What do you want to build?'), 'A jukebox');
    await user.click(screen.getByRole('checkbox', { name: /developer terms/ }));
    expect(await axeViolations(document.body)).toEqual([]);
    await user.click(send);
    await waitFor(() => expect(body).toEqual({ pitch: 'A jukebox', accepted_terms: true }));
    expect(await screen.findByText('Waiting for review')).toBeInTheDocument();
  });

  it('shows approval and a rejection reason', async () => {
    status({ is_developer: true, request: null });
    const { unmount } = openAt('/developers');
    expect(await screen.findByText('You are an approved developer')).toBeInTheDocument();
    unmount();
    status({
      is_developer: false,
      request: {
        id: '1',
        account_id: '1',
        state: 'rejected',
        pitch: 'x',
        prior_work: '',
        created_at: '2026-10-04T00:00:00Z',
        reviewed_at: '2026-10-05T00:00:00Z',
        reviewer_note: 'Too vague.',
      },
    });
    openAt('/developers');
    expect(await screen.findByText(/Too vague\./)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send application' })).toBeInTheDocument();
  });

  it('asks signed-out visitors to log in and admits a missing endpoint', async () => {
    status({ message: 'Unauthorized' }, 401);
    const { unmount } = openAt('/developers');
    expect(await screen.findByText('Sign in to apply')).toBeInTheDocument();
    unmount();
    status({ message: 'Not Found' }, 404);
    openAt('/developers');
    expect(await screen.findByText(/does not take developer applications yet/)).toBeInTheDocument();
  });
});

describe('documentation hub', () => {
  it('lists every page and renders one with copyable samples', async () => {
    const user = userEvent.setup();
    const write = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: write },
      configurable: true,
    });
    openAt('/developers/docs');
    const list = await screen.findByRole('list', { name: 'Documentation pages' });
    for (const t of [
      'Getting started',
      'Authentication',
      'Gateway',
      'REST',
      'Rate limits and status',
      'Interactions',
    ])
      expect(within(list).getByText(t)).toBeInTheDocument();
    await user.click(within(list).getByText('Getting started'));
    expect(
      await screen.findByRole('heading', { level: 1, name: 'Getting started' }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Copy curl' }));
    expect(write).toHaveBeenCalledWith(expect.stringContaining('Authorization: Bot $BOT_TOKEN'));
    expect(write.mock.calls[0]![0]).toContain(`${window.location.origin}/api/v1/users/@me`);
    expect(screen.getByRole('button', { name: 'Copy TypeScript' })).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('explains rate limits and admits there is no status API', async () => {
    openAt('/developers/docs/rate-limits');
    expect(await screen.findByText(/X-RateLimit-Limit/)).toBeInTheDocument();
    expect(screen.getByText(/no public status page/)).toBeInTheDocument();
  });

  it('explains both ways of receiving interactions', async () => {
    openAt('/developers/docs/interactions');
    expect(
      await screen.findByText(/interactions endpoint URL in the General tab/),
    ).toBeInTheDocument();
  });

  it('has pages for the bot SDKs and for porting from Discord', async () => {
    openAt('/developers/docs/sdks');
    expect(await screen.findByRole('heading', { name: 'Rust: sr-sdk' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'TypeScript: @sideroom/bot' })).toBeInTheDocument();
  });

  it('handles an unknown page', async () => {
    openAt('/developers/docs/nope');
    expect(await screen.findByText('No such page')).toBeInTheDocument();
  });
});
