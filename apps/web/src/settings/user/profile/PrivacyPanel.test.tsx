import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { PrivacyPanel } from './PrivacyPanel';
import { allowedTiers, type Policy, type PrivacyView } from './api';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const policy = (over: Partial<Policy> = {}): Policy => ({
  page_tier: 'only_me',
  links_tier: 'only_me',
  top8_tier: 'only_me',
  now_playing_tier: 'only_me',
  bio_tier: 'only_me',
  guestbook_tier: 'only_me',
  media_tier: 'only_me',
  ring_tier: 'only_me',
  guestbook_accepts_public: false,
  ...over,
});

const view = (over: Partial<PrivacyView> = {}): PrivacyView => ({
  preset: 'private',
  stored: policy(),
  effective: policy(),
  public_possible: false,
  ...over,
});

const pageFor = (blocks: unknown[]) => ({
  user: { id: '2', username: 'me', display_name: 'Me', avatar: null, banner: null, accent_color: null },
  doc: {
    version: 1,
    style: {
      palette: { background: '#1e1f22', panel: '#2b2d31', text: '#f2f3f5', accent: '#5865f2' },
      pattern: 'none',
      banner_background: false,
      font: 'inter',
      cursor: 'default',
      overlay: 'none',
      border: 'none',
    },
    blocks,
  },
  friends: [],
  is_owner: false,
  can_sign: false,
  customised: blocks.length > 0,
});

describe('the clamp helper', () => {
  it('offers a block only the page tier and the stricter ones, never a looser one', () => {
    expect(allowedTiers('friends')).toEqual(['friends', 'only_me']);
    expect(allowedTiers('public')).toHaveLength(5);
    expect(allowedTiers('only_me')).toEqual(['only_me']);
  });
});

describe('Who can see my profile (PRF-PRO-001)', () => {
  it('shows the walled-garden notice and the search-engine sentence while public pages are off', async () => {
    server.use(
      http.get('*/api/v1/users/@me/profile-privacy', () => HttpResponse.json(view())),
      http.get('*/api/v1/users/@me/profile-page/preview', () => new HttpResponse(null, { status: 404 })),
    );
    render(<PrivacyPanel />);
    expect(await screen.findByText('Public pages are off on this server')).toBeInTheDocument();
    expect(screen.getByText('Search engines never index SIDE ROOM profiles.')).toBeInTheDocument();
  });

  it('saves a preset by name', async () => {
    const puts: unknown[] = [];
    server.use(
      http.get('*/api/v1/users/@me/profile-privacy', () => HttpResponse.json(view())),
      http.get('*/api/v1/users/@me/profile-page/preview', () => new HttpResponse(null, { status: 404 })),
      http.put('*/api/v1/users/@me/profile-privacy', async ({ request }) => {
        puts.push(await request.json());
        return HttpResponse.json(
          view({ preset: 'friends', stored: policy({ page_tier: 'friends' }) }),
        );
      }),
    );
    render(<PrivacyPanel />);
    const u = userEvent.setup();
    await u.click(await screen.findByRole('radio', { name: 'Friends only' }));
    await waitFor(() => expect(puts).toEqual([{ preset: 'friends' }]));
    expect(await screen.findByText('Only your friends can see your page.')).toBeInTheDocument();
  });

  it('renders the "view as" strip through the server filter and says when a viewer sees nothing', async () => {
    const asked: string[] = [];
    server.use(
      http.get('*/api/v1/users/@me/profile-privacy', () =>
        HttpResponse.json(view({ preset: 'members', stored: policy({ page_tier: 'members' }) })),
      ),
      http.get('*/api/v1/users/@me/profile-page/preview', ({ request }) => {
        const as = new URL(request.url).searchParams.get('as') ?? '';
        asked.push(as);
        if (as === 'public') return new HttpResponse(null, { status: 404 });
        return HttpResponse.json(
          pageFor([
            {
              id: 'a',
              type: 'about',
              x: 0,
              y: 0,
              w: 6,
              h: 3,
              config: { text: 'only members read this' },
            },
          ]),
        );
      }),
    );
    render(<PrivacyPanel />);
    expect(
      await screen.findByText('This viewer sees nothing: your page is not open to them.'),
    ).toBeInTheDocument();
    const u = userEvent.setup();
    await u.click(screen.getByRole('radio', { name: 'Member' }));
    const region = await screen.findByRole('region', { name: 'Your page as that viewer sees it' });
    expect(await within(region).findByText('only members read this')).toBeInTheDocument();
    expect(asked).toEqual(['public', 'member']);
  });
});
