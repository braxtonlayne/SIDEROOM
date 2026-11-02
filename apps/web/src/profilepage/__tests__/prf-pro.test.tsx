import { render, screen } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { resetStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { signedIn } from '../../settings/user/__tests__/helpers';
import { Inspector } from '../Inspector';
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
      http.get('*/api/v1/users/2/vouches', () =>
        HttpResponse.json({
          count: 60,
          tier: 'pillar',
          multiplier: 1.5,
          pillar_min: 50,
          elder_min: 500,
          pillar_multiplier: 1.5,
          elder_multiplier: 3,
          viewer_vouched: false,
          can_vouch: true,
          is_self: false,
        }),
      ),
    );
    const router = createAppRouter(createMemoryHistory({ initialEntries: ['/~bob'] }));
    render(<RouterProvider router={router} />);
    expect(await screen.findByText('short address works')).toBeInTheDocument();
    // MOD-VCH-001: the vouch panel sits on the profile, with the tier badge and the Vouch button.
    expect(await screen.findByRole('button', { name: 'Vouch tier: Pillar' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Vouch' })).toBeInTheDocument();
  });
});

describe('tip QR codes in the Links block editor (FU-ARC-5)', () => {
  const linktree = (links: { label: string; url: string; tip?: string }[]): Block => ({
    id: 'l',
    type: 'linktree',
    x: 0,
    y: 0,
    w: 6,
    h: 3,
    config: { links },
  });

  it('draws a bitcoin link as a QR code with its text, and an https link without one', () => {
    render(
      <Inspector
        block={linktree([
          {
            label: 'Tip me',
            url: 'bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq',
            tip: 'btc',
          },
          { label: 'Site', url: 'https://example.com' },
        ])}
        friends={[]}
        onConfig={() => {}}
      />,
    );
    expect(screen.getByRole('img', { name: 'QR code for Tip me' })).toBeInTheDocument();
    expect(
      screen.getByText('bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq'),
    ).toBeInTheDocument();
    expect(screen.queryByRole('img', { name: 'QR code for Site' })).toBeNull();
    expect(screen.getAllByRole('img')).toHaveLength(1);
  });

  it('keeps the text when the payload is longer than the encoder takes', () => {
    const long = `lightning:${'lnbc1'.padEnd(400, 'q')}`;
    render(
      <Inspector
        block={linktree([{ label: 'Zap', url: long, tip: 'lightning' }])}
        friends={[]}
        onConfig={() => {}}
      />,
    );
    expect(screen.queryByRole('img')).toBeNull();
    expect(screen.getByText(long)).toBeInTheDocument();
  });
});
