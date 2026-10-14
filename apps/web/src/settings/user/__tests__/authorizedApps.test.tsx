import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, resetPrefs, signedIn } from './helpers';

const GRANTS = [
  {
    id: 'g1',
    application: { id: '500', name: 'Jukebox', icon: null },
    scopes: ['identify', 'guilds'],
    created_at: '2026-10-01T12:00:00Z',
  },
  {
    id: 'g2',
    application: { id: '501', name: 'Pinger', icon: null },
    scopes: ['email'],
    created_at: '2026-10-02T12:00:00Z',
  },
];

beforeEach(() => {
  resetPrefs();
  signedIn();
});

describe('Authorized Apps', () => {
  it('lists authorizations with what each app can do', async () => {
    server.use(http.get('*/api/v1/oauth2/tokens', () => HttpResponse.json(GRANTS)));
    openSettings('apps');
    const jukebox = within(await screen.findByRole('list', { name: 'Jukebox can' }));
    expect(jukebox.getByText('Access your username, avatar and account id')).toBeInTheDocument();
    expect(jukebox.getByText('Know which servers you are in')).toBeInTheDocument();
    expect(screen.getByRole('list', { name: 'Pinger can' })).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('says there is nothing authorized', async () => {
    server.use(http.get('*/api/v1/oauth2/tokens', () => HttpResponse.json([])));
    openSettings('apps');
    expect(await screen.findByText('You have not authorized any apps.')).toBeInTheDocument();
  });

  it('deauthorizes only after confirming', async () => {
    const user = userEvent.setup();
    server.use(http.get('*/api/v1/oauth2/tokens', () => HttpResponse.json(GRANTS)));
    let deleted = '';
    server.use(
      http.delete('*/api/v1/oauth2/tokens/:id', ({ params }) => {
        deleted = String(params.id);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openSettings('apps');
    await user.click(await screen.findByRole('button', { name: /Deauthorize\s*Jukebox/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Deauthorize Jukebox?' });
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(deleted).toBe('');
    await user.click(screen.getByRole('button', { name: /Deauthorize\s*Jukebox/ }));
    await user.click(await screen.findByRole('button', { name: 'Deauthorize' }));
    await waitFor(() => expect(deleted).toBe('g1'));
    await waitFor(() =>
      expect(screen.queryByRole('list', { name: 'Jukebox can' })).not.toBeInTheDocument(),
    );
    expect(screen.getByRole('list', { name: 'Pinger can' })).toBeInTheDocument();
  });

  it('shows why a removal failed and keeps the app listed', async () => {
    const user = userEvent.setup();
    server.use(
      http.get('*/api/v1/oauth2/tokens', () => HttpResponse.json(GRANTS)),
      http.delete('*/api/v1/oauth2/tokens/:id', () =>
        HttpResponse.json({ message: 'Try later.' }, { status: 500 }),
      ),
    );
    openSettings('apps');
    await user.click(await screen.findByRole('button', { name: /Deauthorize\s*Pinger/ }));
    await user.click(await screen.findByRole('button', { name: 'Deauthorize' }));
    expect(await screen.findByText("Couldn't remove the app")).toBeInTheDocument();
    expect(screen.getByRole('list', { name: 'Pinger can' })).toBeInTheDocument();
  });
});
