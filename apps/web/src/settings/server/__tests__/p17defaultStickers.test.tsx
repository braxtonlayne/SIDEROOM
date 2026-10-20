// M-03 (D-289): Server Settings > Expressions > Stickers has "Hide default sticker packs".
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('Server settings > Expressions: default sticker packs', () => {
  it('shows the current choice and saves a change', async () => {
    seed({ perms: [MANAGE.expressions] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/emojis', () => HttpResponse.json([])),
      http.get('*/api/v1/guilds/g1/stickers', () => HttpResponse.json([])),
      http.get('*/api/v1/guilds/g1/default-sticker-packs', () =>
        HttpResponse.json({ hidden: false }),
      ),
      http.put('*/api/v1/guilds/g1/default-sticker-packs', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json({ hidden: true });
      }),
    );
    await openSettings('emoji');
    await userEvent.click(await screen.findByRole('tab', { name: 'Stickers' }));
    const sw = await screen.findByRole('switch', { name: 'Hide default sticker packs' });
    await waitFor(() => expect(sw).toBeEnabled());
    expect(sw).not.toBeChecked();
    await userEvent.click(sw);
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ hidden: true });
    await waitFor(() => expect(sw).toBeChecked());
  });
});
