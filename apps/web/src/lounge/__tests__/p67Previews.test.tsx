// SEC-AUD-203 / SEC-AUD-204 (wave 67, D-688, D-689): the message request preview and the thread row's last
// message, and the excerpt rule the client shares with the server (`sr_chat::preview::excerpt`).
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { resetStore, useStore } from '../../store';
import { emptyData, reduceDispatch } from '../../store/reducer';
import type * as S from '../../store/types';
import { useLoungeUi } from '../store';
import { useNotifSettings } from '../shell/notifSettings';
import { useSelfPresence } from '../shell/selfPresence';
import { useSocial } from '../shell/social';
import { useIgnores } from '../shell/ignores';
import { warmLoungeRoute } from '../../test/warmRoutes';
import { applyRequestPreview, applyThreadPreview } from '../previewLive';
import { excerpt } from '../msg/excerpt';
import { singleLineText } from '../msg/SingleLine';
import { parseMarkdown } from '../markdown';
import { ThreadChip } from '../threads/ThreadChip';
import { toLoungeMessage } from '../liveMap';

warmLoungeRoute();

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('excerpt (the same cases as the server)', () => {
  it('keeps short text whole and puts line breaks and white space runs on one line', () => {
    expect(excerpt('hello there')).toBe('hello there');
    expect(excerpt('  one\n\n\ntwo \t three\r\nfour  ')).toBe('one two three four');
  });

  it('gives none for an empty string or only white space', () => {
    expect(excerpt('')).toBeNull();
    expect(excerpt('   \n\t \r\n  ')).toBeNull();
    expect(excerpt('  ')).toBeNull();
  });

  it('does not cut exactly 140 characters and cuts 141 with an ellipsis', () => {
    expect(excerpt('a'.repeat(140))).toBe('a'.repeat(140));
    expect(excerpt('a'.repeat(141))).toBe(`${'a'.repeat(140)}…`);
  });

  it('cuts on a character, never inside a multi-byte one', () => {
    const cut = excerpt('\u{1F600}'.repeat(150))!;
    expect([...cut].filter((c) => c === '\u{1F600}')).toHaveLength(140);
    expect(cut.endsWith('…')).toBe(true);
  });

  it('does not split a combining sequence at the cut', () => {
    expect(excerpt(`${'a'.repeat(139)}étail`)).toBe(`${'a'.repeat(139)}…`);
    expect(excerpt(`${'a'.repeat(138)}é`)).toBe(`${'a'.repeat(138)}é`);
  });

  it('leaves no trailing space before the ellipsis', () => {
    expect(excerpt(`${'a'.repeat(139)} ${'b'.repeat(10)}`)).toBe(`${'a'.repeat(139)}…`);
  });
});

describe('the single-line text', () => {
  it('turns mentions, emoji and links into text and hides spoilers', () => {
    const text = singleLineText(
      parseMarkdown('hi <@2> <#9> [site](http://x.test) ||secret|| `code`'),
      {
        user: (id) => (id === '2' ? { name: 'Bob' } : undefined),
        channel: () => ({ name: 'general' }),
      },
      '[spoiler]',
    );
    expect(text).toContain('@Bob');
    expect(text).toContain('#general');
    expect(text).toContain('site');
    expect(text).not.toContain('http://x.test');
    expect(text).toContain('[spoiler]');
    expect(text).not.toContain('secret');
    expect(text).toContain('code');
  });
});

const me: S.User = { id: '1', username: 'me', global_name: 'Me Myself' };
const bob: S.User = { id: '2', username: 'bob', global_name: 'Bob B' };
const cat: S.User = { id: '3', username: 'cat' };
const dan: S.User = { id: '4', username: 'dan' };
const eve: S.User = { id: '5', username: 'eve' };

function seed() {
  resetStore();
  localStorage.clear();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  useNotifSettings.setState({ byGuild: {} });
  useSocial.getState().reset();
  useSelfPresence.setState({
    status: 'online',
    customText: null,
    customEmoji: null,
    loaded: false,
  });
  useStore.setState({
    currentUserId: '1',
    users: { '1': me, '2': bob, '3': cat },
    guilds: { g1: { id: 'g1', name: 'Alpha Guild', owner_id: '1' } },
    guildOrder: ['g1'],
    channels: { c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', position: 0 } },
    channelIdsByGuild: { g1: ['c1'], '@me': [] },
    connection: 'connected',
  });
}

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

const quiet = () =>
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.get('*/api/v1/channels/:cid/messages', () => HttpResponse.json([])),
    http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
  );

describe('the Message Requests row (D-688)', () => {
  const request = (id: string, from: S.User, extra: Record<string, unknown> = {}) => ({
    id,
    type: 1,
    recipients: [from],
    is_message_request: true,
    ...extra,
  });
  const open = async () => {
    at('/channels/@me');
    await userEvent.click(await screen.findByRole('button', { name: /Message Requests/ }));
    return screen.findByRole('region', { name: 'Message Requests' });
  };

  beforeEach(() => {
    seed();
    quiet();
  });

  it('shows the words, an attachment count, or both, on one line', async () => {
    useStore.setState({
      messageRequestDms: [
        request('d1', dan, {
          sr_request_preview: { content: 'hello there', attachments: 0, timestamp: 't' },
        }),
        request('d2', cat, { sr_request_preview: { content: '', attachments: 2, timestamp: 't' } }),
        request('d3', eve, {
          sr_request_preview: { content: 'look at this', attachments: 1, timestamp: 't' },
        }),
      ],
    });
    const page = await open();
    expect(within(page).getByText('hello there')).toBeInTheDocument();
    expect(within(page).getByText('Sent 2 attachments')).toBeInTheDocument();
    expect(within(page).getByText('look at this')).toBeInTheDocument();
    expect(within(page).getByText(/· Sent 1 attachment/)).toBeInTheDocument();
  });

  it('is as it was without a preview', async () => {
    useStore.setState({ messageRequestDms: [request('d1', dan)] });
    const page = await open();
    expect(within(page).getByText('dan')).toBeInTheDocument();
    expect(page.querySelector('[data-request-preview]')).toBeNull();
    expect(within(page).queryByRole('button', { name: /Show the message/ })).toBeNull();
  });

  it('hides the text of Likely spam until Show message is pressed, for that row only', async () => {
    useStore.setState({
      messageRequestDms: [
        request('d1', dan, {
          sr_request_reason: 'spam',
          sr_request_preview: { content: 'win a free prize', attachments: 0, timestamp: 't' },
        }),
        request('d2', cat, {
          sr_request_reason: 'spam',
          sr_request_preview: { content: 'click this now', attachments: 0, timestamp: 't' },
        }),
      ],
    });
    const page = await open();
    expect(within(page).queryByText('win a free prize')).toBeNull();
    expect(within(page).queryByText('click this now')).toBeNull();
    await userEvent.click(within(page).getByRole('button', { name: 'Show the message from dan' }));
    expect(within(page).getByText('win a free prize')).toBeInTheDocument();
    expect(within(page).queryByText('click this now')).toBeNull();
    expect(
      within(page).getByRole('button', { name: 'Show the message from cat' }),
    ).toBeInTheDocument();
  });

  it('shows hostile text as plain truncated text', async () => {
    const evil = `<img src=x onerror=alert(1)> [click](http://evil.test) ${'w'.repeat(500)}`;
    useStore.setState({
      messageRequestDms: [
        request('d1', dan, {
          sr_request_preview: { content: excerpt(evil)!, attachments: 0, timestamp: 't' },
        }),
      ],
    });
    const page = await open();
    const line = page.querySelector('[data-request-preview]')!;
    expect(line.querySelector('img, a')).toBeNull();
    expect(line.textContent).toContain('<img src=x onerror=alert(1)>');
    expect(line.textContent).not.toContain('http://evil.test');
    expect(line.textContent!.length).toBeLessThanOrEqual(141);
  });

  it('builds the preview from the next message of the sender when it arrives live', () => {
    const list = [request('d1', dan)] as never;
    const msg = (extra: Record<string, unknown>) => ({
      id: '9',
      channel_id: 'd1',
      author: { id: '4' },
      content: 'first words',
      attachments: [{ id: 'a' }],
      timestamp: '2026-11-13T10:00:00.000000+00:00',
      ...extra,
    });
    const next = applyRequestPreview(list, 'MESSAGE_CREATE', msg({}), '1');
    expect(next[0]!.sr_request_preview).toEqual({
      message_id: '9',
      content: 'first words',
      attachments: 1,
      timestamp: '2026-11-13T10:00:00.000000+00:00',
    });
    // Not for my own message, a system line, another channel or another event.
    expect(applyRequestPreview(list, 'MESSAGE_CREATE', msg({ author: { id: '1' } }), '1')).toBe(
      list,
    );
    expect(applyRequestPreview(list, 'MESSAGE_CREATE', msg({ type: 7 }), '1')).toBe(list);
    expect(applyRequestPreview(list, 'MESSAGE_CREATE', msg({ channel_id: 'zz' }), '1')).toBe(list);
    expect(applyRequestPreview(list, 'MESSAGE_UPDATE', msg({}), '1')).toBe(list);
  });
});

describe('the thread row (D-689)', () => {
  const base = { id: 't1', name: 'Plans', messageCount: 3 };
  const last = (author: string, extra: Record<string, unknown> = {}) => ({
    id: '50',
    author: { id: author, displayName: author === '2' ? 'Bobby' : 'Cat' },
    content: 'see you at noon',
    attachments: 0,
    createdAt: '2026-01-02T03:04:05.000000+00:00',
    ...extra,
  });

  beforeEach(() => {
    seed();
    useIgnores.setState({ ids: [] });
    useSocial.setState({ relationships: [] });
  });

  it('shows two lines: the name and count, then the author, the text and the time', () => {
    const { container } = render(
      <ThreadChip thread={{ ...base, last: last('2') }} onOpen={() => undefined} />,
    );
    const chip = container.querySelector('.lng-thread-chip') as HTMLElement;
    expect(within(chip).getByText('Plans')).toBeInTheDocument();
    expect(within(chip).getByText('3 messages')).toBeInTheDocument();
    const second = chip.querySelector('[data-thread-last]')!;
    expect(second.textContent).toContain('Bobby');
    expect(second.textContent).toContain('see you at noon');
    expect(second.querySelector('time')).not.toBeNull();
  });

  it('is one line, as it was, with no last message', () => {
    const { container } = render(<ThreadChip thread={base} onOpen={() => undefined} />);
    expect(container.querySelector('[data-thread-last]')).toBeNull();
    expect(container.querySelector('.lng-thread-chip')!.hasAttribute('data-has-last')).toBe(false);
  });

  it('shows an attachment-only message as a count', () => {
    render(
      <ThreadChip
        thread={{ ...base, last: last('2', { content: '', attachments: 2 }) }}
        onOpen={() => undefined}
      />,
    );
    expect(screen.getByText('Sent 2 attachments')).toBeInTheDocument();
  });

  it('puts the blocked-message wording in place of the text of a blocked author', () => {
    useSocial.setState({
      relationships: [{ id: '3', type: 2, nickname: null, since: '', user: cat }],
    });
    const { container } = render(
      <ThreadChip thread={{ ...base, last: last('3') }} onOpen={() => undefined} />,
    );
    const second = container.querySelector('[data-thread-last]')!;
    expect(second.textContent).toContain('1 blocked message');
    expect(second.textContent).not.toContain('see you at noon');
  });

  it('maps the REST object onto the row', () => {
    const m = toLoungeMessage({
      id: '10',
      channel_id: 'c1',
      author: bob,
      content: 'start',
      timestamp: '2026-01-01T00:00:00.000000+00:00',
      thread: {
        id: '10',
        name: 'Plans',
        message_count: 2,
        sr_last_message: {
          id: '50',
          author: { id: '2', username: 'bob', global_name: 'Bob B' },
          content: 'latest',
          attachments: 1,
          timestamp: '2026-01-02T00:00:00.000000+00:00',
        },
      },
    });
    expect(m.thread?.last).toMatchObject({
      id: '50',
      author: { id: '2', displayName: 'Bob B' },
      content: 'latest',
      attachments: 1,
    });
  });

  describe('live', () => {
    const starter = (lastMessage?: S.ThreadLastMessage): Record<string, S.Message> => ({
      t1: {
        id: 't1',
        channel_id: 'c1',
        author: bob,
        content: 'start',
        timestamp: 'x',
        thread: {
          id: 't1',
          name: 'Plans',
          message_count: 1,
          ...(lastMessage ? { sr_last_message: lastMessage } : {}),
        },
      },
    });
    const prior: S.ThreadLastMessage = {
      id: '50',
      author: { id: '2', username: 'bob' },
      content: 'old',
      attachments: 0,
      timestamp: 'x',
    };
    const created = (id: string, content: string, extra: Record<string, unknown> = {}) => ({
      id,
      channel_id: 't1',
      author: { id: '3', username: 'cat' },
      content,
      attachments: [],
      timestamp: '2026-01-03T00:00:00.000000+00:00',
      ...extra,
    });

    it('follows a new message in the thread', () => {
      const out = applyThreadPreview(
        starter(prior),
        'MESSAGE_CREATE',
        created('60', ' newer\nwords '),
      );
      expect(out.t1!.thread!.sr_last_message).toMatchObject({
        id: '60',
        content: 'newer words',
        author: { id: '3', username: 'cat' },
      });
    });

    it('also gives the first message of a thread that had none', () => {
      const out = applyThreadPreview(starter(), 'MESSAGE_CREATE', created('60', 'hello'));
      expect(out.t1!.thread!.sr_last_message?.content).toBe('hello');
    });

    it('ignores an older message, a system line and another channel', () => {
      const s = starter(prior);
      expect(applyThreadPreview(s, 'MESSAGE_CREATE', created('40', 'older'))).toBe(s);
      expect(applyThreadPreview(s, 'MESSAGE_CREATE', created('60', 'x', { type: 7 }))).toBe(s);
      expect(
        applyThreadPreview(s, 'MESSAGE_CREATE', created('60', 'x', { channel_id: 'c1' })),
      ).toBe(s);
    });

    it('follows an edit of that message only', () => {
      const s = starter(prior);
      const out = applyThreadPreview(
        s,
        'MESSAGE_UPDATE',
        created('50', 'edited', { author: { id: '2' } }),
      );
      expect(out.t1!.thread!.sr_last_message?.content).toBe('edited');
      expect(applyThreadPreview(s, 'MESSAGE_UPDATE', created('49', 'other'))).toBe(s);
    });

    it('drops the preview when that message is deleted, and only then', () => {
      const s = starter(prior);
      const out = applyThreadPreview(s, 'MESSAGE_DELETE', { id: '50', channel_id: 't1' });
      expect(out.t1!.thread).toEqual({ id: 't1', name: 'Plans', message_count: 1 });
      expect(applyThreadPreview(s, 'MESSAGE_DELETE', { id: '51', channel_id: 't1' })).toBe(s);
      const bulk = applyThreadPreview(s, 'MESSAGE_DELETE_BULK', {
        ids: ['1', '50'],
        channel_id: 't1',
      });
      expect(bulk.t1!.thread!.sr_last_message).toBeUndefined();
    });

    it('is kept by the store when the starter message is updated without it', () => {
      const s = { ...emptyData(), messages: starter(prior) };
      const out = reduceDispatch(s, 'MESSAGE_UPDATE', {
        id: 't1',
        channel_id: 'c1',
        content: 'start (edited)',
        thread: { id: 't1', name: 'Plans', message_count: 2 },
      });
      const m = out.messages!.t1!;
      expect(m.content).toBe('start (edited)');
      expect(m.thread).toMatchObject({ message_count: 2, sr_last_message: { id: '50' } });
    });
  });
});
