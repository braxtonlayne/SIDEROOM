// "Watch together" with a link instead of a Cinema title (PRF-UNF-001): a pasted YouTube/Vimeo link or media file,
// and the card's own address. The server validates and canonicalises; the client sends what it was given.
import { ThemeProvider } from '@sideroom/ui';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { vi } from 'vitest';
import { useLoungeUi } from '../../lounge/store';
import { http, HttpResponse, server } from '../../test/server';
import { StartPartyDialog } from '../StartPartyDialog';

vi.mock('../../store/connect', async (orig) => ({
  ...(await orig<typeof import('../../store/connect')>()),
  holdGateway: () => () => undefined,
}));

vi.mock('../../lounge/data', () => ({
  useGuilds: () => [],
  useAllChannels: () => ({}),
  useDms: () => [],
  useUsers: () => ({}),
}));

function mount(ui: React.ReactNode) {
  const root = createRootRoute({
    component: () => <ThemeProvider section="cinema">{ui}</ThemeProvider>,
  });
  const history = createMemoryHistory({ initialEntries: ['/'] });
  const router = createRouter({ routeTree: root, history });
  return { history, ...render(<RouterProvider router={router} />) };
}

const created: unknown[] = [];
function onCreate(status = 201, body: Record<string, unknown> = { id: '900' }) {
  created.length = 0;
  server.use(
    http.post('*/api/v1/watch-parties', async ({ request }) => {
      created.push(await request.json());
      return HttpResponse.json(body, { status });
    }),
  );
}

beforeEach(() => {
  localStorage.removeItem('sr.watchparty.lastDest');
  useLoungeUi.setState({ lastLocation: null });
});

describe('a pasted link', () => {
  it('starts a private party with the link, no title, and opens the theatre', async () => {
    onCreate();
    const onClose = vi.fn();
    const view = mount(<StartPartyDialog open onClose={onClose} />);
    const start = await screen.findByRole('button', { name: 'Start party' });
    expect(start).toBeDisabled();
    await userEvent.type(
      screen.getByRole('textbox', { name: /paste a YouTube or Vimeo link/i }),
      'https://youtu.be/dQw4w9WgXcQ',
    );
    await userEvent.click(start);
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toEqual({
      external: { url: 'https://youtu.be/dQw4w9WgXcQ' },
      channel_id: null,
    });
    await waitFor(() => expect(view.history.location.pathname).toBe('/cinema/party/900'));
    expect(onClose).toHaveBeenCalled();
  });

  it('shows the server reason when it refuses the link', async () => {
    onCreate(400, { code: 50035, message: 'Only plain https links can be watched together.' });
    mount(<StartPartyDialog open onClose={() => undefined} />);
    await userEvent.type(
      await screen.findByRole('textbox', { name: /paste a YouTube or Vimeo link/i }),
      'http://example.org/a.mp4',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Start party' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/plain https links/);
  });
});

describe('a card link', () => {
  it('sends the card address and its title, and asks for no title', async () => {
    onCreate();
    mount(
      <StartPartyDialog
        open
        onClose={() => undefined}
        external={{ url: 'https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ', label: 'A song' }}
      />,
    );
    expect(screen.queryByRole('textbox', { name: /paste a YouTube/i })).toBeNull();
    await userEvent.click(await screen.findByRole('button', { name: 'Start party' }));
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]).toEqual({
      external: { url: 'https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ', title: 'A song' },
      channel_id: null,
    });
  });
});
