import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Markdown, type MdNode, type Resolvers } from '@sideroom/markdown-react';
import { customEmojiResolver, resetCustomEmojiUrls } from '../msg/CustomEmoji';
import { resetRichCards, richCardResolvers } from '../msg/RichCards';
import { resetStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';

const Md = ({ nodes, resolvers }: { nodes: MdNode[]; resolvers: Resolvers }) => (
  <Markdown nodes={nodes} resolvers={resolvers} />
);

beforeEach(() => {
  resetStore();
  resetRichCards();
  resetCustomEmojiUrls();
});

describe('Cinema and Library rich cards (LNG-MSGUI-018)', () => {
  it('renders a Cinema title as a card with Watch and Watch together', async () => {
    server.use(
      http.get('*/api/v1/cinema/titles/7', () =>
        HttpResponse.json({
          title: {
            id: '7',
            kind: 'movie',
            title: 'Night Train',
            year: 1999,
            runtime_min: 101,
            synopsis: 'A short plot.',
            poster_url: '/cdn/p/7.jpg',
            age_restricted: false,
          },
        }),
      ),
    );
    const { container } = render(
      <Md nodes={[{ type: 'cinema', id: '7' }]} resolvers={richCardResolvers} />,
    );
    expect(await screen.findByText('Night Train')).toBeInTheDocument();
    expect(screen.getByText('Movie · 1999 · 101 min')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Watch' })).toHaveAttribute('href', '/cinema/t/7');
    expect(screen.getByRole('button', { name: 'Watch together' })).toBeInTheDocument();
    expect(container.querySelector('img')).toHaveAttribute('src', '/cdn/p/7.jpg');
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it('hides poster and synopsis of an age-restricted title', async () => {
    server.use(
      http.get('*/api/v1/cinema/titles/8', () =>
        HttpResponse.json({
          title: {
            id: '8',
            kind: 'series',
            title: 'Adults Only',
            year: 2020,
            synopsis: 'Secret plot.',
            poster_url: '/cdn/p/8.jpg',
            season_count: 2,
            age_restricted: true,
          },
        }),
      ),
    );
    const { container } = render(
      <Md nodes={[{ type: 'cinema', id: '8' }]} resolvers={richCardResolvers} />,
    );
    expect(await screen.findByText('Adults Only')).toBeInTheDocument();
    expect(screen.getByText('18+')).toBeInTheDocument();
    expect(container.querySelector('img')).toBeNull();
    expect(screen.queryByText('Secret plot.')).toBeNull();
  });

  it('renders a Library series with a Read link and falls back when it fails', async () => {
    server.use(
      http.get('*/api/v1/library/series/3', () =>
        HttpResponse.json({
          series: {
            id: '3',
            kind: 'manga',
            title: 'Blue Hour',
            alt_titles: [],
            chapter_count: 12,
            year: 2018,
            synopsis: '',
            age_restricted: false,
            cover_url: '/cdn/c/3.jpg',
          },
          editions: [],
        }),
      ),
      http.get('*/api/v1/library/series/4', () =>
        HttpResponse.json({ message: 'Not Found' }, { status: 404 }),
      ),
    );
    render(
      <Md
        nodes={[
          { type: 'library', id: '3' },
          { type: 'library', id: '4' },
        ]}
        resolvers={richCardResolvers}
      />,
    );
    expect(await screen.findByText('Blue Hour')).toBeInTheDocument();
    expect(screen.getByText('Manga · 2018 · 12 chapters')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Read' })).toHaveAttribute('href', '/library/s/3');
    expect(await screen.findByText('Library series')).toBeInTheDocument();
  });
});

describe('Custom emoji popout (LNG-EXP-011)', () => {
  it('draws the emoji and opens a popout with its name on click', async () => {
    server.use(
      http.get('*/api/v1/guilds/9/emojis/55/url', () =>
        HttpResponse.json({ url: 'https://cdn.example/e/55.png' }),
      ),
    );
    render(
      <Md
        nodes={[{ type: 'custom_emoji', name: 'blob', id: 55, animated: false }]}
        resolvers={{ customEmoji: customEmojiResolver('9') }}
      />,
    );
    const btn = await screen.findByRole('button', { name: 'About the emoji :blob:' });
    await userEvent.click(btn);
    expect(await screen.findByRole('dialog', { name: 'About :blob:' })).toBeInTheDocument();
    expect(screen.getByText(':blob:')).toBeInTheDocument();
  });

  it('keeps the :name: text when the emoji cannot be resolved', async () => {
    server.use(
      http.get('*/api/v1/guilds/9/emojis/56/url', () =>
        HttpResponse.json({ message: 'Unknown' }, { status: 404 }),
      ),
    );
    render(
      <Md
        nodes={[{ type: 'custom_emoji', name: 'gone', id: 56, animated: false }]}
        resolvers={{ customEmoji: customEmojiResolver('9') }}
      />,
    );
    await waitFor(() => expect(screen.getByText(':gone:')).toBeInTheDocument());
    expect(screen.queryByRole('button')).toBeNull();
  });
});
