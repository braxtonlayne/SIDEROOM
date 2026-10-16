import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { addAppSearch, type DirectoryAppPage } from '../sections/AppDirectory';
import { http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const app = (id: string, name: string, over: object = {}) => ({
  id,
  name,
  description: `${name} does things`,
  icon: null,
  bot_id: `${id}1`,
  tags: ['music'],
  installs: 3,
  ...over,
});

const page: DirectoryAppPage = {
  ...app('500', 'Jukebox'),
  terms_of_service_url: null,
  privacy_policy_url: 'https://jukebox.test/privacy',
  install_scopes: ['bot', 'applications.commands'],
  install_permissions: '2048',
  commands: [
    { name: 'play', description: 'Play a song', type: 1 },
    { name: 'Queue this', description: '', type: 3 },
  ],
};

describe('addAppSearch', () => {
  it('asks for the application install parameters on this server', () => {
    expect(addAppSearch(page, 'g1')).toEqual({
      client_id: '500',
      scope: 'bot applications.commands',
      permissions: '2048',
      guild_id: 'g1',
    });
    expect(addAppSearch({ ...page, install_scopes: [] }, 'g1').scope).toBe(
      'bot applications.commands',
    );
  });
});

describe('Integrations: Bots and Apps (DEV-BOT-012)', () => {
  it('searches by text and category, and shows the app page with Add App', async () => {
    seed({ perms: [MANAGE.guild] });
    const searches: string[] = [];
    server.use(
      http.get('*/api/v1/app-directory/tags', () =>
        HttpResponse.json([
          { tag: 'music', count: 2 },
          { tag: 'moderation', count: 1 },
        ]),
      ),
      http.get('*/api/v1/app-directory/500', () => HttpResponse.json(page)),
      http.get('*/api/v1/app-directory', ({ request }) => {
        const u = new URL(request.url);
        searches.push(u.searchParams.toString());
        const tag = u.searchParams.get('tag');
        const q = (u.searchParams.get('query') ?? '').toLowerCase();
        const all = [app('500', 'Jukebox'), app('600', 'Guard', { tags: ['moderation'] })];
        const items = all.filter(
          (a) => (!tag || a.tags.includes(tag)) && (!q || a.name.toLowerCase().includes(q)),
        );
        return HttpResponse.json({ items, has_more: false });
      }),
    );
    await openSettings('integrations');
    await userEvent.click(await screen.findByRole('tab', { name: 'Bots and Apps' }));
    const list = await screen.findByRole('list', { name: 'Apps' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(2);

    // A category narrows the list.
    await userEvent.click(screen.getByRole('button', { name: 'moderation' }));
    await waitFor(() =>
      expect(
        within(screen.getByRole('list', { name: 'Apps' })).getAllByRole('listitem'),
      ).toHaveLength(1),
    );
    expect(screen.getByText('Guard')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'All' }));

    // Text search runs after a short pause.
    await userEvent.type(screen.getByLabelText('Search apps'), 'juke');
    await waitFor(() => expect(searches.some((s) => s.includes('query=juke'))).toBe(true));
    await waitFor(() => expect(screen.queryByText('Guard')).not.toBeInTheDocument());

    // The app page lists its commands and links to the authorize screen for this server.
    await userEvent.click(screen.getByRole('button', { name: 'View Jukebox' }));
    const dialog = await screen.findByRole('dialog', { name: 'Jukebox' });
    expect(await within(dialog).findByText('/play')).toBeInTheDocument();
    expect(within(dialog).getByText('Queue this')).toBeInTheDocument();
    const add = within(dialog).getByRole('link', { name: 'Add App' });
    const href = add.getAttribute('href') ?? '';
    expect(href).toContain('/oauth2/authorize');
    expect(href).toContain('client_id=500');
    expect(href).toContain('guild_id=g1');
    expect(href).toContain('permissions=2048');
  });

  it('shows no Add App without Manage Server', async () => {
    seed({ perms: [MANAGE.webhooks] });
    server.use(
      http.get('*/api/v1/app-directory/tags', () => HttpResponse.json([])),
      http.get('*/api/v1/app-directory/500', () => HttpResponse.json(page)),
      http.get('*/api/v1/app-directory', () =>
        HttpResponse.json({ items: [app('500', 'Jukebox')], has_more: false }),
      ),
    );
    await openSettings('integrations');
    await userEvent.click(await screen.findByRole('tab', { name: 'Bots and Apps' }));
    await userEvent.click(await screen.findByRole('button', { name: 'View Jukebox' }));
    const dialog = await screen.findByRole('dialog', { name: 'Jukebox' });
    expect(
      await within(dialog).findByText('You need the Manage Server permission to add apps.'),
    ).toBeInTheDocument();
    expect(within(dialog).queryByRole('link', { name: 'Add App' })).not.toBeInTheDocument();
  });

  it('says so when nothing matches', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(
      http.get('*/api/v1/app-directory/tags', () => HttpResponse.json([])),
      http.get('*/api/v1/app-directory', () => HttpResponse.json({ items: [], has_more: false })),
    );
    await openSettings('integrations');
    await userEvent.click(await screen.findByRole('tab', { name: 'Bots and Apps' }));
    expect(await screen.findByText('No apps found')).toBeInTheDocument();
  });
});
