// Server Settings > Server Tag (LNG-SSET-011).
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

describe('Server Tag settings', () => {
  it('starts empty, validates the text and saves with the badge and the display switch', async () => {
    seed({ perms: [MANAGE.guild] });
    let body: unknown;
    server.use(
      http.get('*/api/v1/guilds/g1/tag', () =>
        HttpResponse.json({ code: 10003, message: 'Unknown' }, { status: 404 }),
      ),
      http.put('*/api/v1/guilds/g1/tag', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(body as Record<string, unknown>);
      }),
    );
    await openSettings('tag');
    const input = await screen.findByLabelText('Tag');
    const save = screen.getByRole('button', { name: 'Save tag' });
    expect(save).toBeDisabled();
    await userEvent.type(input, 'a');
    expect(await screen.findByText('Use 2 to 4 letters or numbers.')).toBeInTheDocument();
    expect(save).toBeDisabled();
    await userEvent.type(input, 'bc');
    expect(save).toBeEnabled();
    expect(screen.getByText(/ABC/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Badge 3' }));
    await userEvent.click(screen.getByRole('switch', { name: /Members can choose to display/ }));
    await userEvent.click(save);
    await waitFor(() =>
      expect(body).toMatchObject({ tag: 'abc', badge: 2, members_can_display: false }),
    );
    expect(await screen.findByText('Tag saved.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove tag' })).toBeInTheDocument();
  });

  it('removes an existing tag', async () => {
    seed({ perms: [MANAGE.guild] });
    let removed = false;
    server.use(
      http.get('*/api/v1/guilds/g1/tag', () =>
        HttpResponse.json({ tag: 'BOOK', badge: 1, color: 255, members_can_display: true }),
      ),
      http.delete('*/api/v1/guilds/g1/tag', () => {
        removed = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('tag');
    expect(await screen.findByDisplayValue('BOOK')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Remove tag' }));
    await waitFor(() => expect(removed).toBe(true));
    expect(await screen.findByText('Tag removed.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Remove tag' })).not.toBeInTheDocument();
  });

  it('is hidden from members without Manage Server', async () => {
    seed({ perms: [MANAGE.channels] });
    server.use(http.get('*/api/v1/guilds/g1/tag', () => HttpResponse.json({}, { status: 404 })));
    await openSettings('channels');
    await screen.findByRole('dialog');
    expect(screen.queryByRole('button', { name: 'Server Tag' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Server Tag' })).not.toBeInTheDocument();
  });
});
