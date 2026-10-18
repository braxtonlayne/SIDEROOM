import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { openSettings, resetPrefs, signedIn } from '../../settings/user/__tests__/helpers';
import { useStoreThemes } from '../themes';

const theme = (id: string, name: string, accent: string, scheme: 'light' | 'dark' = 'dark') => ({
  id,
  kind: 'theme',
  slug: name.toLowerCase().replace(/ /g, '-'),
  name,
  description: `${name} colours.`,
  author_id: '9',
  author_name: 'Ada',
  version_id: `${id}1`,
  version: '1.0.0',
  size_bytes: 300,
  updated_at_ms: 0,
  payload: {
    scheme,
    targets: ['lounge'],
    extends: null,
    tokens: { '--ui-accent': accent, '--ui-bg': '#101820' },
    css: '',
  },
});

const enablement = (id: string, name: string, section: string, accent: string) => ({
  package_id: id,
  version_id: `${id}1`,
  kind: 'theme',
  slug: name.toLowerCase(),
  name,
  version: '1.0.0',
  section,
  config: {},
  enabled_at_ms: 0,
  update_available: false,
  payload: { scheme: 'dark', tokens: { '--ui-accent': accent } },
});

const root = () => document.querySelector<HTMLElement>('[data-theme]')!;

beforeEach(() => {
  resetPrefs();
  signedIn();
  useStoreThemes.setState({ themes: [], preview: null, loaded: false });
});

describe('Theme Store (EXT-PKG-005)', () => {
  it('previews a theme on the whole interface, then applies it to a section', async () => {
    const puts: { id: string; body: unknown }[] = [];
    let mine: object[] = [];
    server.use(
      http.get('*/api/v1/packages', () =>
        HttpResponse.json([
          theme('10', 'Ocean Night', '#00aaff'),
          theme('20', 'Sunset', '#ff7700'),
        ]),
      ),
      http.get('*/api/v1/users/@me/packages', () => HttpResponse.json(mine)),
      http.put('*/api/v1/users/@me/packages/:id', async ({ request, params }) => {
        const body = (await request.json()) as { section: string };
        puts.push({ id: String(params.id), body });
        mine = [enablement(String(params.id), 'Ocean Night', body.section, '#00aaff')];
        return HttpResponse.json(mine[0]);
      }),
    );
    openSettings('appearance');
    const user = userEvent.setup();
    const card = (await screen.findByRole('heading', { name: 'Ocean Night' })).closest('li')!;
    expect(within(card).getByText('by Ada')).toBeInTheDocument();

    // Preview: the whole theme root takes the tokens at once; nothing is saved.
    await user.click(within(card).getByRole('button', { name: 'Preview Ocean Night' }));
    expect(
      await screen.findByText('Previewing Ocean Night. Nothing is saved yet.'),
    ).toBeInTheDocument();
    expect(root().style.getPropertyValue('--ui-accent')).toBe('#00aaff');
    expect(puts).toEqual([]);

    // Stopping puts the built-in colours back.
    await user.click(screen.getByRole('button', { name: 'Stop previewing' }));
    expect(root().style.getPropertyValue('--ui-accent')).not.toBe('#00aaff');

    // Apply to Cinema only.
    await user.click(screen.getByRole('combobox', { name: 'Apply to' }));
    await user.click(await screen.findByRole('option', { name: 'Cinema' }));
    await user.click(within(card).getByRole('button', { name: 'Apply Ocean Night' }));
    await waitFor(() => expect(puts).toEqual([{ id: '10', body: { section: 'cinema' } }]));
    const list = await screen.findByRole('list', { name: 'Your themes' });
    expect(within(list).getByText('Ocean Night')).toBeInTheDocument();
    expect(within(list).getByText('Cinema')).toBeInTheDocument();
    // A Cinema theme does not recolour the Lounge.
    expect(root().style.getPropertyValue('--ui-accent')).not.toBe('#00aaff');
    expect(within(card).getByRole('button', { name: 'Apply Ocean Night' })).toBeDisabled();
  });

  it('removes a theme and shows an update when a newer version is approved', async () => {
    const removed: string[] = [];
    const current = { ...enablement('10', 'Ocean', 'all', '#00aaff'), update_available: true };
    server.use(
      http.get('*/api/v1/packages', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/packages', () =>
        HttpResponse.json(removed.length ? [] : [current]),
      ),
      http.delete('*/api/v1/users/@me/packages/10', () => {
        removed.push('10');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openSettings('appearance');
    const user = userEvent.setup();
    const list = await screen.findByRole('list', { name: 'Your themes' });
    expect(within(list).getByText('Update available')).toBeInTheDocument();
    expect(within(list).getByRole('button', { name: 'Update' })).toBeInTheDocument();
    await user.click(within(list).getByRole('button', { name: 'Remove Ocean' }));
    await waitFor(() => expect(removed).toEqual(['10']));
    await waitFor(() => expect(screen.queryByRole('list', { name: 'Your themes' })).toBeNull());
    expect(await screen.findByText('No themes yet')).toBeInTheDocument();
  });

  it('applies a theme already turned on at boot, by section', async () => {
    server.use(
      http.get('*/api/v1/users/@me/packages', () =>
        HttpResponse.json([
          enablement('10', 'Everywhere', 'all', '#112233'),
          enablement('11', 'Cinema only', 'cinema', '#445566'),
        ]),
      ),
      http.get('*/api/v1/packages', () => HttpResponse.json([])),
    );
    openSettings('appearance');
    await waitFor(() => expect(root().style.getPropertyValue('--ui-accent')).toBe('#112233'));
  });
});
