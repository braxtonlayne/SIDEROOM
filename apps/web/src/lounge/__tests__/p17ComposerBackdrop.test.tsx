import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useRef } from 'react';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../test/server';
import { resetGuildStickers } from '../components/ExpressionPicker';
import { ComposerBackdrop } from '../ComposerBackdrop';
import { BACKDROP_MAX_CHARS, tokenizeComposer } from '../composerText';
import { resetStickerCache } from '../msg/Sticker';
import { useSaved } from '../shell/savedMessages';
import { at, baseHandlers, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('tokenizeComposer (M-09, D-299)', () => {
  it('wraps mention, channel, emoji and timestamp tokens and leaves plain text alone', () => {
    const tokens = tokenizeComposer('hi <@42> see <#7> <:pog:9> at <t:1700000000:R> @everyone!');
    expect(tokens.filter((t) => t.kind !== 'text').map((t) => [t.kind, t.text])).toEqual([
      ['mention', '<@42>'],
      ['channel', '<#7>'],
      ['emoji', '<:pog:9>'],
      ['timestamp', '<t:1700000000:R>'],
      ['mention', '@everyone'],
    ]);
    expect(tokens.map((t) => t.text).join('')).toBe(
      'hi <@42> see <#7> <:pog:9> at <t:1700000000:R> @everyone!',
    );
    expect(tokenizeComposer('just words, a < b and @someone')).toEqual([
      { kind: 'text', text: 'just words, a < b and @someone' },
    ]);
    expect(tokenizeComposer('<@&5> <@!6> <a:wave:3>').map((t) => t.kind)).toEqual([
      'mention',
      'text',
      'mention',
      'text',
      'emoji',
    ]);
  });
});

function Pair({ text }: { text: string }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  return (
    <div>
      <ComposerBackdrop text={text} target={ref} />
      <textarea ref={ref} aria-label="Message" defaultValue={text} />
    </div>
  );
}

describe('ComposerBackdrop', () => {
  it('marks tokens, hides itself from the tree and mirrors the scroll offset', () => {
    render(<Pair text={'plain <@42> text'} />);
    const pre = screen.getByTestId('composer-backdrop');
    expect(pre).toHaveAttribute('aria-hidden', 'true');
    const marks = pre.querySelectorAll('mark');
    expect(marks).toHaveLength(1);
    expect(marks[0]).toHaveTextContent('<@42>');
    expect(pre.textContent).toBe('plain <@42> text');
    expect(screen.getByRole('textbox', { name: 'Message' })).toBeInTheDocument();

    const ta = screen.getByRole('textbox', { name: 'Message' });
    ta.scrollTop = 30;
    fireEvent.scroll(ta);
    expect(pre.scrollTop).toBe(ta.scrollTop);
  });

  it('is not drawn for drafts over the length limit, and is the only extra node for the empty draft', async () => {
    baseHandlers();
    resetGuildStickers();
    resetStickerCache();
    useSaved.getState().reset();
    seedLounge({ messages: [msg('100', { content: 'hello' })] });
    server.use(http.get('*/api/v1/guilds/g1/stickers', () => HttpResponse.json([])));
    const { container } = at('/channels/g1/c1');
    const box = await screen.findByRole('textbox', { name: /message/i });
    expect(screen.queryByTestId('composer-backdrop')).toBeNull();
    const user = userEvent.setup();
    await user.click(box);
    await user.paste('hey <@42> and <#7>');
    const pre = await screen.findByTestId('composer-backdrop');
    expect(pre.querySelectorAll('mark')).toHaveLength(2);
    expect(box).toHaveAttribute('data-backdrop', 'true');
    // The backdrop is invisible to the accessibility tree and adds no tab stop.
    expect(await axeViolations(container)).toEqual([]);
    expect(pre.closest('[aria-hidden="true"]')).toBe(pre);

    fireEvent.change(box, { target: { value: 'x'.repeat(BACKDROP_MAX_CHARS + 1) } });
    expect(screen.queryByTestId('composer-backdrop')).toBeNull();
    expect(box).not.toHaveAttribute('data-backdrop');
  });
});
