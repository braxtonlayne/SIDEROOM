import { render, screen, waitFor } from '@testing-library/react';
import { http, HttpResponse, server } from '../../test/server';
import { resetGuildStickers } from '../components/ExpressionPicker';
import { MarkdownPreview, StickerSuggestions, stickerQuery } from '../msg/ComposerExtras';
import { DEFAULT_PREFS, sanitizePrefs } from '../../settings/user/prefs';

describe('sticker suggestions and Markdown preview (LNG-USET-051)', () => {
  beforeEach(() => resetGuildStickers());

  it('offers stickers only for one short plain word', () => {
    expect(stickerQuery('wave')).toBe('wave');
    expect(stickerQuery('  wave  ')).toBe('wave');
    expect(stickerQuery('a')).toBeNull();
    expect(stickerQuery('/wave')).toBeNull();
    expect(stickerQuery(':wave')).toBeNull();
    expect(stickerQuery('<@1> hi')).toBeNull();
    expect(stickerQuery('two\nlines')).toBeNull();
    expect(stickerQuery('x'.repeat(40))).toBeNull();
  });

  it('lists matching stickers of the server and sends the picked one', async () => {
    server.use(
      http.get('/api/v1/guilds/9/stickers', () =>
        HttpResponse.json([
          { id: '1', name: 'wave', tags: 'hi', format_type: 1, available: true },
          { id: '2', name: 'hello wave', tags: '', format_type: 1, available: true },
          { id: '3', name: 'sad', tags: '', format_type: 1, available: true },
        ]),
      ),
    );
    const onPick = vi.fn();
    render(<StickerSuggestions guildId="9" text="wave" onPick={onPick} />);
    await waitFor(() => expect(screen.getAllByRole('button')).toHaveLength(2));
    const first = screen.getByRole('button', { name: 'Send sticker wave' });
    expect(screen.queryByTitle('sad')).toBeNull();
    first.click();
    expect(onPick).toHaveBeenCalledTimes(1);
  });

  it('shows nothing in a DM or for a long message', () => {
    const { container, rerender } = render(
      <StickerSuggestions guildId={undefined} text="wave" onPick={() => {}} />,
    );
    expect(container).toBeEmptyDOMElement();
    rerender(<StickerSuggestions guildId="9" text="this is a long sentence" onPick={() => {}} />);
    expect(screen.queryByRole('group')).toBeNull();
  });

  it('previews only text that uses Markdown', () => {
    const { container, rerender } = render(<MarkdownPreview text="plain words" />);
    expect(container).toBeEmptyDOMElement();
    rerender(<MarkdownPreview text="some **bold** words" />);
    expect(screen.getByRole('region', { name: 'Preview' })).toBeInTheDocument();
  });

  it('keeps the new preferences valid', () => {
    expect(DEFAULT_PREFS.stickerSuggestions).toBe(true);
    expect(DEFAULT_PREFS.markdownPreview).toBe(false);
    expect(sanitizePrefs({ spoilers: 'mine' }).spoilers).toBe('mine');
    expect(sanitizePrefs({ spoilers: 'nope' }).spoilers).toBe('click');
    expect(sanitizePrefs({ stickerSuggestions: 3 }).stickerSuggestions).toBe(true);
  });
});
