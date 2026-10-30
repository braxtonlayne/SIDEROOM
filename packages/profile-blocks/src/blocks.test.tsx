import { render, screen } from '@testing-library/react';
import { BlocksEnv, ProfileRenderer, type BlockLike, type PageLike } from './index';

const t = (key: string, values?: Record<string, string | number>) =>
  values ? `${key} ${JSON.stringify(values)}` : key;
const cdn = (kind: string, owner: string, hash: string | null | undefined) =>
  hash ? `/cdn/${kind}/${owner}/${hash}.png` : null;

const block = (id: string, type: string, config: Record<string, unknown> = {}): BlockLike => ({
  id,
  type,
  x: 0,
  y: 0,
  w: 6,
  h: 3,
  config,
});

const page = (blocks: BlockLike[], over: Partial<PageLike> = {}): PageLike => ({
  user: { id: '2', username: 'bob', display_name: 'Bob', avatar: null, banner: null },
  doc: {
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
  ...over,
});

const draw = (p: PageLike) =>
  render(
    <BlocksEnv t={t} cdn={cdn}>
      <ProfileRenderer page={p} />
    </BlocksEnv>,
  );

describe('the shared profile block renderer', () => {
  it('draws a links block with only https links, in a new tab without a referrer', () => {
    draw(
      page([
        block('l', 'linktree', {
          links: [
            { label: 'Site', url: 'https://example.com/a' },
            { label: 'Bad', url: 'javascript:alert(1)' },
            { label: 'Plain', url: 'http://example.com' },
            { label: 'Creds', url: 'https://user:pw@example.com' },
          ],
        }),
      ]),
    );
    const links = screen.getAllByRole('link');
    expect(links.map((a) => a.textContent)).toEqual(['Site']);
    expect(links[0]).toHaveAttribute('target', '_blank');
    expect(links[0]?.getAttribute('rel')).toContain('noreferrer');
    expect(links[0]?.getAttribute('rel')).toContain('noopener');
  });

  it('shows the featured servers the server sent and nothing the document names', () => {
    draw(
      page([block('f', 'featured_guilds', { guild_ids: ['10', '11'] })], {
        featured_guilds: [{ id: '10', name: 'Book Club', icon: null }],
      }),
    );
    expect(screen.getByText('Book Club')).toBeInTheDocument();
    expect(document.body.textContent).not.toContain('11');
  });

  it('draws a typed now playing line, and nothing for presence without a host slot', () => {
    const first = draw(
      page([block('n', 'now_playing', { mode: 'manual', title: 'Blue', subtitle: 'Joni' })]),
    );
    expect(screen.getByText('Blue')).toBeInTheDocument();
    first.unmount();
    const auto = draw(page([block('n', 'now_playing', { mode: 'auto' })]));
    expect(auto.container.querySelector('section')?.textContent).toBe('');
  });

  it('never interprets a document string as markup', () => {
    const { container } = draw(
      page([block('a', 'about', { text: '<img src=x onerror=alert(1)><b>bold</b>' })]),
    );
    expect(container.querySelector('img')).toBeNull();
    expect(container.querySelector('b')).toBeNull();
    expect(container.textContent).toContain('<b>bold</b>');
  });

  it('leaves the guestbook to the host: no slot, no guestbook', () => {
    const { container } = draw(page([block('g', 'guestbook')]));
    expect(container.querySelector('section')?.textContent).toBe('');
  });

  it('renders no block it does not know', () => {
    const { container } = draw(page([block('x', 'hologram', { text: 'secret' })]));
    expect(container.textContent).not.toContain('secret');
  });
});
