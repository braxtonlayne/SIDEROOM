import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const pkg = (over: object) => ({
  id: '40',
  kind: 'theme',
  slug: 'dusk',
  name: 'Dusk',
  description: 'Warm and dark.',
  author_id: '9',
  author_name: 'Ada',
  version_id: '41',
  version: '1.0.0',
  size_bytes: 300,
  updated_at_ms: 0,
  payload: { scheme: 'dark', tokens: { '--ui-bg': '#222' } },
  ...over,
});

const enablement = (over: object) => ({
  package_id: '40',
  version_id: '41',
  kind: 'theme',
  slug: 'dusk',
  name: 'Dusk',
  version: '1.0.0',
  section: 'all',
  config: {},
  enabled_at_ms: 0,
  update_available: false,
  payload: null,
  ...over,
});

const look = (accent: number | null) =>
  http.get('*/api/v1/guilds/g1/appearance', () =>
    HttpResponse.json({ accent_color: accent, theme: null }),
  );

describe('Server Settings > Extensions: server theme, accent colour and packs (LNG-SSET-050)', () => {
  beforeEach(() => seed({ perms: [MANAGE.guild] }));

  it('chooses a server-wide theme from the approved themes', async () => {
    const put = capture();
    let on: object[] = [];
    server.use(
      look(null),
      http.get('*/api/v1/packages', ({ request }) =>
        HttpResponse.json(
          new URL(request.url).searchParams.get('kind') === 'theme' ? [pkg({})] : [],
        ),
      ),
      http.get('*/api/v1/guilds/g1/packages', () => HttpResponse.json(on)),
      http.put('*/api/v1/guilds/g1/packages/40', async ({ request }) => {
        await put.record(request);
        on = [enablement({})];
        return HttpResponse.json(on[0]);
      }),
    );
    await openSettings('extensions');
    const user = userEvent.setup();
    expect(await screen.findByText('Built-in look')).toBeInTheDocument();
    await user.click(await screen.findByRole('combobox', { name: 'Kind' }));
    await user.click(await screen.findByRole('option', { name: 'Themes' }));
    await user.click(await screen.findByRole('button', { name: 'Use Dusk for this server' }));
    await waitFor(() => expect(put.calls.map((c) => c.body)).toEqual([{ section: 'all' }]));
    expect(await screen.findByRole('button', { name: 'Stop using' })).toBeInTheDocument();
  });

  it('clears the server accent colour', async () => {
    const patch = capture();
    server.use(
      look(0x336699),
      http.get('*/api/v1/packages', () => HttpResponse.json([])),
      http.get('*/api/v1/guilds/g1/packages', () => HttpResponse.json([])),
      http.patch('*/api/v1/guilds/g1/appearance', async ({ request }) => {
        await patch.record(request);
        return HttpResponse.json({ accent_color: null, theme: null });
      }),
    );
    await openSettings('extensions');
    const user = userEvent.setup();
    const clear = await screen.findByRole('button', { name: 'Clear accent colour' });
    await waitFor(() => expect(clear).toBeEnabled());
    await user.click(clear);
    await waitFor(() => expect(patch.calls.map((c) => c.body)).toEqual([{ accent_color: null }]));
  });

  it('adds a pack to the server and lists what did not fit', async () => {
    const post = capture();
    server.use(
      look(null),
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
      http.post('*/api/v1/guilds/g1/packages/20/import', async ({ request }) => {
        await post.record(request);
        return HttpResponse.json({
          enablement: enablement({ package_id: '20', kind: 'emoji_pack' }),
          added: [{ name: 'wave', id: '5' }],
          skipped: [{ name: 'smile', reason: 'The server already has an emoji with this name.' }],
        });
      }),
    );
    await openSettings('extensions');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('combobox', { name: 'Kind' }));
    await user.click(await screen.findByRole('option', { name: 'Emoji packs' }));
    await user.click(await screen.findByRole('button', { name: 'Add Faces to this server' }));
    expect(await screen.findByText('Faces was added')).toBeInTheDocument();
    expect(screen.getByText('1 item was added.')).toBeInTheDocument();
    const skipped = screen.getByRole('list', { name: 'Items that were not added' });
    expect(within(skipped).getByText(/already has an emoji/)).toBeInTheDocument();
    expect(post.calls).toHaveLength(1);
  });
});
