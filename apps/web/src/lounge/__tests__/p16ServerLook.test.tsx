import { act, screen, waitFor } from '@testing-library/react';
import { http, HttpResponse, server } from '../../test/server';
import { useGuildAppearances } from '../../packages/serverAppearance';
import { useStoreThemes } from '../../packages/themes';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const root = () => document.querySelector<HTMLElement>('[data-section="lounge"]');

describe('D-280 server look precedence in the Lounge shell', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    useGuildAppearances.setState({ byGuild: {} });
    useStoreThemes.setState({ themes: [], preview: null, loaded: true });
    baseHandlers();
    seedLounge({ messages: [] });
    server.use(
      http.get('*/api/v1/guilds/g1/appearance', () =>
        HttpResponse.json({
          accent_color: 0x336699,
          theme: {
            package_id: '1',
            version_id: '2',
            slug: 'dusk',
            name: 'Dusk',
            scheme: 'dark',
            tokens: { '--ui-bg': '#222222' },
          },
        }),
      ),
    );
  });
  afterEach(() => useStoreThemes.setState({ preview: null }));

  it('lays a Theme Store preview over the server theme and accent', async () => {
    at('/channels/g1/c1');
    await screen.findByRole('navigation', { name: 'Servers' });
    await waitFor(() => expect(root()?.style.getPropertyValue('--ui-bg')).toBe('#222222'));
    expect(root()?.style.getPropertyValue('--ui-accent')).toBe('#336699');
    act(() =>
      useStoreThemes.getState().setPreview({
        packageId: '9',
        name: 'Ocean',
        section: 'all',
        scheme: 'dark',
        tokens: { '--ui-bg': '#001122' },
      }),
    );
    await waitFor(() => expect(root()?.style.getPropertyValue('--ui-bg')).toBe('#001122'));
    expect(root()?.style.getPropertyValue('--ui-accent')).not.toBe('#336699');
    act(() => useStoreThemes.getState().setPreview(null));
    await waitFor(() => expect(root()?.style.getPropertyValue('--ui-bg')).toBe('#222222'));
  });
});
