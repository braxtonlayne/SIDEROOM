import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { Embed } from '../data';
import { EmbedList } from '../msg/Embeds';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';

const PROXY = '/cdn/external/abc123/aHR0cHM6Ly9pbWcuZXhhbXBsZS9hLnBuZw';

/** Every <img> src in the document: the thing a third-party host would see a request for. */
function imgSources(container: HTMLElement): string[] {
  return [...container.querySelectorAll('img')].map((i) => i.getAttribute('src') ?? '');
}

describe('embed images (LNG-MSG-014, D-176, F-20)', () => {
  const remote: Embed = {
    type: 'rich',
    title: 'A page',
    url: 'https://example.com/page',
    description: 'Some words',
    provider: { name: 'Example' },
    image: { url: 'https://img.example/a.png' },
    thumbnail: { url: 'https://img.example/t.png' },
    author: { name: 'Ann', icon_url: 'https://img.example/ann.png' },
    footer: { text: 'footer', icon_url: 'https://img.example/f.png' },
  };

  it('renders the text and no third-party image before a click', () => {
    const { container } = render(<EmbedList embeds={[remote]} />);
    expect(screen.getByRole('link', { name: 'A page' })).toBeInTheDocument();
    expect(screen.getByText('Some words')).toBeInTheDocument();
    expect(screen.getByText('footer')).toBeInTheDocument();
    expect(container.querySelectorAll('img')).toHaveLength(0);
    expect(screen.getByRole('button', { name: 'Load image from img.example' })).toBeInTheDocument();
    expect(screen.getByText(/shares your IP address with that site/)).toBeInTheDocument();
  });

  it('loads every image of one embed after one click, without a referrer', async () => {
    const { container } = render(<EmbedList embeds={[remote]} />);
    await userEvent.click(screen.getByRole('button', { name: 'Load image from img.example' }));
    expect(imgSources(container).sort()).toEqual(
      [
        'https://img.example/a.png',
        'https://img.example/t.png',
        'https://img.example/ann.png',
        'https://img.example/f.png',
      ].sort(),
    );
    for (const img of container.querySelectorAll('img')) {
      expect(img).toHaveAttribute('referrerpolicy', 'no-referrer');
    }
    expect(screen.queryByRole('button', { name: /Load image/ })).toBeNull();
  });

  it('keeps the control per embed', async () => {
    const other: Embed = {
      type: 'rich',
      title: 'Other',
      image: { url: 'https://other.example/b.png' },
    };
    const { container } = render(<EmbedList embeds={[remote, other]} />);
    await userEvent.click(screen.getByRole('button', { name: 'Load image from img.example' }));
    // The second embed is still gated.
    expect(imgSources(container)).not.toContain('https://other.example/b.png');
    expect(
      screen.getByRole('button', { name: 'Load image from other.example' }),
    ).toBeInTheDocument();
  });

  it('loads a proxy_url straight away and never the original', () => {
    const proxied: Embed = {
      type: 'rich',
      title: 'Proxied',
      image: { url: 'https://img.example/a.png', proxy_url: PROXY },
      thumbnail: { url: 'https://img.example/t.png', proxy_url: `${PROXY}2` },
      author: {
        name: 'Ann',
        icon_url: 'https://img.example/ann.png',
        proxy_icon_url: `${PROXY}3`,
      },
      footer: { text: 'f', icon_url: 'https://img.example/f.png', proxy_icon_url: `${PROXY}4` },
    };
    const { container } = render(<EmbedList embeds={[proxied]} />);
    expect(imgSources(container).sort()).toEqual(
      [PROXY, `${PROXY}2`, `${PROXY}3`, `${PROXY}4`].sort(),
    );
    expect(screen.queryByRole('button', { name: /Load image/ })).toBeNull();
  });

  it('only a partly proxied embed still offers the control for the rest', async () => {
    const partial: Embed = {
      type: 'rich',
      title: 'Mixed',
      image: { url: 'https://img.example/a.png', proxy_url: PROXY },
      thumbnail: { url: 'https://thumbs.example/t.png' },
    };
    const { container } = render(<EmbedList embeds={[partial]} />);
    expect(imgSources(container)).toEqual([PROXY]);
    await userEvent.click(screen.getByRole('button', { name: 'Load image from thumbs.example' }));
    expect(imgSources(container)).toContain('https://thumbs.example/t.png');
    // The proxied image never switches to the original.
    expect(imgSources(container)).not.toContain('https://img.example/a.png');
  });

  it('ignores a proxy_url that does not point at our own proxy path', () => {
    const hostile: Embed = {
      type: 'rich',
      title: 'Hostile',
      image: { url: 'https://img.example/a.png', proxy_url: 'https://evil.example/track.png' },
      thumbnail: { url: 'https://img.example/t.png', proxy_url: '//evil.example/x.png' },
      author: { name: 'x', proxy_icon_url: 'javascript:alert(1)' },
      footer: { text: 'f', proxy_icon_url: '/cdn/external/..\\evil' },
    };
    const { container } = render(<EmbedList embeds={[hostile]} />);
    expect(container.querySelectorAll('img')).toHaveLength(0);
    expect(screen.getByRole('button', { name: 'Load image from img.example' })).toBeInTheDocument();
  });

  it('ignores non-http image urls even after the click', async () => {
    const odd: Embed = {
      type: 'rich',
      title: 'Odd',
      image: { url: 'javascript:alert(1)' },
      thumbnail: { url: 'data:image/png;base64,AAAA' },
    };
    const { container } = render(<EmbedList embeds={[odd]} />);
    expect(screen.queryByRole('button', { name: /Load image/ })).toBeNull();
    expect(container.querySelectorAll('img')).toHaveLength(0);
  });

  it('gates a bare image link too', async () => {
    const bare: Embed = {
      type: 'image',
      url: 'https://img.example/x.png',
      thumbnail: { url: 'https://img.example/x.png' },
    };
    const { container } = render(<EmbedList embeds={[bare]} />);
    expect(container.querySelectorAll('img')).toHaveLength(0);
    await userEvent.click(screen.getByRole('button', { name: 'Load image from img.example' }));
    expect(imgSources(container)).toEqual(['https://img.example/x.png']);
    const proxiedBare: Embed = { ...bare, thumbnail: { url: bare.url, proxy_url: PROXY } };
    const second = render(<EmbedList embeds={[proxiedBare]} />);
    expect(imgSources(second.container)).toEqual([PROXY]);
  });

  it('has no axe violations while gated', async () => {
    const { container } = render(<EmbedList embeds={[remote]} />);
    expect(await axeViolations(container)).toEqual([]);
  });
});
