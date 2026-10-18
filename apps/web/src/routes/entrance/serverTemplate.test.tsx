import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { warmLoungeRoute } from '../../test/warmRoutes';

warmLoungeRoute();

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

const template = {
  code: 'abc123',
  name: 'Book club',
  description: 'Chapters and spoilers',
  usage_count: 3,
  creator: { id: '5', username: 'ann' },
  serialized_source_guild: {
    roles: [{ local_id: 1, name: 'Mods' }],
    channels: [
      { local_id: 1, kind: 4, name: 'Text', position: 0, parent_local_id: null },
      { local_id: 2, kind: 0, name: 'general', position: 1, parent_local_id: 1 },
      { local_id: 3, kind: 0, name: 'spoilers', position: 2, parent_local_id: 1 },
    ],
  },
};

describe('server template page (/template/:code)', () => {
  beforeEach(() => {
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
      http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1' })),
    );
  });

  it('previews the template and creates a server from it', async () => {
    const posted: unknown[] = [];
    server.use(
      http.get('*/api/v1/guilds/templates/abc123', () => HttpResponse.json(template)),
      http.post('*/api/v1/guilds/templates/abc123', async ({ request }) => {
        posted.push(await request.json());
        return HttpResponse.json({ id: 'g9', channels: [{ id: 'c1', type: 0 }] }, { status: 201 });
      }),
    );
    const router = at('/template/abc123');
    expect(await screen.findByRole('heading', { name: 'Book club' })).toBeInTheDocument();
    expect(screen.getByText('Chapters and spoilers')).toBeInTheDocument();
    expect(screen.getByText(/2 channels · 1 roles · used 3 times · by ann/)).toBeInTheDocument();
    expect(screen.getByRole('list', { name: 'Channels in this template' })).toHaveTextContent(
      '#general',
    );
    expect(await axeViolations(document.body)).toEqual([]);
    const name = screen.getByLabelText('Server name');
    await userEvent.clear(name);
    await userEvent.type(name, 'Our club');
    await userEvent.click(screen.getByRole('button', { name: 'Create server' }));
    await waitFor(() => expect(posted).toEqual([{ name: 'Our club' }]));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g9/c1'));
  });

  it('says so when the template does not exist', async () => {
    server.use(
      http.get('*/api/v1/guilds/templates/gone', () =>
        HttpResponse.json({ message: 'Unknown Guild Template', code: 10057 }, { status: 404 }),
      ),
    );
    at('/template/gone');
    expect(await screen.findByRole('alert')).toHaveTextContent('does not exist or was deleted');
  });

  it('sends a signed-out visitor to log in and back', async () => {
    server.use(
      http.get('*/api/v1/users/@me', () => HttpResponse.json({ message: 'no' }, { status: 401 })),
    );
    at('/template/abc123');
    const link = await screen.findByRole('link', { name: 'Log in' });
    expect(link).toHaveAttribute('href', expect.stringContaining('next=%2Ftemplate%2Fabc123'));
  });
});
