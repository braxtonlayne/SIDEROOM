import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const pkg = (over: object) => ({
  id: '10',
  kind: 'recipe',
  slug: 'club-starter',
  name: 'Club starter',
  description: 'A tidy start for a book club.',
  author_id: '9',
  author_name: 'Ada',
  version_id: '11',
  version: '1.0.0',
  size_bytes: 400,
  updated_at_ms: 0,
  payload: {
    features: [
      { id: 'lounge.starboard', enabled: true, settings: { threshold: 2 } },
      { id: 'lounge.sticky', enabled: false, settings: null },
    ],
  },
  ...over,
});

describe('Server Settings > Extensions (EXT-PKG-006)', () => {
  beforeEach(() => seed({ perms: [MANAGE.guild] }));

  it('shows what a recipe will change before applying it, and applies it once confirmed', async () => {
    const applied = capture();
    let on: object[] = [];
    server.use(
      http.get('*/api/v1/packages', ({ request }) =>
        HttpResponse.json(
          new URL(request.url).searchParams.get('kind') === 'recipe' ? [pkg({})] : [],
        ),
      ),
      http.get('*/api/v1/guilds/g1/packages', () => HttpResponse.json(on)),
      http.post('*/api/v1/guilds/g1/packages/10/apply', async ({ request }) => {
        await applied.record(request);
        on = [
          {
            package_id: '10',
            version_id: '11',
            kind: 'recipe',
            slug: 'club-starter',
            name: 'Club starter',
            version: '1.0.0',
            section: 'all',
            config: {},
            enabled_at_ms: 0,
            update_available: false,
            payload: null,
          },
        ];
        return HttpResponse.json({ applied: [], enablement: on[0] });
      }),
    );
    await openSettings('extensions');
    const user = userEvent.setup();
    const card = await screen.findByRole('heading', { name: 'Club starter' });
    expect(card).toBeInTheDocument();
    expect(
      screen.getByText(/Sets up 2 modules: lounge.starboard, lounge.sticky/),
    ).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Apply' }));
    const dialog = await screen.findByRole('dialog', { name: 'Apply Club starter' });
    expect(within(dialog).getByText('lounge.starboard')).toBeInTheDocument();
    expect(within(dialog).getByText('Turns on')).toBeInTheDocument();
    expect(within(dialog).getByText('Turns off')).toBeInTheDocument();
    expect(within(dialog).getByText('Sets threshold')).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'Apply Club starter' })).toBeNull(),
    );
    expect(await screen.findByText('Applied to this server')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Apply again' })).toBeInTheDocument();
  });

  it('previews packs without offering to add them, and lists a refusal', async () => {
    server.use(
      http.get('*/api/v1/packages', ({ request }) =>
        HttpResponse.json(
          new URL(request.url).searchParams.get('kind') === 'emoji_pack'
            ? [
                pkg({
                  id: '20',
                  kind: 'emoji_pack',
                  name: 'Faces',
                  version_id: '21',
                  payload: { items: [{ name: 'wave', file: 'emoji/wave.png' }] },
                }),
              ]
            : [],
        ),
      ),
      http.get('*/api/v1/guilds/g1/packages', () => HttpResponse.json([])),
    );
    await openSettings('extensions');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('combobox', { name: 'Kind' }));
    await user.click(await screen.findByRole('option', { name: 'Emoji packs' }));
    expect(await screen.findByRole('heading', { name: 'Faces' })).toBeInTheDocument();
    expect(screen.getByText('Browse only for now')).toBeInTheDocument();
    const img = screen.getByRole('img', { name: 'wave' });
    expect(img).toHaveAttribute('src', '/api/v1/packages/versions/21/files/emoji/wave.png');
    expect(screen.queryByRole('button', { name: 'Apply' })).toBeNull();
  });

  it('creates a server from a template package', async () => {
    const made = capture();
    server.use(
      http.get('*/api/v1/packages', ({ request }) =>
        HttpResponse.json(
          new URL(request.url).searchParams.get('kind') === 'server_template'
            ? [pkg({ id: '30', kind: 'server_template', name: 'Anime club', payload: null })]
            : [],
        ),
      ),
      http.get('*/api/v1/guilds/g1/packages', () => HttpResponse.json([])),
      http.post('*/api/v1/packages/30/create-guild', async ({ request }) => {
        await made.record(request);
        return HttpResponse.json({ id: 'g1' }, { status: 201 });
      }),
    );
    await openSettings('extensions');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('combobox', { name: 'Kind' }));
    await user.click(await screen.findByRole('option', { name: 'Server templates' }));
    await user.click(await screen.findByRole('button', { name: 'Create a server' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create a server from Anime club' });
    const name = within(dialog).getByLabelText('Server name');
    await user.clear(name);
    await user.type(name, 'Weeb Hub');
    await user.click(within(dialog).getByRole('button', { name: 'Create a server' }));
    await waitFor(() => expect(made.calls.map((c) => c.body)).toEqual([{ name: 'Weeb Hub' }]));
  });
});
