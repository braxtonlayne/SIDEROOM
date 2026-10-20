// M-03 (D-289): the sticker picker shows the default packs in a "Default" section above the server's own stickers,
// and a server can hide them.
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { resetGuildStickers } from '../components/ExpressionPicker';
import { resetStickerCache } from '../msg/Sticker';
import { useSaved } from '../shell/savedMessages';
import { at, baseHandlers, calls, me, msg, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const PACKS = {
  sticker_packs: [
    {
      id: '5',
      name: 'Side Room Words',
      description: null,
      cover_sticker_id: '50',
      stickers: [
        { id: '50', name: 'brb', tags: 'be right back', format_type: 1, available: true },
        { id: '51', name: 'gg', tags: 'good game', format_type: 1, available: true },
      ],
    },
  ],
};

function handlers(hidden: boolean) {
  server.use(
    http.get('*/api/v1/guilds/g1/stickers', () =>
      HttpResponse.json([{ id: '9', name: 'wave', tags: 'hi', format_type: 1, available: true }]),
    ),
    http.get('*/api/v1/guilds/g1/default-sticker-packs', () => HttpResponse.json({ hidden })),
    http.get('*/api/v1/sticker-packs', () => HttpResponse.json(PACKS)),
    http.get('*/api/v1/guilds/g1/stickers/:sid/url', () => new HttpResponse(null, { status: 404 })),
    http.get('*/api/v1/stickers/:sid/url', () =>
      HttpResponse.json({ url: '/cdn/stickers/pack/5/50.png?sig=x', expires_at: 1 }),
    ),
    http.post('*/api/v1/channels/c1/messages', async (ctx) => {
      await record('POST')(ctx);
      return HttpResponse.json(msg('200', { author: me }));
    }),
  );
}

describe('sticker picker: default packs', () => {
  beforeEach(() => {
    baseHandlers();
    resetGuildStickers();
    resetStickerCache();
    useSaved.getState().reset();
    useStore.setState({});
    seedLounge({ messages: [msg('100', { content: 'hello' })] });
  });

  it('shows a Default section above the server stickers and sends a default sticker', async () => {
    handlers(false);
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Open sticker picker' }));
    const section = await screen.findByRole('region', { name: 'Default' });
    expect(within(section).getByRole('button', { name: 'Send sticker brb' })).toBeInTheDocument();
    expect(within(section).getByRole('button', { name: 'Send sticker gg' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'This server' })).toBeInTheDocument();
    const all = screen.getAllByRole('button', { name: /^Send sticker / });
    expect(all.map((b) => b.getAttribute('aria-label'))).toEqual([
      'Send sticker brb',
      'Send sticker gg',
      'Send sticker wave',
    ]);
    await userEvent.click(within(section).getByRole('button', { name: 'Send sticker gg' }));
    const sent = calls.find((c) => c.key === 'POST /channels/c1/messages')!;
    expect((sent.body as { sticker_ids: string[] }).sticker_ids).toEqual(['51']);
  });

  it('shows only the server stickers when the server hides the default packs', async () => {
    handlers(true);
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Open sticker picker' }));
    expect(await screen.findByRole('button', { name: 'Send sticker wave' })).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Default' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Send sticker brb' })).toBeNull();
  });
});
