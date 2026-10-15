// User Settings > Profiles > Server tag picker (LNG-SSET-012).
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { ServerTagCard } from '../sections/ServerTagCard';

const guilds = [
  { id: 'g1', name: 'Book Club' },
  { id: 'g2', name: 'Quiet Place' },
  { id: 'g3', name: 'No Display' },
];

function endpoints(current: unknown = null) {
  const puts: unknown[] = [];
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json(guilds)),
    http.get('*/api/v1/guilds/g1/tag', () =>
      HttpResponse.json({ tag: 'bk', badge: 0, color: 255, members_can_display: true }),
    ),
    http.get('*/api/v1/guilds/g2/tag', () => HttpResponse.json({}, { status: 404 })),
    http.get('*/api/v1/guilds/g3/tag', () =>
      HttpResponse.json({ tag: 'nd', badge: 1, color: 0, members_can_display: false }),
    ),
    http.get('*/api/v1/users/@me/primary-guild', () =>
      HttpResponse.json({ primary_guild: current }),
    ),
    http.put('*/api/v1/users/@me/primary-guild', async ({ request }) => {
      puts.push(await request.json());
      return HttpResponse.json({ primary_guild: null });
    }),
  );
  return puts;
}

describe('ServerTagCard', () => {
  it('offers only servers with a tag members may display and saves the choice', async () => {
    const puts = endpoints();
    render(<ServerTagCard />);
    const box = await screen.findByRole('combobox', { name: 'Tag to display' });
    await userEvent.click(box);
    expect(screen.getAllByRole('option').map((o) => o.textContent)).toEqual([
      expect.stringContaining('No tag'),
      expect.stringContaining('Book Club'),
    ]);
    await userEvent.click(screen.getByRole('option', { name: /Book Club/ }));
    await waitFor(() => expect(puts).toEqual([{ guild_id: 'g1' }]));
    expect(await screen.findByText('Saved.')).toBeInTheDocument();
    expect(screen.getByText(/BK/, { selector: '.ss-tag-chip' })).toBeInTheDocument();
  });

  it('shows the tag already in use and clears it with No tag', async () => {
    const puts = endpoints({
      identity_guild_id: 'g1',
      identity_enabled: true,
      tag: 'bk',
      badge: 0,
      color: 255,
    });
    render(<ServerTagCard />);
    const box = await screen.findByRole('combobox', { name: 'Tag to display' });
    expect(box).toHaveTextContent('Book Club');
    await userEvent.click(box);
    await userEvent.click(screen.getByRole('option', { name: 'No tag' }));
    await waitFor(() => expect(puts).toEqual([{ guild_id: null }]));
  });

  it('says so when no server has a tag', async () => {
    server.use(
      http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([guilds[1]])),
      http.get('*/api/v1/guilds/g2/tag', () => HttpResponse.json({}, { status: 404 })),
      http.get('*/api/v1/users/@me/primary-guild', () =>
        HttpResponse.json({ primary_guild: null }),
      ),
    );
    render(<ServerTagCard />);
    expect(await screen.findByText(/None of your servers has a tag/)).toBeInTheDocument();
  });
});
