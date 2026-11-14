// SEC-AUD-246..248 (wave 70, D-729, D-730): a message's text outside the message itself is plain (spoiler hidden,
// mention named) and is not shown at all for a blocked or ignored author: the reply line, the Inbox's mention
// list, the DM toast, the delete confirmation, the pins panel, search results, the message-link card and the
// typing line.
import { act, render, renderHook, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ToastProvider } from '@sideroom/ui';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { resetStore, useStore } from '../../store';
import { emitDispatch } from '../../store/events';
import { http, HttpResponse, server } from '../../test/server';
import type { Message } from '../data';
import { InboxButton } from '../inbox/InboxButton';
import { MessageItem } from '../MessageItem';
import { setMarkdownEngine, testFallbackEngine } from '../markdown';
import { DeleteMessageDialog } from '../msg/MessageDialogs';
import { MessageLinkCard } from '../msg/MessageLinkCard';
import { PinsButton } from '../header/PinsButton';
import { DmToasts } from '../shell/DmToasts';
import { useIgnores } from '../shell/ignores';
import { useNotifSettings } from '../shell/notifSettings';
import { useSocial } from '../shell/social';
import { useLiveTypingIds } from '../live';
import { SearchBox } from '../SearchBox';
import { resetHitContext } from '../searchContext';

vi.mock('@tanstack/react-router', async (orig) => ({
  ...(await orig<typeof import('@tanstack/react-router')>()),
  useNavigate: () => vi.fn(),
}));
vi.mock('../nav', async (orig) => ({
  ...(await orig<typeof import('../nav')>()),
  useLoungeRoute: () => ({ guildId: '@me', channelId: 'other' }),
  useLoungeGoto: () => vi.fn(),
  useLoungeGotoMessage: () => vi.fn(),
}));

const RAW = 'psst ||the ending|| <@123>';
const bob = { id: '2', username: 'bob', global_name: 'Bob' };
const carol = { id: '3', username: 'carol', global_name: 'Carol' };

const loungeMsg = (over: Partial<Message> = {}): Message => ({
  id: 'm1',
  channelId: 'c1',
  author: { id: '2', displayName: 'Bob', avatarUrl: null } as Message['author'],
  content: RAW,
  createdAt: '2026-01-01T00:00:00Z',
  ...over,
});

const block = (id: string) =>
  useSocial.setState({
    relationships: [{ id, type: 2, since: '', user: { id, username: 'x' } }],
  } as never);

beforeEach(() => {
  resetStore();
  setMarkdownEngine(testFallbackEngine);
  useSocial.getState().reset();
  useIgnores.getState().reset();
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  useStore.setState({
    currentUserId: '1',
    users: {
      '1': { id: '1', username: 'me' },
      '123': { id: '123', username: 'dana', global_name: 'Dana' },
    },
  } as never);
  server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
});

function expectPlain(root: HTMLElement | Document = document.body) {
  const text = root instanceof Document ? root.body.textContent : root.textContent;
  expect(text).not.toContain('the ending');
  expect(text).not.toContain('123');
}

const renderReply = (target: Message) =>
  render(
    <ToastProvider>
      <MessageItem
        message={loungeMsg({ id: 'm2', replyToId: 'm1', content: 'answer', author: target.author })}
        grouped={false}
        replyTarget={target}
        replyState="loaded"
        resolvers={{}}
        isOwn={false}
        guildId="g1"
        onReply={vi.fn()}
        onEdit={vi.fn()}
        onReact={vi.fn()}
      />
    </ToastProvider>,
  );

describe('the reply line (D-729, D-730)', () => {
  it('shows plain text: the spoiler hidden, the mention named', () => {
    const { container } = renderReply(loungeMsg());
    const line = container.querySelector('.lng-reply-text')!;
    expect(line.textContent).toBe('psst [spoiler] @Dana');
    expectPlain(container.querySelector('.lng-reply') as HTMLElement);
  });

  it('keeps the attachment fallback for a message that is only an attachment', () => {
    renderReply(loungeMsg({ content: '' }));
    expect(screen.getByText('Click to see attachment')).toBeInTheDocument();
  });

  it('shows the fallback, never the raw text, while the parser is not loaded', () => {
    setMarkdownEngine(null);
    const { container } = renderReply(loungeMsg());
    expectPlain(container.querySelector('.lng-reply') as HTMLElement);
    expect(screen.getByText('Click to see attachment')).toBeInTheDocument();
    setMarkdownEngine(testFallbackEngine);
  });

  it('shows the blocked wording and names nobody for a blocked author', () => {
    block('2');
    const { container } = renderReply(loungeMsg());
    const reply = container.querySelector('.lng-reply') as HTMLElement;
    expect(reply.textContent).toContain('1 blocked message');
    expect(reply.textContent).not.toContain('Bob');
    expect(reply.textContent).not.toContain('psst');
    expect(reply.tagName).toBe('BUTTON');
  });

  it('reads "ignored" for an ignored author and leaves an unrelated author alone', () => {
    useIgnores.setState({ ids: ['2'] } as never);
    const { container, unmount } = renderReply(loungeMsg());
    expect(container.querySelector('.lng-reply')!.textContent).toContain('1 ignored message');
    unmount();
    const other = renderReply(
      loungeMsg({
        author: { id: '3', displayName: 'Carol', avatarUrl: null } as Message['author'],
      }),
    );
    expect(other.container.querySelector('.lng-reply')!.textContent).toContain('@Carol');
    expect(other.container.querySelector('.lng-reply')!.textContent).toContain('psst');
  });

  it('follows a block made while the line is open', () => {
    const { container } = renderReply(loungeMsg());
    expect(container.querySelector('.lng-reply')!.textContent).toContain('@Bob');
    act(() => block('2'));
    expect(container.querySelector('.lng-reply')!.textContent).not.toContain('@Bob');
    expect(container.querySelector('.lng-reply')!.textContent).toContain('blocked');
  });
});

const mention = (author: unknown, over: Record<string, unknown> = {}) => ({
  id: 'm1',
  channel_id: 'c1',
  guild_id: 'g1',
  author,
  content: RAW,
  timestamp: '2026-01-01T00:00:00Z',
  ...over,
});

async function openInbox(list: unknown[]) {
  server.use(http.get('*/api/v1/users/@me/mentions', () => HttpResponse.json(list)));
  const view = render(<InboxButton />);
  await userEvent.click(view.getByRole('button', { name: 'Inbox' }));
  return view;
}

describe('the Inbox mention list (D-729, D-730)', () => {
  it('shows plain text with the author and the place', async () => {
    useStore.setState({
      channels: { c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general' } },
    } as never);
    await openInbox([mention(bob)]);
    const item = await screen.findByRole('button', { name: /Bob/ });
    expect(item.textContent).toContain('#general');
    expect(item.textContent).toContain('psst [spoiler] @Dana');
    expectPlain(item);
  });

  it('keeps an empty text empty (no raw text)', async () => {
    await openInbox([mention(bob, { content: '' })]);
    const item = await screen.findByRole('button', { name: /Bob/ });
    expect(item.querySelector('.lng-inbox-snippet')).toBeNull();
  });

  it('drops the row of a blocked or ignored author and keeps the others', async () => {
    block('2');
    useIgnores.setState({ ids: ['4'] } as never);
    await openInbox([
      mention(bob),
      mention({ id: '4', username: 'dave' }, { id: 'm3' }),
      mention(carol, { id: 'm2', content: 'hello' }),
    ]);
    expect(await screen.findByRole('button', { name: /Carol/ })).toBeInTheDocument();
    expect(screen.queryByText(/Bob/)).toBeNull();
    expect(screen.queryByText(/dave/)).toBeNull();
  });
});

describe('the DM toast (D-729)', () => {
  it('shows plain text in the description', async () => {
    useNotifSettings.setState({ load: async () => true, byGuild: {} });
    vi.spyOn(document, 'hasFocus').mockReturnValue(true);
    render(
      <ToastProvider>
        <DmToasts />
      </ToastProvider>,
    );
    await act(async () => {
      emitDispatch('MESSAGE_CREATE', {
        id: 'm9',
        channel_id: 'dm1',
        author: bob,
        content: RAW,
        timestamp: '2026-01-01T00:00:00Z',
        mentions: [],
      });
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(await screen.findByText('Bob sent you a message')).toBeInTheDocument();
    expect(await screen.findByText('psst [spoiler] @Dana')).toBeInTheDocument();
    expectPlain();
    vi.restoreAllMocks();
  });
});

describe('the delete confirmation (D-729, D-730)', () => {
  const dialog = (m: Message) =>
    render(<DeleteMessageDialog message={m} onConfirm={vi.fn()} onCancel={vi.fn()} />);

  it('shows plain text', () => {
    dialog(loungeMsg());
    const d = screen.getByRole('dialog');
    expect(d.textContent).toContain('psst [spoiler] @Dana');
    expectPlain(d);
  });

  it('keeps the attachment count for a message that is only an attachment', () => {
    dialog(loungeMsg({ content: '', attachments: [{ id: 'a' } as never, { id: 'b' } as never] }));
    expect(screen.getByRole('dialog').textContent).toContain('2');
    expect(screen.getByRole('dialog').textContent).toMatch(/attachment/i);
  });

  it('shows the blocked wording for a blocked author', () => {
    block('2');
    dialog(loungeMsg());
    expect(screen.getByRole('dialog').textContent).toContain('1 blocked message');
    expect(screen.getByRole('dialog').textContent).not.toContain('psst');
  });
});

describe('the pins panel (D-730)', () => {
  const wire = (author: unknown, over: Record<string, unknown> = {}) => ({
    id: 'p1',
    channel_id: 'c1',
    author,
    content: RAW,
    timestamp: '2026-01-01T00:00:00Z',
    pinned: true,
    ...over,
  });
  async function openPins(list: unknown[]) {
    server.use(http.get('*/api/v1/channels/c1/pins', () => HttpResponse.json(list)));
    const view = render(<PinsButton guildId="g1" channelId="c1" canUnpin={false} />);
    await userEvent.click(view.getByRole('button', { name: 'Pinned Messages' }));
    return view;
  }

  it('draws a blocked author as a hidden message of one, with Show', async () => {
    block('2');
    await openPins([wire(bob), wire(carol, { id: 'p2', content: 'hello' })]);
    expect(await screen.findByText('hello')).toBeInTheDocument();
    expect(screen.queryByText(/psst/)).toBeNull();
    expect(screen.queryByText('Bob')).toBeNull();
    expect(screen.getByText('1 blocked message')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Show message' }));
    expect(screen.getByText(/psst/)).toBeInTheDocument();
    expect(screen.getByText('Bob')).toBeInTheDocument();
  });

  it('updates when a block is made while it is open', async () => {
    await openPins([wire(bob)]);
    expect(await screen.findByText(/psst/)).toBeInTheDocument();
    act(() => block('2'));
    expect(screen.queryByText(/psst/)).toBeNull();
    expect(screen.getByText('1 blocked message')).toBeInTheDocument();
  });
});

describe('the message-link card (D-730)', () => {
  const link = { guildId: 'g1', channelId: 'c1', messageId: 'm1' };
  const linked = (author: unknown, content = 'secret words') =>
    server.use(
      http.get('*/api/v1/channels/c1/messages/m1', () =>
        HttpResponse.json({
          id: 'm1',
          author,
          content,
          timestamp: '2026-01-01T00:00:00Z',
          attachments: [{ id: 'a' }],
        }),
      ),
    );

  it('quotes an unrelated author', async () => {
    linked(bob);
    render(<MessageLinkCard link={link} />);
    expect(await screen.findByText('secret words')).toBeInTheDocument();
  });

  it('quotes plain text: the spoiler hidden, the mention named (D-729)', async () => {
    linked(bob, RAW);
    const { container } = render(<MessageLinkCard link={link} />);
    expect(await screen.findByText('psst [spoiler] @Dana')).toBeInTheDocument();
    expectPlain(container);
  });

  it('shows the wording instead of the quoted text for a blocked author', async () => {
    block('2');
    linked(bob);
    render(<MessageLinkCard link={link} />);
    expect(await screen.findByText('1 blocked message')).toBeInTheDocument();
    expect(screen.queryByText('secret words')).toBeNull();
    expect(screen.queryByText('Bob')).toBeNull();
    expect(screen.queryByText(/attachment/i)).toBeNull();
  });
});

describe('the typing line (D-730)', () => {
  const type = (...ids: string[]) =>
    useStore.setState({
      users: {
        '1': { id: '1', username: 'me' },
        '2': bob,
        '3': carol,
        '4': { id: '4', username: 'dave' },
      },
      typing: { c1: ids.map((user_id) => ({ user_id, expires_at: Date.now() + 60_000 })) },
    } as never);

  it('lists everyone when nobody is hidden', () => {
    type('2', '3');
    expect(renderHook(() => useLiveTypingIds('c1')).result.current).toEqual(['2', '3']);
  });

  it('leaves out a blocked and an ignored typer, and counts only the shown ones', () => {
    block('2');
    useIgnores.setState({ ids: ['4'] } as never);
    type('2', '3', '4');
    expect(renderHook(() => useLiveTypingIds('c1')).result.current).toEqual(['3']);
  });

  it('takes effect when a block is made now', () => {
    type('2', '3');
    const hook = renderHook(() => useLiveTypingIds('c1'));
    expect(hook.result.current).toEqual(['2', '3']);
    act(() => block('2'));
    expect(hook.result.current).toEqual(['3']);
    expect(useStore.getState().typing.c1).toHaveLength(2);
  });
});

describe('search results (D-730)', () => {
  const hit = (author: unknown, id: string, text: string) => ({
    id,
    channel_id: 'c1',
    guild_id: 'g1',
    author,
    content: text,
    timestamp: '2026-01-01T00:00:00Z',
    hit: true,
    sr_snippet: { text, highlights: [] as [number, number][] },
  });
  const around = (id: string, author: unknown, content: string) => ({
    id,
    channel_id: 'c1',
    author,
    content,
    timestamp: '2026-01-01T00:00:00Z',
  });
  async function search() {
    resetHitContext();
    useStore.setState({
      guilds: { g1: { id: 'g1', name: 'G', owner_id: '1' } },
      channels: { c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general' } },
      channelIdsByGuild: { g1: ['c1'] },
      users: { '1': { id: '1', username: 'me' }, '2': bob, '3': carol },
    } as never);
    server.use(
      http.get('*/api/v1/channels/c1/messages', () =>
        HttpResponse.json([
          around('1000000000000000012', bob, 'bob after'),
          around('1000000000000000011', carol, 'carol hit'),
          around('1000000000000000010', bob, 'bob before'),
        ]),
      ),
      http.get('*/api/v1/guilds/g1/messages/search', () =>
        HttpResponse.json({
          total_results: 2,
          messages: [
            [hit(bob, '1000000000000000020', 'bobs secret words')],
            [hit(carol, '1000000000000000011', 'carols plain words')],
          ],
        }),
      ),
    );
    const view = render(<SearchBox guildId="g1" channelId="c1" label="Search #general" />);
    await userEvent.type(view.getByRole('searchbox', { name: 'Search #general' }), 'words{Enter}');
    return screen.findByRole('region', { name: 'Search results' });
  }

  it('quotes everybody when nobody is hidden', async () => {
    const panel = await search();
    expect(await within(panel).findByText('bobs secret words')).toBeInTheDocument();
    expect(within(panel).getByText('carols plain words')).toBeInTheDocument();
    await waitFor(() => expect(panel.textContent).toContain('bob before'));
  });

  it('shows the wording for a blocked author, in the hit and in the lines around it', async () => {
    block('2');
    const panel = await search();
    expect(await within(panel).findByText('carols plain words')).toBeInTheDocument();
    expect(within(panel).queryByText(/bobs secret/)).toBeNull();
    await waitFor(() =>
      expect(panel.querySelectorAll('[data-hidden-line]').length).toBeGreaterThanOrEqual(2),
    );
    expect(panel.textContent).not.toContain('bob before');
    expect(panel.textContent).not.toContain('bob after');
  });

  it('reads "ignored" for an ignored author', async () => {
    useIgnores.setState({ ids: ['2'] } as never);
    const panel = await search();
    expect((await within(panel).findAllByText('1 ignored message')).length).toBeGreaterThan(0);
    expect(within(panel).queryByText(/bobs secret/)).toBeNull();
  });
});
