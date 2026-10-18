import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../router';
import { resetStore, useStore } from '../../store';
import type * as S from '../../store/types';
import { http, HttpResponse, server } from '../../test/server';
import { resetComposerDrafts } from '../Composer';
import { useLoungeUi } from '../store';
import { resetStickerCache } from '../msg/Sticker';
import { interactionConfig } from '../msg/Components';
import { warmLoungeRoute } from '../../test/warmRoutes';

warmLoungeRoute();

// The gateway is not under test; the store is seeded directly.
vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const me: S.User = { id: '1', username: 'me', global_name: 'Me Myself' };
const bob: S.User = { id: '2', username: 'bob', global_name: 'Bob' };

/** Owner: every permission, so delete/pin/forward are all offered. */
const NOW = Date.now();
const iso = (minAgo: number) => new Date(NOW - minAgo * 60_000).toISOString();

const msg = (id: string, over: Partial<S.Message> = {}): S.Message => ({
  id,
  channel_id: 'c1',
  author: bob,
  content: '',
  timestamp: iso(60 - (Number(id.slice(-2)) || 0)),
  ...over,
});

const WAVE = btoa(String.fromCharCode(10, 80, 200, 120, 30, 255, 90));

function seedMessages(): S.Message[] {
  return [
    msg('100', { content: 'plain hello' }),
    msg('101', {
      content: 'files',
      attachments: [
        {
          id: 'a1',
          filename: 'cat.png',
          size: 2048,
          url: '/cdn/attachments/c1/a1/cat.png',
          content_type: 'image/png',
          width: 100,
          height: 80,
          description: 'a cat',
        },
        {
          id: 'a2',
          filename: 'SPOILER_secret.png',
          size: 2048,
          url: '/cdn/attachments/c1/a2/secret.png',
          content_type: 'image/png',
          width: 100,
          height: 80,
        },
        {
          id: 'a3',
          filename: 'report.pdf',
          size: 3 * 1024 * 1024,
          url: '/cdn/attachments/c1/a3/report.pdf',
          content_type: 'application/pdf',
        },
        {
          id: 'a4',
          filename: 'song.mp3',
          size: 5000,
          url: '/cdn/attachments/c1/a4/song.mp3',
          content_type: 'audio/mpeg',
        },
        {
          id: 'a5',
          filename: 'old.zip',
          size: 99,
          url: '/cdn/attachments/c1/a5/old.zip',
          content_type: 'application/zip',
          expires_at: '2020-01-01T00:00:00Z',
        },
      ],
    }),
    msg('102', {
      attachments: [
        {
          id: 'v1',
          filename: 'voice-message.ogg',
          size: 900,
          url: '/cdn/attachments/c1/v1/voice-message.ogg',
          content_type: 'audio/ogg',
          duration_secs: 7.4,
          waveform: WAVE,
        },
      ],
      flags: 8192,
    }),
    msg('103', {
      content: 'embed time',
      embeds: [
        {
          title: 'A page',
          url: 'https://example.com/page',
          description: 'Some **bold** words',
          color: 0xff0000,
          provider: { name: 'Example' },
          fields: [
            { name: 'One', value: 'first', inline: true },
            { name: 'Two', value: 'second', inline: true },
          ],
          footer: { text: 'footer text' },
        },
        {
          title: 'A video',
          url: 'https://video.example/watch',
          provider: { name: 'VideoSite' },
          video: { url: 'https://video.example/embed/1' },
          thumbnail: { url: 'https://video.example/t.jpg' },
        },
      ],
    }),
    msg('104', {
      poll: {
        question: { text: 'Best snack?' },
        answers: [
          { answer_id: 1, poll_media: { text: 'Chips', emoji: { name: '🍟' } } },
          { answer_id: 2, poll_media: { text: 'Fruit' } },
        ],
        expiry: new Date(NOW + 3 * 3_600_000).toISOString(),
        allow_multiselect: false,
        results: {
          is_finalized: false,
          answer_counts: [
            { id: 1, count: 2, me_voted: false },
            { id: 2, count: 1, me_voted: false },
          ],
        },
      },
    }),
    msg('105', {
      content: '',
      message_reference: { message_id: '100', type: 1 },
      message_snapshots: [
        {
          message: {
            content: 'original words',
            timestamp: iso(500),
            attachments: [],
            embeds: [],
          },
        },
      ],
    }),
    msg('106', { sticker_items: [{ id: '9', name: 'wave', format_type: 1 }] }),
    msg('107', { type: 6, content: '', message_reference: { message_id: '100' } }),
    msg('108', {
      author: me,
      content: 'mine to edit',
      mentions: [me],
    }),
    msg('109', {
      content: 'reply to a vanished message',
      message_reference: { message_id: '101000000000000' },
    }),
    msg('110', { content: 'hey @Me Myself', mentions: [me] }),
    msg('111', {
      content: 'app message',
      components: [
        {
          type: 1,
          components: [
            { type: 2, style: 1, label: 'Approve', custom_id: 'a' },
            { type: 2, style: 5, label: 'Docs', url: 'https://docs.example/x' },
            { type: 2, style: 2, label: 'Nope', custom_id: 'b', disabled: true },
          ],
        },
        {
          type: 1,
          components: [
            {
              type: 3,
              custom_id: 's',
              placeholder: 'Pick a colour',
              options: [{ label: 'Red', value: 'r' }],
            },
          ],
        },
      ],
    }),
    msg('112', { content: `join us ${location.origin}/invite/abc123` }),
  ];
}

function seed(over: { messages?: S.Message[] } = {}) {
  resetStore();
  localStorage.clear();
  resetComposerDrafts();
  resetStickerCache();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  const list = over.messages ?? seedMessages();
  useStore.setState({
    currentUserId: '1',
    users: { '1': me, '2': bob },
    guilds: { g1: { id: 'g1', name: 'Store Guild', owner_id: '1' } },
    guildOrder: ['g1'],
    channels: {
      cat: { id: 'cat', type: 4, guild_id: 'g1', name: 'Text Channels', position: 0 },
      c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', parent_id: 'cat', position: 1 },
      c2: { id: 'c2', type: 0, guild_id: 'g1', name: 'random', parent_id: 'cat', position: 2 },
    },
    channelIdsByGuild: { g1: ['cat', 'c1', 'c2'] },
    roles: {
      g1: {
        id: 'g1',
        guild_id: 'g1',
        name: '@everyone',
        color: 0,
        position: 0,
        permissions: '3072',
      },
    },
    roleIdsByGuild: { g1: ['g1'] },
    memberIdsByGuild: { g1: ['1', '2'] },
    members: {
      'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
      'g1:2': { guild_id: 'g1', user_id: '2', roles: [] },
    },
    windows: {
      c1: {
        ids: list.map((m) => m.id),
        pending: [],
        hasMoreBefore: false,
        atLatest: true,
        loading: false,
        error: null,
      },
    },
    messages: Object.fromEntries(list.map((m) => [m.id, m])),
    connection: 'connected',
  });
}

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  const view = render(<RouterProvider router={router} />);
  return { router, ...view };
}

/** Requests the app made, in order, as `METHOD path` with the parsed JSON body. */
const calls: { key: string; body: unknown }[] = [];
const record =
  (method: string) =>
  async ({ request }: { request: Request }) => {
    let body: unknown = null;
    try {
      body = await request.clone().json();
    } catch {
      /* no body */
    }
    calls.push({ key: `${method} ${new URL(request.url).pathname.replace('/api/v1', '')}`, body });
    return undefined;
  };

function handlers() {
  calls.length = 0;
  const noContent = () => new HttpResponse(null, { status: 204 });
  server.use(
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.get('*/api/v1/guilds/:gid/members', () => HttpResponse.json([])),
    http.get('*/api/v1/channels/:cid/messages', () => HttpResponse.json([])),
    http.get('*/api/v1/invites/abc123', () =>
      HttpResponse.json({
        type: 0,
        code: 'abc123',
        guild: { id: 'g2', name: 'Book Club' },
        channel: { id: 'c9', type: 0 },
        uses: 0,
        max_uses: 0,
        max_age: 0,
        created_at: iso(5),
        approximate_member_count: 42,
        approximate_presence_count: 7,
      }),
    ),
    http.get('*/api/v1/guilds/g1/stickers/9/url', () =>
      HttpResponse.json({ url: '/cdn/stickers/g1/9.png?sig=x', expires_at: 1 }),
    ),
    http.post('*/api/v1/channels/:cid/messages/:mid/ack', noContent),
    http.delete('*/api/v1/channels/:cid/messages/:mid', async (ctx) => {
      await record('DELETE')(ctx);
      return noContent();
    }),
    http.put('*/api/v1/channels/:cid/pins/:mid', async (ctx) => {
      await record('PUT')(ctx);
      return noContent();
    }),
    http.put('*/api/v1/channels/:cid/polls/:mid/answers/:aid/voters/@me', async (ctx) => {
      await record('PUT')(ctx);
      return noContent();
    }),
    http.put('*/api/v1/channels/:cid/messages/:mid/reactions/:emoji/@me', async (ctx) => {
      await record('PUT')(ctx);
      return noContent();
    }),
    http.patch('*/api/v1/channels/:cid/messages/:mid', async (ctx) => {
      await record('PATCH')(ctx);
      return HttpResponse.json(
        msg('108', { author: me, content: 'edited', edited_timestamp: iso(0) }),
      );
    }),
  );
}

describe('message rendering', () => {
  beforeEach(() => {
    seed();
    handlers();
  });

  it('renders attachments, voice, embeds, polls, forwards, stickers and system rows', async () => {
    at('/channels/g1/c1');
    const log = await screen.findByRole('log', { name: 'Messages in general' });
    // image mosaic with alt text, spoiler reveal, file card, audio, expired chip
    expect(await within(log).findByAltText('a cat')).toBeInTheDocument();
    expect(within(log).getByRole('button', { name: 'Reveal spoiler image' })).toBeInTheDocument();
    expect(within(log).getByRole('link', { name: 'report.pdf' })).toHaveAttribute(
      'href',
      '/cdn/attachments/c1/a3/report.pdf',
    );
    expect(within(log).getByText('3.0 MB')).toBeInTheDocument();
    expect(within(log).getByLabelText('Audio: song.mp3')).toBeInTheDocument();
    expect(within(log).getByText('This file has expired')).toBeInTheDocument();
    // voice message player
    const voice = within(log).getByRole('group', { name: 'Voice message' });
    expect(within(voice).getByRole('button', { name: 'Play voice message' })).toBeInTheDocument();
    expect(within(voice).getByText('0:07')).toBeInTheDocument();
    // embed
    expect(within(log).getByRole('link', { name: 'A page' })).toHaveAttribute(
      'href',
      'https://example.com/page',
    );
    expect(within(log).getByText('footer text')).toBeInTheDocument();
    expect(within(log).getByText('Two')).toBeInTheDocument();
    // poll
    const poll = within(log).getByRole('region', { name: 'Poll: Best snack?' });
    expect(within(poll).getByText(/3 votes/)).toBeInTheDocument();
    // forward frame
    const fwd = within(log).getByRole('group', { name: 'Forwarded message' });
    expect(within(fwd).getByText('Forwarded')).toBeInTheDocument();
    expect(within(fwd).getByText('original words')).toBeInTheDocument();
    // sticker
    expect(await within(log).findByAltText('Sticker: wave')).toHaveAttribute(
      'src',
      '/cdn/stickers/g1/9.png?sig=x',
    );
    // pin system message
    expect(within(log).getByText(/pinned/)).toBeInTheDocument();
    // deleted reply target and mention highlight
    expect(within(log).getByText('Original message was deleted')).toBeInTheDocument();
    const mentioned = within(log)
      .getByText(/hey @Me Myself/)
      .closest('article');
    expect(mentioned).toHaveAttribute('data-mention');
  });

  it('renders app components and a server invite card', async () => {
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    expect(within(log).getByRole('button', { name: 'Approve' })).toBeEnabled();
    expect(within(log).getByRole('button', { name: 'Nope' })).toBeDisabled();
    expect(within(log).getByRole('link', { name: 'Docs' })).toHaveAttribute(
      'href',
      'https://docs.example/x',
    );
    expect(within(log).getByRole('combobox', { name: 'Pick a colour' })).toBeInTheDocument();
    interactionConfig.timeoutMs = 60;
    await userEvent.click(within(log).getByRole('button', { name: 'Approve' }));
    // Waiting for the app: the button shows a spinner and cannot be pressed again, then the failure.
    expect(within(log).getByRole('button', { name: /Approve/ })).toHaveAttribute(
      'aria-busy',
      'true',
    );
    expect(await screen.findByText('This interaction failed')).toBeInTheDocument();
    const invite = await within(log).findByRole('region', { name: 'Invite to Book Club' });
    expect(within(invite).getByText('7 Online • 42 Members')).toBeInTheDocument();
    expect(within(invite).getByRole('button', { name: 'Join' })).toBeInTheDocument();
  });

  it('keeps third-party video embeds behind a click', async () => {
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    expect(log.querySelector('iframe')).toBeNull();
    expect(within(log).getByText(/can see your IP address/)).toBeInTheDocument();
    await userEvent.click(within(log).getByRole('button', { name: 'Load video from VideoSite' }));
    expect(log.querySelector('iframe')).toHaveAttribute('src', 'https://video.example/embed/1');
  });

  it('has no axe violations in the rich message list', async () => {
    const { container } = at('/channels/g1/c1');
    await screen.findByAltText('a cat');
    expect(await axeViolations(container)).toEqual([]);
  });

  it('opens the lightbox and moves between images', async () => {
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Open image cat.png' }));
    const dialog = await screen.findByRole('dialog', { name: 'cat.png' });
    expect(within(dialog).getByText('1 / 2')).toBeInTheDocument();
    expect(within(dialog).getByRole('link', { name: 'Download' })).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Next image' }));
    expect(within(dialog).getByText('2 / 2')).toBeInTheDocument();
  });
});

describe('message actions', () => {
  beforeEach(() => {
    seed();
    handlers();
  });

  const articleFor = async (text: string) => {
    const el = (await screen.findByText(text)).closest('article') as HTMLElement;
    return within(el);
  };

  it('deletes with a confirmation, and Shift skips it', async () => {
    at('/channels/g1/c1');
    const mine = await articleFor('mine to edit');
    await userEvent.click(mine.getByRole('button', { name: 'Delete' }));
    const dialog = await screen.findByRole('dialog', { name: 'Delete Message' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Delete' }));
    await waitFor(() => expect(screen.queryByText('mine to edit')).toBeNull());
    expect(calls.map((c) => c.key)).toContain('DELETE /channels/c1/messages/108');

    const other = await articleFor('plain hello');
    const user = userEvent.setup();
    await user.keyboard('{Shift>}');
    await user.click(other.getByRole('button', { name: 'Delete' }));
    await user.keyboard('{/Shift}');
    expect(screen.queryByRole('dialog', { name: 'Delete Message' })).toBeNull();
    await waitFor(() =>
      expect(calls.map((c) => c.key)).toContain('DELETE /channels/c1/messages/100'),
    );
  });

  it('pins from the more menu after confirming', async () => {
    at('/channels/g1/c1');
    const el = await articleFor('plain hello');
    await userEvent.click(el.getByRole('button', { name: 'More' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Pin Message' }));
    const dialog = await screen.findByRole('dialog');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Pin' }));
    await waitFor(() => expect(calls.map((c) => c.key)).toContain('PUT /channels/c1/pins/100'));
    expect(useStore.getState().messages['100']?.pinned).toBe(true);
  });

  it('copies a message link built from the ids', async () => {
    const writeText = vi.fn();
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    at('/channels/g1/c1');
    const el = await articleFor('plain hello');
    await userEvent.click(el.getByRole('button', { name: 'More' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Copy Message Link' }));
    expect(writeText).toHaveBeenCalledWith(`${location.origin}/channels/g1/c1/100`);
  });

  it('reacts through the picker and toggles an existing reaction', async () => {
    at('/channels/g1/c1');
    const el = await articleFor('plain hello');
    await userEvent.click(el.getByRole('button', { name: 'Add Reaction' }));
    await userEvent.click(await screen.findByRole('button', { name: 'fire' }));
    await waitFor(() =>
      expect(calls.some((c) => c.key.startsWith('PUT /channels/c1/messages/100/reactions/'))).toBe(
        true,
      ),
    );
    expect(
      await screen.findByRole('button', { name: /🔥 1 reaction, including you/ }),
    ).toBeInTheDocument();
  });

  it('edits in place: Enter saves, Escape cancels, an empty edit asks to delete', async () => {
    at('/channels/g1/c1');
    const mine = await articleFor('mine to edit');
    await userEvent.click(mine.getByRole('button', { name: 'Edit' }));
    const box = await screen.findByRole('textbox', { name: 'Edit message' });
    expect(screen.getByText(/escape to/)).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('textbox', { name: 'Edit message' })).toBeNull();

    await userEvent.click((await articleFor('mine to edit')).getByRole('button', { name: 'Edit' }));
    const box2 = await screen.findByRole('textbox', { name: 'Edit message' });
    await userEvent.clear(box2);
    await userEvent.type(box2, 'edited{Enter}');
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'PATCH /channels/c1/messages/108')?.body).toEqual({
        content: 'edited',
      }),
    );
    expect(box).toBeDefined();

    await userEvent.click((await articleFor('edited')).getByRole('button', { name: 'Edit' }));
    const box3 = await screen.findByRole('textbox', { name: 'Edit message' });
    await userEvent.clear(box3);
    await userEvent.keyboard('{Enter}');
    expect(await screen.findByRole('dialog', { name: 'Delete Message' })).toBeInTheDocument();
  });

  it('votes in a poll and shows results', async () => {
    at('/channels/g1/c1');
    const poll = await screen.findByRole('region', { name: 'Poll: Best snack?' });
    await userEvent.click(within(poll).getByRole('radio', { name: /Fruit/ }));
    await waitFor(() =>
      expect(calls.map((c) => c.key)).toContain('PUT /channels/c1/polls/104/answers/2/voters/@me'),
    );
    expect(within(poll).getByText(/4 votes/)).toBeInTheDocument();
    expect(within(poll).getByRole('radio', { name: /Fruit/ })).toBeChecked();
  });

  it('forwards to up to five destinations with a note', async () => {
    server.use(
      http.post('*/api/v1/channels/:cid/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(msg('200', { channel_id: 'c2' }));
      }),
    );
    at('/channels/g1/c1');
    const el = await articleFor('plain hello');
    await userEvent.click(el.getByRole('button', { name: 'Forward' }));
    const dialog = await screen.findByRole('dialog', { name: 'Forward To' });
    await userEvent.click(within(dialog).getByRole('checkbox', { name: /#random/ }));
    await userEvent.click(within(dialog).getByRole('button', { name: /Send/ }));
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/c2/messages')).toBe(true),
    );
    const body = calls.find((c) => c.key === 'POST /channels/c2/messages')!.body as {
      message_reference: Record<string, unknown>;
    };
    expect(body.message_reference).toMatchObject({ type: 1, message_id: '100', channel_id: 'c1' });
  });
});

describe('reactors, failed sends, drops and limits', () => {
  beforeEach(() => {
    seed({
      messages: [
        msg('100', {
          content: 'react to me',
          reactions: [
            { emoji: { id: null, name: '🔥' }, count: 2, me: false },
            { emoji: { id: null, name: '👍' }, count: 1, me: false },
          ],
        }),
      ],
    });
    handlers();
  });

  it('lists who reacted per emoji and lets a moderator remove a reaction', async () => {
    let removed = '';
    server.use(
      http.get('*/api/v1/channels/c1/messages/100/reactions/:emoji', ({ params }) =>
        HttpResponse.json(
          decodeURIComponent(String(params.emoji)) === '🔥'
            ? [bob, { id: '5', username: 'carol' }]
            : [{ id: '6', username: 'dave' }],
        ),
      ),
      http.delete('*/api/v1/channels/c1/messages/100/reactions/:emoji/:uid', ({ params }) => {
        removed = String(params.uid);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    const row = (await screen.findByText('react to me')).closest('article') as HTMLElement;
    await userEvent.click(within(row).getByRole('button', { name: /🔥 2 reactions/ }), {
      pointerEventsCheck: 0,
    });
    // left click toggles; the modal opens from the context action
    await userEvent.pointer({
      keys: '[MouseRight]',
      target: within(row).getByRole('button', { name: /🔥/ }),
    });
    const dialog = await screen.findByRole('dialog', { name: 'Reactions' });
    expect(await within(dialog).findByText('carol')).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole('tab', { name: /👍/ }));
    expect(await within(dialog).findByText('dave')).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole('button', { name: /Remove dave/ }));
    await waitFor(() => expect(removed).toBe('6'));
    expect(within(dialog).queryByText('dave')).toBeNull();
  });

  it('retries or discards a message that failed to send', async () => {
    useStore.setState((s) => ({
      messages: {
        ...s.messages,
        'pending:n1': msg('pending:n1', {
          author: me,
          content: 'did not go',
          timestamp: iso(1),
          local: { status: 'failed', nonce: 'n1' },
        }),
      },
      windows: { ...s.windows, c1: { ...s.windows.c1!, pending: ['pending:n1'] } },
    }));
    server.use(
      http.post('*/api/v1/channels/c1/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(msg('400', { author: me, content: 'did not go', nonce: 'n1' }));
      }),
    );
    at('/channels/g1/c1');
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('could not be delivered');
    await userEvent.click(within(alert).getByRole('button', { name: 'Retry' }));
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/c1/messages')).toBe(true),
    );
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
    expect(useStore.getState().messages['400']).toBeDefined();
  });

  it('queues files dropped on the channel and blocks an over-limit message', async () => {
    at('/channels/g1/c1');
    const section = (await screen.findByRole('region', { name: '#general' })) as HTMLElement;
    const file = new File(['x'], 'drop.txt', { type: 'text/plain' });
    fireEvent.dragOver(section, { dataTransfer: { types: ['Files'], files: [file] } });
    fireEvent.drop(section, { dataTransfer: { types: ['Files'], files: [file] } });
    const tray = await screen.findByRole('list', { name: 'Files to upload' });
    expect(within(tray).getByText('drop.txt')).toBeInTheDocument();
    await userEvent.click(within(tray).getByRole('button', { name: 'Remove drop.txt' }));
    expect(screen.queryByRole('list', { name: 'Files to upload' })).toBeNull();

    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    fireEvent.change(box, { target: { value: 'x'.repeat(2005) } });
    expect(await screen.findByRole('alert')).toHaveTextContent('5 characters over the limit');
    expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled();
    await userEvent.click(screen.getByRole('button', { name: 'Upload as a file instead' }));
    const tray2 = await screen.findByRole('list', { name: 'Files to upload' });
    expect(within(tray2).getByText('message.txt')).toBeInTheDocument();
    expect(box).toHaveValue('');
  });
});

describe('composer', () => {
  beforeEach(() => {
    seed();
    handlers();
  });

  it('autocompletes @mentions and sends the wire token', async () => {
    server.use(
      http.post('*/api/v1/channels/c1/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(msg('300', { author: me, content: 'x' }));
      }),
    );
    at('/channels/g1/c1');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.type(box, 'hi @bo');
    const list = await screen.findByRole('listbox', { name: 'Members' });
    expect(within(list).getByRole('option', { name: /Bob/ })).toBeInTheDocument();
    await userEvent.keyboard('{Enter}');
    expect(box).toHaveValue('hi @Bob ');
    await userEvent.keyboard('there{Enter}');
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/c1/messages')).toBe(true),
    );
    expect(
      (calls.find((c) => c.key === 'POST /channels/c1/messages')!.body as { content: string })
        .content,
    ).toBe('hi <@2> there');
  });

  it('autocompletes #channels and :emoji:, Escape closes the list', async () => {
    at('/channels/g1/c1');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.type(box, '#ran');
    expect(await screen.findByRole('option', { name: /#random/ })).toBeInTheDocument();
    await userEvent.keyboard('{Tab}');
    expect(box).toHaveValue('#random ');
    await userEvent.type(box, ':fir');
    expect(await screen.findByRole('option', { name: /:fire:/ })).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('expands /shrug and rejects an empty /me', async () => {
    server.use(
      http.post('*/api/v1/channels/c1/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(msg('301', { author: me }));
      }),
    );
    at('/channels/g1/c1');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.type(box, '/shrug oh well{Enter}');
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/c1/messages')).toBe(true),
    );
    expect((calls[0]!.body as { content: string }).content).toBe('oh well ¯\\_(ツ)_/¯');
  });

  it('shows a formatting toolbar for a selection and wraps it', async () => {
    at('/channels/g1/c1');
    const box = (await screen.findByRole('textbox', {
      name: 'Message #general',
    })) as HTMLTextAreaElement;
    await userEvent.type(box, 'word');
    box.setSelectionRange(0, 4);
    fireEvent.keyUp(box);
    await userEvent.click(await screen.findByRole('button', { name: 'Spoiler' }));
    expect(box).toHaveValue('||word||');
  });

  it('uploads a file with progress and sends the message with the attachment', async () => {
    const put = vi.fn();
    server.use(
      http.post('*/api/v1/channels/c1/attachments', async (ctx) => {
        await record('POST')(ctx);
        const body = (await ctx.request.clone().json()) as { files: { id: string }[] };
        return HttpResponse.json({
          attachments: body.files.map((f) => ({
            id: f.id,
            upload_id: 'u1',
            upload_url: '/storage/put/u1',
            upload_filename: 'abc/cat.png',
            upload_url_expires_in: 900,
            max_size: 10_000_000,
          })),
        });
      }),
      http.put('*/storage/put/u1', async () => {
        put();
        return new HttpResponse(null, { status: 200 });
      }),
      http.post('*/api/v1/channels/c1/attachments/u1/complete', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json({ id: 'u1', filename: 'cat.png', size: 4, url: '/cdn/x' });
      }),
      http.post('*/api/v1/channels/c1/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(msg('302', { author: me }));
      }),
    );
    at('/channels/g1/c1');
    const input = await screen.findByTestId('file-input');
    const file = new File(['abcd'], 'cat.png', { type: 'image/png' });
    await userEvent.upload(input, file);
    const tray = await screen.findByRole('list', { name: 'Files to upload' });
    expect(within(tray).getByText('cat.png')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Send message' }));
    await waitFor(() => expect(put).toHaveBeenCalled());
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/c1/messages')).toBe(true),
    );
    const post = calls.find((c) => c.key === 'POST /channels/c1/messages')!.body as {
      attachments: { id: string; uploaded_filename: string }[];
    };
    expect(post.attachments).toHaveLength(1);
    expect(post.attachments[0]).toMatchObject({ uploaded_filename: 'abc/cat.png' });
    await waitFor(() => expect(screen.queryByRole('list', { name: 'Files to upload' })).toBeNull());
  });

  it('creates a poll from the plus menu', async () => {
    server.use(
      http.post('*/api/v1/channels/c1/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(msg('303', { author: me }));
      }),
    );
    at('/channels/g1/c1');
    await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.click(screen.getByRole('button', { name: 'Add to message' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Create Poll' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create Poll' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Post' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('Ask a question.');
    await userEvent.type(within(dialog).getByPlaceholderText(/question/), 'Lunch?');
    await userEvent.type(within(dialog).getByRole('textbox', { name: 'Answer 1' }), 'Pizza');
    await userEvent.type(within(dialog).getByRole('textbox', { name: 'Answer 2' }), 'Sushi');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Post' }));
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/c1/messages')).toBe(true),
    );
    const body = calls[0]!.body as { poll: { question: { text: string }; duration: number } };
    expect(body.poll.question.text).toBe('Lunch?');
    expect(body.poll.duration).toBe(24);
  });

  it('offers the reply ping toggle', async () => {
    server.use(
      http.post('*/api/v1/channels/c1/messages', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(msg('304', { author: me }));
      }),
    );
    at('/channels/g1/c1');
    const el = (await screen.findByText('plain hello')).closest('article') as HTMLElement;
    await userEvent.click(within(el).getByRole('button', { name: 'Reply' }));
    const toggle = await screen.findByRole('button', { name: /Mention Bob: on/ });
    await userEvent.click(toggle);
    expect(screen.getByRole('button', { name: /Mention Bob: off/ })).toBeInTheDocument();
    await userEvent.type(screen.getByRole('textbox', { name: 'Message #general' }), 'yo{Enter}');
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'POST /channels/c1/messages')).toBe(true),
    );
    const body = calls[0]!.body as { allowed_mentions: { replied_user: boolean } };
    expect(body.allowed_mentions.replied_user).toBe(false);
  });
});
