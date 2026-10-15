import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { GifPicker, loadFavorites, toggleFavorite, type Gif } from './GifPicker';

const gif = (id: string): Gif => ({
  id,
  title: `gif ${id}`,
  url: `https://media.tenor.com/${id}.gif`,
  preview_url: `/cdn/external/sig/${id}`,
  width: 200,
  height: 120,
});

describe('GifPicker', () => {
  beforeEach(() => localStorage.clear());

  it('says no provider is connected when the server has none', async () => {
    server.use(http.get('*/api/v1/gifs/config', () => HttpResponse.json({ enabled: false })));
    render(<GifPicker onPick={vi.fn()} />);
    expect(await screen.findByText(/gif provider/i)).toBeInTheDocument();
  });

  it('shows trending GIFs, searches and picks one', async () => {
    const seen: string[] = [];
    server.use(
      http.get('*/api/v1/gifs/config', () => HttpResponse.json({ enabled: true })),
      http.get('*/api/v1/gifs/categories', () =>
        HttpResponse.json({
          categories: [{ name: 'happy', search_term: 'happy', image_url: '/cdn/external/s/h' }],
        }),
      ),
      http.get('*/api/v1/gifs/trending', () => {
        seen.push('trending');
        return HttpResponse.json({ results: [gif('a')], next: 'p2' });
      }),
      http.get('*/api/v1/gifs/search', ({ request }) => {
        seen.push(`search:${new URL(request.url).searchParams.get('q')}`);
        return HttpResponse.json({ results: [gif('b')] });
      }),
    );
    const onPick = vi.fn();
    const user = userEvent.setup();
    render(<GifPicker onPick={onPick} />);
    expect(await screen.findByRole('button', { name: 'Send GIF gif a' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /load more/i })).toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: 'happy' }));
    await user.click(await screen.findByRole('button', { name: 'Send GIF gif b' }));
    expect(onPick).toHaveBeenCalledWith(expect.objectContaining({ id: 'b' }));
    expect(seen).toEqual(['trending', 'search:happy']);
  });

  it('keeps favorites in local storage', async () => {
    server.use(
      http.get('*/api/v1/gifs/config', () => HttpResponse.json({ enabled: true })),
      http.get('*/api/v1/gifs/categories', () => HttpResponse.json({ categories: [] })),
      http.get('*/api/v1/gifs/trending', () => HttpResponse.json({ results: [gif('a')] })),
    );
    const user = userEvent.setup();
    render(<GifPicker onPick={vi.fn()} />);
    await user.click(await screen.findByRole('button', { name: 'Add gif a to favorites' }));
    expect(loadFavorites().map((g) => g.id)).toEqual(['a']);
    await user.click(screen.getByRole('button', { name: 'Favorites' }));
    await waitFor(() =>
      expect(
        screen.getByRole('button', { name: 'Remove gif a from favorites' }),
      ).toBeInTheDocument(),
    );
  });

  it('toggleFavorite adds newest first, removes and caps', () => {
    let list: Gif[] = [];
    list = toggleFavorite(list, gif('1'));
    list = toggleFavorite(list, gif('2'));
    expect(list.map((g) => g.id)).toEqual(['2', '1']);
    expect(toggleFavorite(list, gif('2')).map((g) => g.id)).toEqual(['1']);
    for (let i = 0; i < 120; i++) list = toggleFavorite(list, gif(`x${i}`));
    expect(list.length).toBe(100);
  });
});
