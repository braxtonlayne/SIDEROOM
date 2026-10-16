import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, resetPrefs, signedIn } from './helpers';

beforeEach(() => {
  resetPrefs();
  signedIn();
});

const github = {
  type: 'github',
  id: '3',
  name: 'alice-gh',
  verified: true,
  url: 'https://github.com/alice-gh',
  visible: true,
  show_activity: false,
  stats: {},
};
const providers = [
  { id: 'github', name: 'GitHub', verified: true, available: true, requires_url: false },
  { id: 'twitch', name: 'Twitch', verified: true, available: false, requires_url: false },
  { id: 'website', name: 'Website', verified: false, available: true, requires_url: true },
];

describe('Connections (LNG-USET-023)', () => {
  it('lists connections and providers, toggles the stats switch and removes one', async () => {
    const patches: unknown[] = [];
    let removed = false;
    server.use(
      http.get('*/api/v1/users/@me/connections', () =>
        HttpResponse.json({ connections: removed ? [] : [github], providers }),
      ),
      http.patch('*/api/v1/users/@me/connections/github/3', async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        patches.push(body);
        return HttpResponse.json({ ...github, ...body });
      }),
      http.delete('*/api/v1/users/@me/connections/github/3', () => {
        removed = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    openSettings('connections');
    expect(await screen.findByRole('link', { name: 'alice-gh' })).toBeInTheDocument();
    expect(screen.getByText('Not set up on this server')).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
    await user.click(screen.getByRole('switch', { name: 'Show stats (followers and the like)' }));
    await waitFor(() => expect(patches).toEqual([{ show_activity: true }]));
    await user.click(screen.getByRole('button', { name: /Remove\s+alice-gh/ }));
    expect(await screen.findByText('You have not connected any accounts yet.')).toBeInTheDocument();
  });

  it('adds a link-only provider with its address', async () => {
    let body: unknown;
    server.use(
      http.get('*/api/v1/users/@me/connections', () =>
        HttpResponse.json({ connections: [], providers }),
      ),
      http.put('*/api/v1/users/@me/connections/link', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ ...github, type: 'website' });
      }),
    );
    const user = userEvent.setup();
    openSettings('connections');
    await user.click(await screen.findByRole('button', { name: /Connect\s+Website/ }));
    await user.type(screen.getByLabelText('Profile address (https://)'), 'https://example.com');
    await user.click(screen.getByRole('button', { name: 'Add link' }));
    await waitFor(() => expect(body).toEqual({ provider: 'website', url: 'https://example.com' }));
  });
});

describe('Request my data (LNG-USET-019)', () => {
  it('requests a package and shows it waiting', async () => {
    let requested = false;
    server.use(
      http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/data-export', () =>
        HttpResponse.json(requested ? { state: 'building' } : { state: 'none' }),
      ),
      http.post('*/api/v1/users/@me/data-export', () => {
        requested = true;
        return HttpResponse.json({ state: 'pending', requested_at: '2026-05-01T00:00:00Z' });
      }),
    );
    const user = userEvent.setup();
    openSettings('privacy');
    await user.click(await screen.findByRole('button', { name: 'Request my data' }));
    expect(await screen.findByText('Your request is waiting in line.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Request my data again' })).toBeDisabled();
  });

  it('shows the download link of a ready package', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/data-export', () =>
        HttpResponse.json({
          state: 'ready',
          id: '9',
          size_bytes: 2048,
          expires_at: '2026-06-01T00:00:00Z',
          download_url: '/users/@me/data-export/9/download',
        }),
      ),
    );
    openSettings('privacy');
    const link = await screen.findByRole('link', { name: 'Download your data' });
    expect(link).toHaveAttribute('href', '/api/v1/users/@me/data-export/9/download');
    expect(within(link.parentElement as HTMLElement).getByText(/2 KB/)).toBeInTheDocument();
  });
});
