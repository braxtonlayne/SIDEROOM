import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ForYou } from '../inbox/ForYou';
import { resetStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';

const navigate = vi.fn();
vi.mock('@tanstack/react-router', async (orig) => ({
  ...(await orig<typeof import('@tanstack/react-router')>()),
  useNavigate: () => navigate,
}));

beforeEach(() => {
  resetStore();
  navigate.mockClear();
  server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })));
});

const rel = (id: string, type: number, username: string) => ({
  id,
  type,
  since: '2026-01-01T00:00:00Z',
  user: { id, username, global_name: null, avatar: null },
});

describe('Inbox For You tab (LNG-NTF-006)', () => {
  it('collects friend requests, library updates, decisions and redeemed invites', async () => {
    let accepted = false;
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json([rel('2', 3, 'bob'), rel('3', 1, 'friendly')]),
      ),
      http.put('*/api/v1/users/@me/relationships/2', () => {
        accepted = true;
        return new HttpResponse(null, { status: 204 });
      }),
      http.get('*/api/v1/library/updates', () =>
        HttpResponse.json({
          items: [
            {
              chapter_id: '90',
              chapter_number: 12,
              published_at_ms: 1,
              language: 'en',
              series_id: '4',
              series_title: 'Blue Hour',
              kind: 'manga',
              age_restricted: false,
            },
          ],
        }),
      ),
      http.get('*/api/v1/submissions', () =>
        HttpResponse.json({
          items: [
            { id: 's1', title: 'My Comic', state: 'approved' },
            { id: 's2', title: 'Still Draft', state: 'draft' },
          ],
          total: 2,
          limit: 20,
          offset: 0,
        }),
      ),
      http.get('*/api/v1/sr/invites', () =>
        HttpResponse.json({
          credits: 1,
          open: [],
          redeemed: [
            {
              code: 'AAAAA-BBBBB',
              link: '/invite/x',
              note: '',
              created_at_ms: 1,
              redeemed_by: { id: '7', username: 'newbie' },
              redeemed_at_ms: 5,
            },
          ],
          blocked: null,
        }),
      ),
    );
    const onGo = vi.fn();
    const user = userEvent.setup();
    render(<ForYou onGo={onGo} />);
    expect(await screen.findByText('bob')).toBeInTheDocument();
    expect(screen.queryByText('friendly')).toBeNull();
    expect(screen.getByText('My Comic')).toBeInTheDocument();
    expect(screen.queryByText('Still Draft')).toBeNull();
    expect(screen.getByText('newbie')).toBeInTheDocument();

    await user.click(screen.getByText('Blue Hour'));
    expect(navigate).toHaveBeenCalledWith({ to: '/library/r/90' });
    expect(onGo).toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Accept the friend request from bob' }));
    await waitFor(() => expect(accepted).toBe(true));
    await waitFor(() => expect(screen.queryByText('bob')).toBeNull());
  });

  it('says so when there is nothing, and survives one part failing', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
      http.get('*/api/v1/library/updates', () =>
        HttpResponse.json({ message: 'no' }, { status: 500 }),
      ),
      http.get('*/api/v1/submissions', () =>
        HttpResponse.json({ items: [], total: 0, limit: 20, offset: 0 }),
      ),
      http.get('*/api/v1/sr/invites', () => HttpResponse.json({ message: 'no' }, { status: 500 })),
    );
    render(<ForYou onGo={() => undefined} />);
    expect(await screen.findByText('Nothing new for you right now.')).toBeInTheDocument();
  });
});
