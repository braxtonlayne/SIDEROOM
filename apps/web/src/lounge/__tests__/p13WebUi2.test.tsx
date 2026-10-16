// LNG-EXP-011 (emoji popout), D-233 (client-side upload block) and notification keyword highlights.
import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { resetUploadLimits } from '../msg/uploadLimit';
import userEvent from '@testing-library/user-event';
import { useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { setMarkdownEngine, testFallbackEngine } from '../markdown';
import { resetEmojiUrls } from '../msg/CustomEmoji';
import { at, baseHandlers, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('emoji popout (LNG-EXP-011, D-232)', () => {
  beforeEach(() => {
    // The TS stand-in parser has no custom emoji; this one understands <:name:id>.
    setMarkdownEngine({
      ...testFallbackEngine,
      parse: (text) => [
        { type: 'text', text: 'look ' },
        { type: 'custom_emoji', name: 'party', id: '777', animated: false },
        { type: 'text', text: text.endsWith('there') ? ' there' : '' },
      ],
    });
    baseHandlers();
    resetEmojiUrls();
    seedLounge({ messages: [msg('100', { content: 'look <:party:777> there' })] });
    server.use(
      http.get('*/api/v1/sr/emojis/777/url', () =>
        HttpResponse.json({ url: '/cdn/emojis/777.png?sig=1' }),
      ),
    );
  });

  afterEach(() => setMarkdownEngine(testFallbackEngine));

  it('draws the emoji from the signed URL and shows the source server with a Join button', async () => {
    let joined = '';
    server.use(
      http.get('*/api/v1/sr/emojis/777', () =>
        HttpResponse.json({
          id: '777',
          name: 'party',
          animated: false,
          guild: { id: '55', name: 'Party House', icon: null, listing: '55' },
        }),
      ),
      http.post('*/api/v1/directory/55/join', () => {
        joined = '55';
        return HttpResponse.json({});
      }),
    );
    const user = userEvent.setup();
    at('/channels/g1/c1');
    const emoji = await screen.findByRole('button', { name: 'Emoji :party:' });
    await waitFor(() =>
      expect(within(emoji).getByRole('img')).toHaveAttribute(
        'src',
        expect.stringContaining('/cdn/emojis/777.png'),
      ),
    );
    await user.click(emoji);
    const popout = await screen.findByRole('dialog', { name: 'About :party:' });
    expect(await within(popout).findByText('From Party House')).toBeInTheDocument();
    await user.click(within(popout).getByRole('button', { name: 'Join server' }));
    await waitFor(() => expect(joined).toBe('55'));
    expect(await within(popout).findByText('You joined Party House.')).toBeInTheDocument();
  });

  it('says a private server and offers no Join when the emoji has no visible source', async () => {
    server.use(
      http.get('*/api/v1/sr/emojis/777', () =>
        HttpResponse.json({ id: '777', name: 'party', animated: false, guild: null }),
      ),
    );
    const user = userEvent.setup();
    at('/channels/g1/c1');
    await user.click(await screen.findByRole('button', { name: 'Emoji :party:' }));
    const popout = await screen.findByRole('dialog', { name: 'About :party:' });
    expect(
      await within(popout).findByText('This emoji is from a private server.'),
    ).toBeInTheDocument();
    expect(within(popout).queryByRole('button', { name: 'Join server' })).toBeNull();
  });

  it('does not offer Join for a server the viewer is already in', async () => {
    server.use(
      http.get('*/api/v1/sr/emojis/777', () =>
        HttpResponse.json({
          id: '777',
          name: 'party',
          animated: false,
          guild: { id: 'g1', name: 'Store Guild', icon: null, listing: 'g1' },
        }),
      ),
    );
    const user = userEvent.setup();
    at('/channels/g1/c1');
    await user.click(await screen.findByRole('button', { name: 'Emoji :party:' }));
    const popout = await screen.findByRole('dialog', { name: 'About :party:' });
    expect(await within(popout).findByText('You are a member of this server.')).toBeInTheDocument();
    expect(within(popout).queryByRole('button', { name: 'Join server' })).toBeNull();
  });

  it('falls back to :name: text when the URL cannot be resolved', async () => {
    server.use(
      http.get('*/api/v1/sr/emojis/777/url', () =>
        HttpResponse.json({ message: 'Unknown emoji' }, { status: 404 }),
      ),
    );
    at('/channels/g1/c1');
    const emoji = await screen.findByRole('button', { name: 'Emoji :party:' });
    expect(within(emoji).getByText(':party:')).toBeInTheDocument();
  });
});

describe('client-side upload block (D-233)', () => {
  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [msg('100', { content: 'hello' })] });
    useStore.setState((s) => ({
      guilds: { g1: { ...s.guilds.g1!, sr_max_upload_bytes: 1000 } },
    }));
  });

  const pick = (files: File[]) => {
    const input = screen.getByTestId('file-input') as HTMLInputElement;
    fireEvent.change(input, { target: { files } });
  };
  const file = (name: string, size: number) =>
    new File([new Uint8Array(size)], name, { type: 'application/octet-stream' });

  it('refuses a file over the limit with a message and keeps the others', async () => {
    at('/channels/g1/c1');
    await screen.findByTestId('file-input');
    pick([file('big.bin', 5000), file('small.bin', 10)]);
    expect(await screen.findByText(/big\.bin is larger than the/)).toBeInTheDocument();
    expect(await screen.findByText('small.bin')).toBeInTheDocument();
    expect(screen.queryByText('big.bin')).toBeNull();
  });

  it('points a free member to Patron when it refuses a file, and never queues it (H-22)', async () => {
    resetUploadLimits();
    server.use(
      http.get('*/api/v1/donations/me', () =>
        HttpResponse.json({
          patron: { active: false, in_grace: false, permanent: false },
          limits: { upload_bytes: 1000, patron: false },
          preferences: {},
        }),
      ),
    );
    at('/channels/g1/c1');
    await screen.findByTestId('file-input');
    pick([file('big.bin', 5000)]);
    expect(await screen.findByText(/Patrons can upload larger files/)).toBeInTheDocument();
    expect(screen.queryByText('big.bin')).toBeNull();
    resetUploadLimits();
  });

  it('lets everything through when the limit is unknown', async () => {
    useStore.setState((s) => ({ guilds: { g1: { ...s.guilds.g1! } } }));
    useStore.setState((s) => {
      const g = { ...s.guilds.g1! };
      delete g.sr_max_upload_bytes;
      return { guilds: { g1: g } };
    });
    at('/channels/g1/c1');
    await screen.findByTestId('file-input');
    pick([file('big.bin', 5000)]);
    expect(await screen.findByText('big.bin')).toBeInTheDocument();
  });
});

describe('notification keyword highlights', () => {
  it("highlights a message that contains a keyword, but not the viewer's own", async () => {
    baseHandlers();
    server.use(
      http.get('*/api/v1/users/@me/notification-keywords', () =>
        HttpResponse.json({ keywords: ['deploy'] }),
      ),
    );
    seedLounge({
      messages: [
        msg('100', { content: 'time to DEPLOY now' }),
        msg('101', { content: 'nothing special' }),
        msg('102', { content: 'my own deploy note', author: { id: '1', username: 'me' } }),
      ],
    });
    at('/channels/g1/c1');
    const hit = await screen.findByText('time to DEPLOY now');
    await waitFor(() => expect(hit.closest('[data-mention]')).not.toBeNull());
    expect(screen.getByText('nothing special').closest('[data-mention]')).toBeNull();
    expect(screen.getByText('my own deploy note').closest('[data-mention]')).toBeNull();
  });
});
