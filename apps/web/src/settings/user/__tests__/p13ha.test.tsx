import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, resetPrefs, signedIn } from './helpers';

beforeEach(() => {
  resetPrefs();
  signedIn();
});

describe('Reading audience (H-17, D-229)', () => {
  it('shows the stored level, explains it and saves a change as library.share_reading', async () => {
    const patches: unknown[] = [];
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ version: 1, settings: { library: { share_reading: 'everyone' } } }),
      ),
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        const body = (await request.json()) as { settings: Record<string, unknown> };
        patches.push(body.settings);
        return HttpResponse.json({ version: 2, settings: body.settings });
      }),
    );
    const user = userEvent.setup();
    openSettings('activity');
    const level = await screen.findByRole('combobox', { name: 'Who can see what I am reading' });
    await waitFor(() => expect(level).toHaveTextContent('Friends and people in my servers'));
    expect(screen.getByText(/people who share a server with you see what you are reading/)).toBeVisible();
    await user.click(level);
    await user.click(await screen.findByRole('option', { name: 'Nobody' }));
    await waitFor(() => expect(patches).toEqual([{ library: { share_reading: 'off' } }]));
    expect(await screen.findByText('Nobody sees what you read.')).toBeVisible();
  });
});
