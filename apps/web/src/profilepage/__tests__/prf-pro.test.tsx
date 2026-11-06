import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { resetStore } from '../../store';
import { useRingFeedStore } from '../../lounge/rings/store';
import { setReportsEnabled } from '../../moderation/reports';
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

describe('tip QR codes in the signed-in profile view (FU-18)', () => {
  const tips = blk('l', 'linktree', {
    links: [
      {
        label: 'Tip me',
        url: 'bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq',
        tip: 'btc',
      },
      { label: 'Ko-fi', url: 'https://ko-fi.com/someone', tip: 'kofi' },
    ],
  });

  it('draws Bitcoin and Lightning links as QR codes, with the text beside them', () => {
    render(<ProfileRenderer page={page([tips])} />);
    expect(screen.getByRole('img', { name: 'QR code for Tip me' })).toBeInTheDocument();
    expect(
      screen.getByText('bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq'),
    ).toBeInTheDocument();
    // Ko-fi is a page, not a payment URI: no code.
    expect(screen.getAllByRole('img')).toHaveLength(1);
  });

  it('keeps the mini popout text-only', () => {
    render(<ProfileRenderer page={page([tips])} compact />);
    expect(screen.queryByRole('img')).toBeNull();
    expect(
      screen.getByText('bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq'),
    ).toBeInTheDocument();
  });
});

describe('the ring on the profile page (D-504)', () => {
  const header = blk('h', 'header');
  const feed = () =>
    useRingFeedStore.setState({
      entries: [{ author_id: '2', all_seen: false, rings: [] }],
      loadedAt: Date.now(),
    });
  // A fresh empty feed, so no avatar left on screen asks the server for one.
  afterEach(() => useRingFeedStore.setState({ entries: null, loadedAt: Date.now() }));

  it('wraps the header picture in the ring when the page says the viewer may see it', () => {
    feed();
    render(<ProfileRenderer page={page([header], { ring_visible: true })} />);
    expect(screen.getByRole('button', { name: 'Bob has a new ring' })).toBeInTheDocument();
  });

  it('draws the plain picture when the page does not say so, even with a ring in the feed', () => {
    feed();
    render(<ProfileRenderer page={page([header])} />);
    expect(screen.queryByRole('button', { name: /has a (new )?ring/ })).toBeNull();
  });

  it('wraps the popout picture the same way', () => {
    feed();
    const { unmount } = render(
      <ProfileRenderer page={page([header], { ring_visible: true })} compact />,
    );
    expect(screen.getByRole('button', { name: 'Bob has a new ring' })).toBeInTheDocument();
    unmount();
    render(<ProfileRenderer page={page([header])} compact />);
    expect(screen.queryByRole('button', { name: /has a (new )?ring/ })).toBeNull();
  });
});

describe('the media showcase (D-502, D-503)', () => {
  const hash = 'a'.repeat(64);
  const block = blk('s', 'media_showcase', { items: [{ shelf_id: '9', object_hash: hash }] });
  const views = [
    {
      shelf_id: '9',
      object_hash: hash,
      title: 'Live at the Roxy',
      duration_ms: 185_000,
      media: 'audio' as const,
      caption: 'Our first show',
    },
  ];

  it('lists what the server resolved and mints the address only on Play', async () => {
    let minted = 0;
    server.use(
      http.get(`*/api/v1/profile-pages/bob/showcase/${hash}/url`, () => {
        minted += 1;
        return HttpResponse.json({ url: 'https://bucket.example/x.mp3?sig=1', expires_at: '' });
      }),
    );
    const { container } = render(<ProfileRenderer page={page([block], { showcase: views })} />);
    expect(screen.getByText('Live at the Roxy')).toBeInTheDocument();
    expect(screen.getByText('Our first show')).toBeInTheDocument();
    expect(screen.getByText('Audio · 3:05')).toBeInTheDocument();
    expect(container.querySelector('audio')).toBeNull();
    expect(minted).toBe(0);
    fireEvent.click(screen.getByRole('button', { name: 'Play Live at the Roxy' }));
    await waitFor(() => expect(container.querySelector('audio')).not.toBeNull());
    expect(minted).toBe(1);
    const audio = container.querySelector('audio');
    expect(audio?.getAttribute('controlsList')).toContain('nodownload');
    expect(audio?.getAttribute('src')).toBe('https://bucket.example/x.mp3?sig=1');
  });

  it('says so when nothing can be played', () => {
    render(<ProfileRenderer page={page([block])} />);
    expect(screen.getByText('Nothing to play here right now.')).toBeInTheDocument();
  });

  it('is not drawn in the popout', () => {
    render(<ProfileRenderer page={page([block], { showcase: views })} compact />);
    expect(screen.queryByText('Live at the Roxy')).toBeNull();
  });

  it('edits captions, order and removal in the block settings', () => {
    const onConfig = vi.fn();
    const other = 'b'.repeat(64);
    const two = blk('s', 'media_showcase', {
      items: [
        { shelf_id: '9', object_hash: hash },
        { shelf_id: '9', object_hash: other },
      ],
    });
    render(<Inspector block={two} friends={[]} onConfig={onConfig} showcase={views} />);
    expect(screen.getByLabelText('Caption for Live at the Roxy')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Move Live at the Roxy down' }));
    expect(onConfig).toHaveBeenLastCalledWith({
      items: [
        { shelf_id: '9', object_hash: other },
        { shelf_id: '9', object_hash: hash },
      ],
    });
    fireEvent.change(screen.getByLabelText('Caption for Live at the Roxy'), {
      target: { value: 'Encore' },
    });
    expect(onConfig).toHaveBeenLastCalledWith({
      items: [
        { shelf_id: '9', object_hash: hash, caption: 'Encore' },
        { shelf_id: '9', object_hash: other },
      ],
    });
    fireEvent.click(
      screen.getByRole('button', { name: 'Remove Live at the Roxy from the showcase' }),
    );
    expect(onConfig).toHaveBeenLastCalledWith({ items: [{ shelf_id: '9', object_hash: other }] });
    expect(screen.getByRole('button', { name: 'Add from my shelf' })).toBeEnabled();
  });
});

describe('reporting a guestbook note (D-506)', () => {
  const book = blk('g', 'guestbook');
  const cat = {
    id: '3',
    username: 'cat',
    display_name: 'Cat',
    avatar: null,
    banner: null,
    accent_color: null,
  };
  const held = {
    id: '100',
    author: null,
    pending: true,
    body: 'held note',
    created_at_ms: 0,
    can_remove: true,
  };
  const approved = {
    id: '101',
    author: cat,
    pending: false,
    body: 'approved note',
    created_at_ms: 0,
    can_remove: false,
  };
  beforeEach(() => setReportsEnabled(true));
  afterEach(() => setReportsEnabled(false));

  it('offers the owner Report on a held note next to Approve, and on an approved one', async () => {
    server.use(http.get('*/api/v1/users/2/guestbook', () => HttpResponse.json([held, approved])));
    render(<ProfileRenderer page={page([book], { is_owner: true })} />);
    expect(await screen.findByText('held note')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Approve' })).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Report the note from A visitor' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Report the note from Cat' })).toBeInTheDocument();
  });

  it('offers another viewer Report on an approved note that is not theirs, never on their own', async () => {
    const mine = { ...approved, id: '102', body: 'my note', can_remove: true };
    server.use(http.get('*/api/v1/users/2/guestbook', () => HttpResponse.json([approved, mine])));
    render(<ProfileRenderer page={page([book])} />);
    expect(await screen.findByText('my note')).toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: /^Report the note/ })).toHaveLength(1);
  });

  it('shows no Report item while reporting is off', async () => {
    setReportsEnabled(false);
    server.use(http.get('*/api/v1/users/2/guestbook', () => HttpResponse.json([held, approved])));
    render(<ProfileRenderer page={page([book], { is_owner: true })} />);
    expect(await screen.findByText('held note')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Report the note/ })).toBeNull();
  });
});
