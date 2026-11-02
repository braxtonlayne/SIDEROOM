import { render, screen } from '@testing-library/react';
import { BlocksEnv, ProfileRenderer, type BlockLike, type PageLike } from './index';
import { inkOn } from './ProfileRenderer';

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

describe('tip entries of the Links block (ARC-ROOM-001)', () => {
  it('open a payment URI only for a valid Bitcoin or Lightning link', () => {
    draw(
      page([
        block('l', 'linktree', {
          links: [
            {
              label: 'Tip BTC',
              url: 'bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq',
              tip: 'btc',
            },
            { label: 'Bad scheme', url: 'javascript:alert(1)', tip: 'btc' },
            { label: 'Wrong kind', url: 'bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq' },
            { label: 'Ko-fi', url: 'https://ko-fi.com/someone', tip: 'kofi' },
            { label: 'Elsewhere', url: 'https://example.com/x', tip: 'kofi' },
          ],
        }),
      ]),
    );
    const links = screen.getAllByRole('link');
    expect(links.map((a) => a.textContent)).toEqual(['Tip BTC', 'Ko-fi']);
    expect(links[0]).toHaveAttribute(
      'href',
      'bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq',
    );
  });
});

describe('tip QR codes of the Links block (FU-18)', () => {
  const links = [
    {
      label: 'Tip BTC',
      url: 'bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq',
      tip: 'btc',
    },
    { label: 'Ko-fi', url: 'https://ko-fi.com/someone', tip: 'kofi' },
    { label: 'Plain', url: 'https://example.com/x' },
  ];
  const Qr = ({ value, label }: { value: string; label: string }) => (
    <svg role="img" aria-label={label} data-value={value} />
  );

  it('are drawn by the renderer the host fills, for Bitcoin and Lightning only', () => {
    render(
      <BlocksEnv t={t} cdn={cdn} tipQr={Qr}>
        <ProfileRenderer page={page([block('l', 'linktree', { links })])} />
      </BlocksEnv>,
    );
    const qr = screen.getAllByRole('img');
    expect(qr).toHaveLength(1);
    expect(qr[0]).toHaveAttribute('data-value', links[0]!.url);
    expect(qr[0]!.getAttribute('aria-label')).toContain('profile.editor.tipQrLabel');
    // The text link and the code stay beside it.
    expect(screen.getByText(links[0]!.url)).toBeTruthy();
  });

  it('stay text when the host fills nothing (the public profile, the popout)', () => {
    draw(page([block('l', 'linktree', { links })]));
    expect(screen.queryAllByRole('img')).toHaveLength(0);
    expect(screen.getByText(links[0]!.url)).toBeTruthy();
  });
});

describe('the initial on the accent circle (XC-VIS-004)', () => {
  const ratio = (a: string, b: string) => {
    const lum = (hex: string) => {
      const n = parseInt(hex.slice(1), 16);
      const c = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => {
        const s = v / 255;
        return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
      });
      return 0.2126 * c[0]! + 0.7152 * c[1]! + 0.0722 * c[2]!;
    };
    const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
    return (hi! + 0.05) / (lo! + 0.05);
  };

  it.each(['#5865f2', '#c2b08f', '#ffffff', '#000000', '#23a55a', '#f0b232', '#00a8fc', '#9b59b6'])(
    'reads at AA on %s',
    (accent) => {
      expect(ratio(accent, inkOn(accent))).toBeGreaterThanOrEqual(4.5);
    },
  );

  it('is white on the default blue', () => {
    expect(inkOn('#5865f2')).toBe('#ffffff');
  });
});
