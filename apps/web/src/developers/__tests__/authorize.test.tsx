import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { external } from '../oauth';
import type { AuthorizePreview } from '../types';
import { BOT, http, HttpResponse, openAt, readJson, server, signedIn } from './helpers';

const PREVIEW: AuthorizePreview = {
  application: {
    id: '500',
    name: 'Jukebox',
    description: 'Plays songs in voice.',
    icon: null,
    bot_public: true,
    release_state: 'released',
    privacy_policy_url: null,
    terms_of_service_url: null,
  },
  bot: BOT,
  scopes: [
    { name: 'identify', description: 'See who you are' },
    { name: 'bot', description: 'Add a bot' },
  ],
  redirect_uri: 'https://example.com/cb',
  state: 'xyz',
  user: { id: '1', username: 'alice', global_name: 'Alice' },
  guilds: [
    { id: '7', name: 'Book Club', icon: null, owner: false, permissions: '32', can_install: true },
    {
      id: '8',
      name: 'Just a Member',
      icon: null,
      owner: false,
      permissions: '0',
      can_install: false,
      reason: 'missing_permission',
    },
    {
      id: '9',
      name: 'Big Server',
      icon: null,
      owner: true,
      permissions: '8',
      can_install: false,
      reason: 'app_not_released',
    },
  ],
  permissions: String((1n << 11n) | (1n << 10n)),
  already_authorized: false,
};
const URL_ =
  '/oauth2/authorize?client_id=500&scope=identify+bot&redirect_uri=https%3A%2F%2Fexample.com%2Fcb&state=xyz';

const preview = (over: Partial<AuthorizePreview> = {}) =>
  server.use(
    http.get('*/api/v1/oauth2/authorize', () => HttpResponse.json({ ...PREVIEW, ...over })),
  );

let go: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
  signedIn();
  go = vi.spyOn(external, 'go').mockImplementation(() => undefined);
  preview();
});
afterEach(() => go.mockRestore());

describe('OAuth2 consent screen', () => {
  it('shows the app, scopes, bot permissions and which servers can take it', async () => {
    openAt(URL_);
    expect(await screen.findByRole('heading', { name: 'Jukebox' })).toBeInTheDocument();
    expect(screen.getByText(/account Alice/)).toBeInTheDocument();
    const scopes = screen.getByRole('list', { name: 'Requested access' });
    expect(
      within(scopes).getByText('Access your username, avatar and account id'),
    ).toBeInTheDocument();
    expect(within(scopes).getByText('Add a bot to a server')).toBeInTheDocument();
    const perms = screen.getByRole('list', { name: 'Bot permissions' });
    expect(within(perms).getByText('View Channels')).toBeInTheDocument();
    expect(within(perms).getByText('Send Messages')).toBeInTheDocument();
    expect(screen.getByText('example.com')).toBeInTheDocument();
    const select = screen.getByLabelText('Add to server');
    expect(within(select).getByRole('option', { name: 'Book Club' })).toBeEnabled();
    expect(
      within(select).getByRole('option', { name: 'Just a Member (you need Manage Server)' }),
    ).toBeDisabled();
    expect(
      within(select).getByRole('option', { name: 'Big Server (test servers only)' }),
    ).toBeDisabled();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('needs a server before authorizing a bot', async () => {
    openAt(URL_);
    const authorize = await screen.findByRole('button', { name: 'Authorize' });
    expect(authorize).toBeDisabled();
    await userEvent.setup().selectOptions(screen.getByLabelText('Add to server'), '7');
    expect(authorize).toBeEnabled();
  });

  it('authorizes into the chosen server and follows the redirect', async () => {
    const user = userEvent.setup();
    let body: unknown;
    server.use(
      http.post('*/api/v1/oauth2/authorize', async ({ request }) => {
        body = await readJson(request);
        return HttpResponse.json({ location: 'https://example.com/cb?code=abc&state=xyz' });
      }),
    );
    openAt(URL_);
    await user.selectOptions(await screen.findByLabelText('Add to server'), '7');
    await user.click(screen.getByRole('button', { name: 'Authorize' }));
    await waitFor(() =>
      expect(go).toHaveBeenCalledWith('https://example.com/cb?code=abc&state=xyz'),
    );
    expect(body).toMatchObject({
      client_id: '500',
      scope: 'identify bot',
      redirect_uri: 'https://example.com/cb',
      response_type: 'code',
      state: 'xyz',
      guild_id: '7',
      authorize: true,
    });
  });

  it('lands on the in-app page when there is no redirect URI', async () => {
    const user = userEvent.setup();
    preview({ redirect_uri: null, state: null });
    server.use(
      http.post('*/api/v1/oauth2/authorize', () =>
        HttpResponse.json({ location: '/oauth2/authorized' }),
      ),
    );
    const { router } = openAt('/oauth2/authorize?client_id=500&scope=bot');
    await user.selectOptions(await screen.findByLabelText('Add to server'), '7');
    await user.click(screen.getByRole('button', { name: 'Authorize' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/oauth2/authorized'));
    expect(await screen.findByRole('heading', { name: 'Authorized' })).toBeInTheDocument();
    expect(go).not.toHaveBeenCalled();
  });

  it('refuses to follow a redirect that is not the registered one', async () => {
    const user = userEvent.setup();
    server.use(
      http.post('*/api/v1/oauth2/authorize', () =>
        HttpResponse.json({ location: 'https://evil.test/steal?code=abc' }),
      ),
    );
    openAt(URL_);
    await user.selectOptions(await screen.findByLabelText('Add to server'), '7');
    await user.click(screen.getByRole('button', { name: 'Authorize' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('not a registered redirect');
    expect(go).not.toHaveBeenCalled();
  });

  it('Cancel tells the server and sends the person back with access_denied', async () => {
    const user = userEvent.setup();
    let body: unknown;
    server.use(
      http.post('*/api/v1/oauth2/authorize', async ({ request }) => {
        body = await readJson(request);
        return HttpResponse.json({
          location: 'https://example.com/cb?error=access_denied&state=xyz',
        });
      }),
    );
    openAt(URL_);
    await user.click(await screen.findByRole('button', { name: 'Cancel' }));
    await waitFor(() =>
      expect(go).toHaveBeenCalledWith(expect.stringContaining('error=access_denied')),
    );
    expect(body).toMatchObject({ authorize: false });
  });

  it('works for a plain identify request without a server picker', async () => {
    preview({ scopes: [{ name: 'identify', description: '' }], guilds: [], permissions: '0' });
    openAt(
      '/oauth2/authorize?client_id=500&scope=identify&redirect_uri=https%3A%2F%2Fexample.com%2Fcb',
    );
    expect(await screen.findByRole('button', { name: 'Authorize' })).toBeEnabled();
    expect(screen.queryByLabelText('Add to server')).not.toBeInTheDocument();
  });

  it('warns about Administrator, unreleased apps and a repeat authorization', async () => {
    preview({
      application: { ...PREVIEW.application, release_state: 'development' },
      permissions: '8',
      already_authorized: true,
    });
    openAt(URL_);
    expect(await screen.findByText('This app is still in development')).toBeInTheDocument();
    expect(await screen.findByText(/lets the bot do anything/)).toBeInTheDocument();
    expect(screen.getByText(/authorized this app before/)).toBeInTheDocument();
  });

  it('says when no server can take the bot', async () => {
    preview({ guilds: [PREVIEW.guilds[1]!] });
    openAt(URL_);
    expect(await screen.findByText(/no server you can add this bot to/)).toBeInTheDocument();
  });

  it('shows an error state for a bad redirect URI and never redirects', async () => {
    server.use(
      http.get('*/api/v1/oauth2/authorize', () =>
        HttpResponse.json(
          { code: 50035, message: 'Invalid redirect_uri: it is not registered for this app.' },
          { status: 400 },
        ),
      ),
    );
    openAt(URL_);
    expect(await screen.findByText('This request cannot be authorized')).toBeInTheDocument();
    expect(screen.getByRole('alert')).toHaveTextContent('Invalid redirect_uri');
    expect(screen.queryByRole('button', { name: 'Authorize' })).not.toBeInTheDocument();
    expect(go).not.toHaveBeenCalled();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('says when the application is unknown', async () => {
    server.use(
      http.get('*/api/v1/oauth2/authorize', () =>
        HttpResponse.json({ message: 'Unknown Application' }, { status: 404 }),
      ),
    );
    openAt(URL_);
    expect(await screen.findByRole('alert')).toHaveTextContent('do not know that application');
  });

  it('rejects links missing a client id or scope without calling the server', async () => {
    openAt('/oauth2/authorize?scope=identify');
    expect(await screen.findByRole('alert')).toHaveTextContent('missing a client_id');
  });

  it('sends signed-out people to log in and back', async () => {
    server.use(
      http.get('*/api/v1/oauth2/authorize', () =>
        HttpResponse.json({ message: 'Unauthorized' }, { status: 401 }),
      ),
    );
    const { router } = openAt(URL_);
    await waitFor(() => expect(router.state.location.pathname).toBe('/login'));
    expect(String(router.state.location.search.next)).toContain('/oauth2/authorize?client_id=500');
  });

  it('shows a server failure and lets them try again', async () => {
    const user = userEvent.setup();
    let n = 0;
    server.use(
      http.post('*/api/v1/oauth2/authorize', () => {
        n++;
        return n === 1
          ? HttpResponse.json({ message: 'You need Manage Server there.' }, { status: 403 })
          : HttpResponse.json({ location: '' });
      }),
    );
    openAt(URL_);
    await user.selectOptions(await screen.findByLabelText('Add to server'), '7');
    await user.click(screen.getByRole('button', { name: 'Authorize' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Manage Server there');
    await user.click(screen.getByRole('button', { name: 'Authorize' }));
    expect(await screen.findByText('The bot was added to your server.')).toBeInTheDocument();
  });
});
