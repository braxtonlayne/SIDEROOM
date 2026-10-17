import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { resetStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { ME, signedIn } from '../../settings/user/__tests__/helpers';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { ProfileRenderer } from '../ProfileRenderer';
import { emptyDoc, type Block, type ProfileDoc, type ProfilePage } from '../types';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const user = (over: Partial<ProfilePage['user']> = {}): ProfilePage['user'] => ({
  id: '2',
  username: 'bob',
  display_name: 'Bob',
  avatar: null,
  banner: null,
  accent_color: null,
  ...over,
});

const blk = (
  id: string,
  type: Block['type'],
  x: number,
  y: number,
  w: number,
  h: number,
  config: Block['config'] = {},
): Block => ({ id, type, x, y, w, h, config });

const doc = (blocks: Block[], style: Partial<ProfileDoc['style']> = {}): ProfileDoc => {
  const d = emptyDoc();
  return { ...d, style: { ...d.style, ...style }, blocks };
};

const page = (d: ProfileDoc, over: Partial<ProfilePage> = {}): ProfilePage => ({
  user: user(),
  doc: d,
  friends: [],
  is_owner: false,
  can_sign: false,
  customised: d.blocks.length > 0,
  ...over,
});

describe('profile page renderer (EXT-PRF-002, EXT-PRF-003)', () => {
  it('draws blocks on the grid inside a scoped container', () => {
    const d = doc(
      [
        blk('h', 'header', 0, 0, 12, 3, { tagline: 'hello there' }),
        blk('a', 'about', 0, 3, 6, 4, { text: 'I like <b>soup</b>' }),
        blk('f', 'favorites', 6, 3, 6, 4, { title: 'Soups', items: ['miso', 'ramen'] }),
      ],
      { font: 'newsreader', border: 'double', overlay: 'stars', pattern: 'dots', cursor: 'help' },
    );
    const { container } = render(<ProfileRenderer page={page(d)} />);
    const root = container.querySelector('.pp-root') as HTMLElement;
    expect(root).toHaveAttribute('data-font', 'newsreader');
    expect(root).toHaveAttribute('data-border', 'double');
    expect(root).toHaveAttribute('data-overlay', 'stars');
    expect(root).toHaveAttribute('data-pattern', 'dots');
    expect(root).toHaveAttribute('data-cursor', 'help');
    expect(root.style.getPropertyValue('--pp-accent')).toBe('#5865f2');
    expect(screen.getByRole('heading', { name: 'Bob' })).toBeInTheDocument();
    expect(screen.getByText('hello there')).toBeInTheDocument();
    // Text is text: markup in a document is shown, never interpreted.
    expect(screen.getByText('I like <b>soup</b>')).toBeInTheDocument();
    expect(root.querySelector('b')).toBeNull();
    expect(screen.getByRole('heading', { name: 'Soups' })).toBeInTheDocument();
    const sections = [...root.querySelectorAll<HTMLElement>('.pp-block')];
    expect(sections.map((s) => s.dataset.type)).toEqual(['header', 'about', 'favorites']);
    expect(sections[1]!.style.gridColumn).toBe('1 / span 6');
    expect(sections[2]!.style.gridRow).toBe('4 / span 4');
  });

  it('follows only https links and sends them out of the app without a referrer', () => {
    const d = doc([
      blk('c', 'custom', 0, 0, 6, 4, {
        title: 'Links',
        links: [
          { label: 'Good', url: 'https://example.org/page' },
          { label: 'Script', url: 'javascript:alert(1)' },
          { label: 'Plain', url: 'http://example.org/' },
        ],
      }),
      blk('g', 'gallery', 6, 0, 6, 4, {
        items: [{ url: 'https://example.org/a.png', caption: 'Cat' }, { url: 'data:text/html,x' }],
      }),
    ]);
    render(<ProfileRenderer page={page(d)} />);
    const good = screen.getByRole('link', { name: 'Good' });
    expect(good).toHaveAttribute('href', 'https://example.org/page');
    expect(good).toHaveAttribute('rel', expect.stringContaining('noreferrer'));
    expect(good).toHaveAttribute('target', '_blank');
    expect(screen.queryByText('Script')).toBeNull();
    expect(screen.queryByText('Plain')).toBeNull();
    expect(screen.getByRole('link', { name: 'Cat' })).toHaveAttribute(
      'href',
      'https://example.org/a.png',
    );
    // A gallery picture is never loaded from its own host (it would tell that host who looks).
    expect(document.querySelectorAll('img[src^="https://example.org"]')).toHaveLength(0);
  });

  it('loads a gallery picture only through the signed proxy path, with the link under it (D-259)', () => {
    const proxied = '/cdn/external/abc123/aHR0cHM6Ly9pbWcuZXhhbXBsZS9hLnBuZw';
    const d = doc([
      blk('g', 'gallery', 0, 0, 12, 5, {
        items: [
          { url: 'https://img.example/a.png', caption: 'Cat', proxy_url: proxied },
          // The proxy refused this host (deny list, size or time): no path, so a placeholder tile.
          { url: 'https://denied.example/b.png', caption: 'Dog' },
          // A client cannot make the page load an arbitrary address through a made-up proxy_url.
          {
            url: 'https://evil.example/c.png',
            caption: 'Bad',
            proxy_url: 'https://evil.example/c.png',
          },
        ],
      }),
    ]);
    const { container } = render(<ProfileRenderer page={page(d)} />);
    const imgs = [...container.querySelectorAll('img')];
    expect(imgs.map((i) => i.getAttribute('src'))).toEqual([proxied]);
    expect(imgs[0]).toHaveAttribute('alt', 'Cat');
    // The original address stays as the link under each tile.
    expect(screen.getByRole('link', { name: 'Cat' })).toHaveAttribute(
      'href',
      'https://img.example/a.png',
    );
    expect(screen.getByRole('link', { name: 'Dog' })).toHaveAttribute(
      'href',
      'https://denied.example/b.png',
    );
    expect(container.querySelectorAll('.pp-pic-missing')).toHaveLength(2);
  });

  it('falls back to the placeholder tile when the proxy cannot deliver the picture', () => {
    const d = doc([
      blk('g', 'gallery', 0, 0, 12, 5, {
        items: [
          { url: 'https://img.example/a.png', caption: 'Cat', proxy_url: '/cdn/external/x/y' },
        ],
      }),
    ]);
    const { container } = render(<ProfileRenderer page={page(d)} />);
    const img = container.querySelector('img')!;
    fireEvent.error(img);
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('.pp-pic-missing')).not.toBeNull();
    expect(screen.getByRole('link', { name: 'Cat' })).toBeInTheDocument();
  });

  it('draws uploaded pictures from our own CDN and the background picture behind the pattern (K-17)', () => {
    const mine = (id: string, kind: 'background' | 'gallery') => ({
      id,
      kind,
      url: `/cdn/avatars/2/${id}.png`,
      content_type: 'image/png',
      width: 640,
      height: 480,
      bytes: 100,
      created_at_ms: 1,
    });
    const d = doc(
      [
        blk('g', 'gallery', 0, 0, 12, 5, {
          items: [{ image_id: '21', caption: 'Mine' }, { image_id: '99' }],
        }),
      ],
      { background: { kind: 'image', image_id: '11', pattern: 'dots' } },
    );
    const { container } = render(
      <ProfileRenderer
        page={page(d, { media: [mine('11', 'background'), mine('21', 'gallery')] })}
      />,
    );
    const root = container.querySelector('.pp-root') as HTMLElement;
    expect(root).toHaveAttribute('data-bg-image', 'true');
    expect((container.querySelector('.pp-bgimg') as HTMLElement).style.backgroundImage).toContain(
      '/cdn/avatars/2/11.png',
    );
    expect(container.querySelector('.pp-patlayer')).toHaveAttribute('data-pattern', 'dots');
    const imgs = [...container.querySelectorAll('img.pp-pic')];
    expect(imgs.map((i) => i.getAttribute('src'))).toEqual(['/cdn/avatars/2/21.png']);
    // The older pattern plus banner pair still draws the banner behind the pattern.
    const old = render(
      <ProfileRenderer
        page={page(doc([], { banner_background: true, pattern: 'grid' }))}
        bannerUrl="/cdn/avatars/2/b.png"
      />,
    );
    expect(old.container.querySelector('.pp-bgimg')).not.toBeNull();
    expect(old.container.querySelector('.pp-patlayer')).toHaveAttribute('data-pattern', 'grid');
  });

  it('asks before opening a song and never embeds it', async () => {
    const u = userEvent.setup();
    const d = doc([
      blk('s', 'song', 0, 0, 6, 3, { title: 'Blue', artist: 'Joni', url: 'https://youtu.be/abc' }),
    ]);
    const { container } = render(<ProfileRenderer page={page(d)} />);
    expect(container.querySelector('iframe')).toBeNull();
    expect(screen.queryByRole('link', { name: 'Open the song' })).toBeNull();
    await u.click(screen.getByRole('button', { name: 'Play' }));
    expect(screen.getByText(/opens youtu\.be in a new tab/)).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Open the song' })).toHaveAttribute(
      'href',
      'https://youtu.be/abc',
    );
    expect(container.querySelector('iframe')).toBeNull();
  });

  it('shows the friends the server confirmed, in order', () => {
    const d = doc([blk('t', 'top_friends', 0, 0, 6, 4, { user_ids: ['3', '4'] })]);
    render(
      <ProfileRenderer
        page={page(d, {
          friends: [
            user({ id: '3', username: 'cy', display_name: 'Cy' }),
            user({ id: '4', username: 'di', display_name: null }),
          ],
        })}
      />,
    );
    const list = screen.getByRole('list');
    const links = within(list).getAllByRole('link');
    expect(links.map((l) => l.getAttribute('href'))).toEqual(['/u/cy', '/u/di']);
    expect(within(links[0]!).getByText('Cy')).toBeInTheDocument();
    expect(within(links[1]!).getByText('di')).toBeInTheDocument();
  });

  it('has no accessibility violations with every block on the page', async () => {
    const d = doc([
      blk('h', 'header', 0, 0, 12, 3, { tagline: 'hi' }),
      blk('a', 'about', 0, 3, 6, 3, { text: 'About' }),
      blk('t', 'top_friends', 6, 3, 6, 3, { user_ids: ['3'] }),
      blk('n', 'now_watching', 0, 6, 6, 2, { mode: 'manual', title: 'Frieren' }),
      blk('f', 'favorites', 6, 6, 6, 3, { items: ['miso'] }),
      blk('g', 'gallery', 0, 8, 6, 3, {
        items: [{ url: 'https://example.org/a.png', caption: 'Cat' }],
      }),
      blk('s', 'song', 6, 9, 6, 2, { title: 'Blue', url: 'https://youtu.be/abc' }),
      blk('c', 'custom', 0, 11, 6, 3, {
        title: 'Links',
        links: [{ label: 'Site', url: 'https://example.org' }],
      }),
    ]);
    const { container } = render(
      <ProfileRenderer
        page={page(d, { friends: [user({ id: '3', username: 'cy', display_name: 'Cy' })] })}
      />,
    );
    expect(await axeViolations(container)).toEqual([]);
  });

  it('is compact in the popout: one column, five blocks at most, no guestbook', () => {
    const blocks = [
      blk('b1', 'about', 0, 0, 6, 2),
      blk('b2', 'guestbook', 6, 0, 6, 2),
      blk('b3', 'custom', 0, 2, 6, 2, { title: 'Three' }),
      blk('b4', 'custom', 6, 2, 6, 2, { title: 'Four' }),
      blk('b5', 'custom', 0, 4, 6, 2, { title: 'Five' }),
      blk('b6', 'custom', 6, 4, 6, 2, { title: 'Six' }),
      blk('b7', 'custom', 0, 6, 6, 2, { title: 'Seven' }),
    ];
    const { container } = render(<ProfileRenderer page={page(doc(blocks))} compact />);
    const types = [...container.querySelectorAll<HTMLElement>('.pp-block')].map(
      (s) => s.dataset.type,
    );
    expect(types).toEqual(['about', 'custom', 'custom', 'custom', 'custom']);
    expect(container.querySelector('.pp-root')).toHaveAttribute('data-compact', 'true');
    expect((container.querySelector('.pp-block') as HTMLElement).style.gridColumn).toBe('');
    expect(screen.queryByText('Seven')).toBeNull();
  });

  it('ignores a colour that is not #rrggbb', () => {
    const d = doc([blk('a', 'about', 0, 0, 6, 4)]);
    d.style.palette.background = 'red; background: url(x)';
    const { container } = render(<ProfileRenderer page={page(d)} />);
    expect(
      (container.querySelector('.pp-root') as HTMLElement).style.getPropertyValue('--pp-bg'),
    ).toBe('#1e1f22');
  });
});

function openPage(path: string) {
  resetStore();
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

describe('profile page route (EXT-PRF-002)', () => {
  beforeEach(() => signedIn());

  it('loads /u/:username and shows the guestbook to a friend, who signs it', async () => {
    const entries: unknown[] = [
      {
        id: '90',
        author: user({ id: '3', username: 'cy', display_name: 'Cy' }),
        body: 'first!',
        created_at_ms: Date.UTC(2026, 0, 2),
        can_remove: false,
      },
    ];
    const posted: string[] = [];
    server.use(
      http.get('*/api/v1/profile-pages/bob', () =>
        HttpResponse.json(
          page(doc([blk('g', 'guestbook', 0, 0, 12, 6, { title: 'Say hi' })]), { can_sign: true }),
        ),
      ),
      http.get('*/api/v1/users/2/guestbook', () => HttpResponse.json(entries)),
      http.post('*/api/v1/users/2/guestbook', async ({ request }) => {
        const body = (await request.json()) as { body: string };
        posted.push(body.body);
        const e = {
          id: '91',
          author: user({ id: ME.id, username: 'alice', display_name: 'Alice' }),
          body: body.body,
          created_at_ms: Date.now(),
          can_remove: true,
        };
        entries.unshift(e);
        return HttpResponse.json(e, { status: 201 });
      }),
    );
    openPage('/u/bob');
    expect(await screen.findByRole('heading', { name: 'Say hi' })).toBeInTheDocument();
    expect(await screen.findByText('first!')).toBeInTheDocument();
    // Somebody else's entry has no Remove button.
    expect(screen.queryByRole('button', { name: /Remove the message/ })).toBeNull();

    const u = userEvent.setup();
    await u.type(screen.getByLabelText('Leave a message'), 'nice page');
    await u.click(screen.getByRole('button', { name: 'Sign' }));
    await waitFor(() => expect(posted).toEqual(['nice page']));
    expect(await screen.findByText('nice page')).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Remove the message from Alice' }),
    ).toBeInTheDocument();
  });

  it('lets the owner remove any entry and offers the editor', async () => {
    const removed: string[] = [];
    server.use(
      http.get('*/api/v1/profile-pages/bob', () =>
        HttpResponse.json(
          page(doc([blk('g', 'guestbook', 0, 0, 12, 6)]), { is_owner: true, can_sign: true }),
        ),
      ),
      http.get('*/api/v1/users/2/guestbook', () =>
        HttpResponse.json([
          {
            id: '90',
            author: user({ id: '3', username: 'cy', display_name: 'Cy' }),
            body: 'rude',
            created_at_ms: 1,
            can_remove: true,
          },
        ]),
      ),
      http.delete('*/api/v1/users/2/guestbook/90', () => {
        removed.push('90');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openPage('/u/bob');
    expect(await screen.findByText('rude')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Edit page' })).toHaveAttribute(
      'href',
      '/profile-page/edit',
    );
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: 'Remove the message from Cy' }));
    await waitFor(() => expect(removed).toEqual(['90']));
    await waitFor(() => expect(screen.queryByText('rude')).toBeNull());
  });

  it('says so when a person has not customised their page, and when there is no such person', async () => {
    server.use(
      http.get('*/api/v1/profile-pages/bob', () => HttpResponse.json(page(doc([])))),
      http.get('*/api/v1/profile-pages/ghost', () =>
        HttpResponse.json({ message: 'Unknown profile' }, { status: 404 }),
      ),
    );
    openPage('/u/bob');
    expect(await screen.findByText('Bob has not customised a profile page')).toBeInTheDocument();
  });

  it('shows a plain not-found card for an unknown person', async () => {
    server.use(
      http.get('*/api/v1/profile-pages/ghost', () =>
        HttpResponse.json({ message: 'Unknown profile' }, { status: 404 }),
      ),
    );
    openPage('/u/ghost');
    expect(await screen.findByText('No such profile')).toBeInTheDocument();
  });
});
