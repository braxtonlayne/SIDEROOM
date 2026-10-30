import { render, screen } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { resetStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { signedIn } from '../../settings/user/__tests__/helpers';
import { ProfileRenderer } from '../ProfileRenderer';
import { emptyDoc, type Block, type ProfilePage } from '../types';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const blk = (id: string, type: Block['type'], config: Block['config'] = {}): Block => ({
  id,
  type,
  x: 0,
  y: 0,
  w: 6,
  h: 3,
  config,
});

const page = (blocks: Block[], over: Partial<ProfilePage> = {}): ProfilePage => ({
  user: {
    id: '2',
    username: 'bob',
    display_name: 'Bob',
    avatar: null,
    banner: null,
    accent_color: null,
  },
  doc: { ...emptyDoc(), blocks },
  friends: [],
  is_owner: false,
  can_sign: false,
  customised: blocks.length > 0,
  ...over,
});

afterEach(() => resetStore());

describe('the new profile blocks in the app (PRF-PRO-001)', () => {
  it('draws links, featured servers and a typed now playing line', () => {
    render(
      <ProfileRenderer
        page={page(
          [
            blk('l', 'linktree', { links: [{ label: 'Site', url: 'https://example.com' }] }),
            blk('f', 'featured_guilds', { guild_ids: ['10'] }),
            blk('n', 'now_playing', { mode: 'manual', title: 'Blue' }),
          ],
          { featured_guilds: [{ id: '10', name: 'Book Club', icon: null }] },
        )}
      />,
    );
    expect(screen.getByRole('link', { name: 'Site' })).toHaveAttribute(
      'href',
      'https://example.com/',
    );
    expect(screen.getByText('Book Club')).toBeInTheDocument();
    expect(screen.getByText('Blue')).toBeInTheDocument();
  });

  it('serves /~handle as the same page as /u/handle', async () => {
    signedIn();
    server.use(
      http.get('*/api/v1/profile-pages/bob', () =>
        HttpResponse.json(page([blk('a', 'about', { text: 'short address works' })])),
      ),
    );
    const router = createAppRouter(createMemoryHistory({ initialEntries: ['/~bob'] }));
    render(<RouterProvider router={router} />);
    expect(await screen.findByText('short address works')).toBeInTheDocument();
  });
});
