import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { createPublicRouter } from '../../test/publicRouter';
import { http, HttpResponse, server } from '../../test/server';

const user = { id: '2', username: 'bob', display_name: 'Bob', avatar: null, banner: null };
const style = {
  palette: { background: '#1e1f22', panel: '#2b2d31', text: '#f2f3f5', accent: '#5865f2' },
  pattern: 'none',
  banner_background: false,
  font: 'inter',
  cursor: 'default',
  overlay: 'none',
  border: 'none',
};
const blk = (id: string, type: string, config: Record<string, unknown> = {}, y = 0) => ({
  id,
  type,
  x: 0,
  y,
  w: 12,
  h: 3,
  config,
});

const body = (over: Record<string, unknown> = {}) => ({
  page: {
    user,
    doc: {
      version: 1,
      style,
      blocks: [
        blk('h', 'header', { tagline: 'hello' }),
        blk('l', 'linktree', { links: [{ label: 'Site', url: 'https://example.com' }] }, 3),
      ],
    },
    friends: [],
    is_owner: false,
    can_sign: false,
    customised: true,
    guestbook_limit: 500,
    ...over,
  },
  guestbook: null,
  accepts_guestbook: false,
});

function at(path: string) {
  const router = createPublicRouter(path, createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
}

describe('the public profile surface (PRF-PRO-001)', () => {
  it('renders exactly the page the server returned, and has no axe violations', async () => {
    server.use(http.get('*/api/v1/public/profiles/bob', () => HttpResponse.json(body())));
    at('/~bob');
    expect(await screen.findByRole('link', { name: 'Site' })).toHaveAttribute(
      'href',
      'https://example.com/',
    );
    expect(screen.getByText('Bob')).toBeInTheDocument();
    expect(screen.getByText('hello')).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('answers a handle with no public page with the same "nothing here" for every reason', async () => {
    server.use(
      http.get('*/api/v1/public/profiles/*', () =>
        HttpResponse.json({ code: 10000, message: 'Unknown profile' }, { status: 404 }),
      ),
    );
    at('/~ghost');
    expect(await screen.findByRole('heading', { name: 'Nothing here' })).toBeInTheDocument();
    // A signed-in member is offered the sign-in door, not the profile.
    expect(screen.getByRole('link', { name: /member access/i })).toHaveAttribute(
      'href',
      '/login?next=%2Fu%2Fghost',
    );
  });

  it('shows approved guestbook notes and holds a visitor note for the owner', async () => {
    const posted: string[] = [];
    server.use(
      http.get('*/api/v1/public/profiles/bob', () =>
        HttpResponse.json({
          ...body({
            doc: { version: 1, style, blocks: [blk('g', 'guestbook')] },
          }),
          guestbook: [
            { id: '1', author: null, body: 'hi from outside', created_at_ms: 1, pending: false },
          ],
          accepts_guestbook: true,
        }),
      ),
      http.post('*/api/v1/public/profiles/bob/guestbook', async ({ request }) => {
        posted.push(((await request.json()) as { body: string }).body);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/~bob');
    expect(await screen.findByText('hi from outside')).toBeInTheDocument();
    expect(screen.getByText('A visitor')).toBeInTheDocument();
    const u = userEvent.setup();
    await u.type(screen.getByLabelText('Leave a note'), 'lovely page');
    await u.click(screen.getByRole('button', { name: 'Send note' }));
    await waitFor(() => expect(posted).toEqual(['lovely page']));
    expect(await screen.findByText(/waiting for the owner/)).toBeInTheDocument();
  });

  it('offers no guestbook form when the member did not opt in', async () => {
    server.use(
      http.get('*/api/v1/public/profiles/bob', () =>
        HttpResponse.json({
          ...body({ doc: { version: 1, style, blocks: [blk('g', 'guestbook')] } }),
          guestbook: [],
        }),
      ),
    );
    at('/~bob');
    expect(await screen.findByText('No notes yet.')).toBeInTheDocument();
    expect(screen.queryByLabelText('Leave a note')).toBeNull();
  });
});
