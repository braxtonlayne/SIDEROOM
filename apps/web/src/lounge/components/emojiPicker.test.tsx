import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
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

describe('EmojiPicker category sidebar (RDY-PAR-002)', () => {
  beforeEach(() => {
    localStorage.clear();
    resetGuildEmojis();
  });

  it('lists one button per category, jumps and moves with the arrow keys', async () => {
    const user = userEvent.setup();
    render(<EmojiPicker onPick={vi.fn()} />);
    const bar = screen.getByRole('toolbar', { name: 'Emoji categories' });
    const tabs = within(bar).getAllByRole('button');
    expect(tabs.length).toBeGreaterThan(3);
    // One stop in the tab order (roving tabindex), on the first category.
    expect(tabs.filter((b) => b.tabIndex === 0)).toHaveLength(1);
    expect(tabs[0]).toHaveAttribute('aria-pressed', 'true');
    tabs[0]!.focus();
    await user.keyboard('{ArrowDown}');
    expect(tabs[1]).toHaveFocus();
    expect(tabs[1]).toHaveAttribute('aria-pressed', 'true');
    expect(tabs[1]).toHaveAttribute('tabindex', '0');
    await user.keyboard('{End}');
    expect(tabs.at(-1)).toHaveFocus();
    await user.keyboard('{Home}');
    expect(tabs[0]).toHaveFocus();
  });

  it('jumps the scroll area to the section and follows the scroll position', async () => {
    const user = userEvent.setup();
    const { container } = render(<EmojiPicker onPick={vi.fn()} />);
    const box = container.querySelector<HTMLElement>('.lng-emoji-scroll')!;
    const sections = [...container.querySelectorAll<HTMLElement>('[data-cat]')];
    // jsdom has no layout: give every section a 100px slot.
    box.getBoundingClientRect = () => ({ top: 0 }) as DOMRect;
    sections.forEach((el, i) => {
      el.getBoundingClientRect = () => ({ top: i * 100 - box.scrollTop }) as DOMRect;
    });
    const tabs = within(screen.getByRole('toolbar')).getAllByRole('button');
    await user.click(tabs[2]!);
    expect(box.scrollTop).toBe(200);
    expect(tabs[2]).toHaveAttribute('aria-pressed', 'true');
    box.scrollTop = 300;
    fireEvent.scroll(box);
    expect(tabs[3]).toHaveAttribute('aria-pressed', 'true');
    expect(tabs[2]).toHaveAttribute('aria-pressed', 'false');
  });

  it('shows the server section with its icon or initial and narrows with search', async () => {
    server.use(
      http.get('*/api/v1/guilds/g1/emojis', () =>
        HttpResponse.json([{ id: '5', name: 'pog', animated: false, available: true, roles: [] }]),
      ),
      http.get('*/api/v1/guilds/g1/emojis/:id/url', () =>
        HttpResponse.json({ url: '/cdn/e.png', expires_at: 1 }),
      ),
    );
    const user = userEvent.setup();
    render(<EmojiPicker onPick={vi.fn()} guildId="g1" onPickCustom={vi.fn()} />);
    const bar = screen.getByRole('toolbar', { name: 'Emoji categories' });
    await waitFor(() =>
      expect(within(bar).getByRole('button', { name: 'This server' })).toBeTruthy(),
    );
    await user.type(screen.getByLabelText('Search emoji'), 'fire');
    expect(within(bar).queryByRole('button', { name: 'This server' })).toBeNull();
    expect(within(bar).getAllByRole('button').length).toBe(1);
  });
});
