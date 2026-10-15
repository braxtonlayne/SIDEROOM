import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { customEmojiText, EmojiPicker, resetGuildEmojis } from './EmojiPicker';
import { emptyPrefs, frequent, recordPick, toggleFavorite, withTone } from './emojiPrefs';

describe('emoji prefs', () => {
  it('ranks frequently used emoji and toggles favorites', () => {
    let p = emptyPrefs();
    for (const k of ['a', 'b', 'b', 'c', 'c', 'c']) p = recordPick(p, k);
    expect(frequent(p)).toEqual(['c', 'b', 'a']);
    p = toggleFavorite(p, 'a');
    expect(p.favorites).toEqual(['a']);
    expect(toggleFavorite(p, 'a').favorites).toEqual([]);
  });

  it('applies skin tones after the base character', () => {
    expect(withTone('👍', 0)).toBe('👍');
    expect(withTone('👍', 3)).toBe('👍\u{1F3FD}');
    expect(withTone('❤️', 2)).toBe('❤\u{1F3FC}');
  });

  it('formats custom emoji for message text', () => {
    expect(customEmojiText({ id: '9', name: 'pog', animated: false })).toBe('<:pog:9>');
    expect(customEmojiText({ id: '9', name: 'pog', animated: true })).toBe('<a:pog:9>');
  });
});

describe('EmojiPicker', () => {
  beforeEach(() => {
    localStorage.clear();
    resetGuildEmojis();
  });

  it('remembers picks, applies the skin tone and favorites on right-click', async () => {
    const onPick = vi.fn();
    const user = userEvent.setup();
    render(<EmojiPicker onPick={onPick} />);
    await user.selectOptions(screen.getByLabelText('Skin tone'), '3');
    await user.click(screen.getByRole('button', { name: 'thumbsup' }));
    expect(onPick).toHaveBeenCalledWith('👍\u{1F3FD}');
    // Frequently used now lists it.
    expect(screen.getByRole('region', { name: 'Frequently used' })).toBeInTheDocument();
    await user.pointer({
      keys: '[MouseRight]',
      target: screen.getAllByRole('button', { name: 'fire' })[0]!,
    });
    expect(screen.getByRole('region', { name: 'Favorites' })).toBeInTheDocument();
    // A fresh picker restores both from storage.
    render(<EmojiPicker onPick={vi.fn()} />);
    expect(screen.getAllByRole('region', { name: 'Favorites' }).length).toBeGreaterThan(0);
  });

  it('shows this server custom emoji and inserts them', async () => {
    server.use(
      http.get('*/api/v1/guilds/g1/emojis', () =>
        HttpResponse.json([
          { id: '5', name: 'pog', animated: false, available: true, roles: [] },
          { id: '6', name: 'gone', animated: false, available: false, roles: [] },
        ]),
      ),
      http.get('*/api/v1/guilds/g1/emojis/:id/url', () =>
        HttpResponse.json({ url: '/cdn/e.png', expires_at: 1 }),
      ),
    );
    const onCustom = vi.fn();
    const user = userEvent.setup();
    render(<EmojiPicker onPick={vi.fn()} guildId="g1" onPickCustom={onCustom} />);
    const pog = await screen.findByRole('button', { name: 'pog' });
    expect(screen.queryByRole('button', { name: 'gone' })).toBeNull();
    await user.click(pog);
    expect(onCustom).toHaveBeenCalledWith(expect.objectContaining({ id: '5', name: 'pog' }));
    await user.type(screen.getByLabelText('Search emoji'), 'qqqq');
    await waitFor(() => expect(screen.getByText('No emoji found.')).toBeInTheDocument());
  });
});
