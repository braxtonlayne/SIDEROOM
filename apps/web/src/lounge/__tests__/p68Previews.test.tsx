// SEC-AUD-218..220 (wave 68, D-699, D-700, D-701): a deleted message leaves the request preview, the thread row's
// relative time and its shared minute tick, and the chip on the "started a thread" row.
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { resetStore, useStore } from '../../store';
import { emitDispatch } from '../../store/events';
import type * as S from '../../store/types';
import { useLoungeUi } from '../store';
import { useSocial } from '../shell/social';
import { warmLoungeRoute } from '../../test/warmRoutes';
import { applyRequestPreview, applyThreadPreview, startPreviewsLive } from '../previewLive';
import { ThreadChip } from '../threads/ThreadChip';
import { SystemMessage } from '../msg/SystemMessage';
import { toLoungeMessage } from '../liveMap';
import { formatRelativeTime, useMinuteTick } from '../time';
import type { DmObject } from '../shell/api';

warmLoungeRoute();

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const dan: S.User = { id: '4', username: 'dan' };
const bob: S.User = { id: '2', username: 'bob', global_name: 'Bob B' };

describe('a deleted message leaves the request preview (D-699)', () => {
  const preview = { message_id: '9', content: 'first words', attachments: 0, timestamp: 't' };
  const request = (extra: Record<string, unknown> = {}): DmObject[] =>
    [
      {
        id: 'd1',
        type: 1,
        recipients: [dan],
        is_message_request: true,
        sr_request_preview: preview,
        ...extra,
      },
      {
        id: 'd2',
        type: 1,
        recipients: [bob],
        is_message_request: true,
        sr_request_preview: preview,
      },
    ] as never;

  it('removes the preview when the previewed message is deleted', () => {
    const list = request();
    const out = applyRequestPreview(list, 'MESSAGE_DELETE', { id: '9', channel_id: 'd1' }, '1');
    expect(out[0]!.sr_request_preview).toBeUndefined();
    expect('sr_request_preview' in out[0]!).toBe(false);
    // The other conversation keeps its own, though it names the same id.
    expect(out[1]!.sr_request_preview).toEqual(preview);
  });

  it('keeps it when another message is deleted, or one of another conversation', () => {
    const list = request();
    expect(applyRequestPreview(list, 'MESSAGE_DELETE', { id: '8', channel_id: 'd1' }, '1')).toBe(
      list,
    );
    expect(applyRequestPreview(list, 'MESSAGE_DELETE', { id: '9', channel_id: 'zz' }, '1')).toBe(
      list,
    );
  });

  it('removes it on a bulk delete that names the message', () => {
    const list = request();
    const out = applyRequestPreview(
      list,
      'MESSAGE_DELETE_BULK',
      { ids: ['3', '9'], channel_id: 'd1' },
      '1',
    );
    expect(out[0]!.sr_request_preview).toBeUndefined();
    expect(
      applyRequestPreview(list, 'MESSAGE_DELETE_BULK', { ids: ['3'], channel_id: 'd1' }, '1'),
    ).toBe(list);
  });

  it('rebuilds the text when that message is edited, with the excerpt rule', () => {
    const list = request();
    const out = applyRequestPreview(
      list,
      'MESSAGE_UPDATE',
      { id: '9', channel_id: 'd1', content: '  edited\n\nwords ' },
      '1',
    );
    expect(out[0]!.sr_request_preview).toEqual({ ...preview, content: 'edited words' });
    expect(
      applyRequestPreview(list, 'MESSAGE_UPDATE', { id: '8', channel_id: 'd1', content: 'x' }, '1'),
    ).toBe(list);
    // An update that carries no content (an embed arriving) changes nothing.
    expect(applyRequestPreview(list, 'MESSAGE_UPDATE', { id: '9', channel_id: 'd1' }, '1')).toBe(
      list,
    );
  });

  it('leaves a preview without an id alone (an older answer in a cached state)', () => {
    const old = [
      {
        id: 'd1',
        type: 1,
        recipients: [dan],
        is_message_request: true,
        sr_request_preview: { content: 'x', attachments: 0, timestamp: 't' },
      },
    ] as never;
    expect(applyRequestPreview(old, 'MESSAGE_DELETE', { id: '9', channel_id: 'd1' }, '1')).toBe(
      old,
    );
    expect(
      applyRequestPreview(old, 'MESSAGE_UPDATE', { id: '9', channel_id: 'd1', content: 'y' }, '1'),
    ).toBe(old);
  });

  describe('on the page', () => {
    const at = (path: string) => {
      const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
      render(<RouterProvider router={router} />);
    };
    beforeEach(() => {
      resetStore();
      localStorage.clear();
      useLoungeUi.setState(useLoungeUi.getInitialState(), true);
      useSocial.getState().reset();
      useStore.setState({
        currentUserId: '1',
        users: { '1': { id: '1', username: 'me' } },
        guilds: {},
        guildOrder: [],
        channels: {},
        channelIdsByGuild: { '@me': [] },
        connection: 'connected',
      });
      server.use(
        http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
        http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
      );
    });

    it('the line is gone at once, and in Likely spam so is the Show message button', async () => {
      useStore.setState({
        messageRequestDms: request({ sr_request_reason: 'spam' }).map((d, i) =>
          i === 1
            ? { ...d, sr_request_preview: { ...preview, message_id: '10', content: 'two' } }
            : d,
        ) as never,
      });
      const stop = startPreviewsLive();
      try {
        at('/channels/@me');
        await userEvent.click(await screen.findByRole('button', { name: /Message Requests/ }));
        const page = await screen.findByRole('region', { name: 'Message Requests' });
        expect(
          within(page).getByRole('button', { name: 'Show the message from dan' }),
        ).toBeInTheDocument();
        expect(within(page).getByText('two')).toBeInTheDocument();
        act(() => emitDispatch('MESSAGE_DELETE', { id: '9', channel_id: 'd1' }));
        expect(
          within(page).queryByRole('button', { name: 'Show the message from dan' }),
        ).toBeNull();
        // The other row is untouched, and the first stays (as a request without a message).
        expect(within(page).getByText('two')).toBeInTheDocument();
        expect(within(page).getByText('dan')).toBeInTheDocument();
      } finally {
        stop();
      }
    });
  });
});

describe('formatRelativeTime (D-700)', () => {
  const now = Date.parse('2026-11-14T12:00:00.000Z');
  const ago = (ms: number) => new Date(now - ms).toISOString();
  const S = 1000;
  const M = 60 * S;
  const H = 60 * M;
  const D = 24 * H;

  it.each([
    [0, 'just now'],
    [59 * S, 'just now'],
    [60 * S, '1m ago'],
    [59 * M, '59m ago'],
    [59 * M + 59 * S, '59m ago'],
    [60 * M, '1h ago'],
    [23 * H, '23h ago'],
    [23 * H + 59 * M, '23h ago'],
    [24 * H, '1d ago'],
    [30 * D, '30d ago'],
    [31 * D - 1, '30d ago'],
    [31 * D, 'more than 30 days ago'],
    [400 * D, 'more than 30 days ago'],
  ])('%i ms ago reads %s', (ms, text) => {
    expect(formatRelativeTime(ago(ms), now)).toBe(text);
  });

  it('reads a time in the future (a clock that is off) as just now', () => {
    expect(formatRelativeTime(ago(-5 * M), now)).toBe('just now');
    expect(formatRelativeTime(ago(-3 * D), now)).toBe('just now');
  });
});

describe('the shared minute tick', () => {
  function Row({ id }: { id: number }) {
    const now = useMinuteTick();
    return <p data-testid={`row${id}`}>{formatRelativeTime('2026-11-14T12:00:00.000Z', now)}</p>;
  }

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-11-14T12:00:00.000Z'));
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('runs one interval for many rows and none after the last unmounts', () => {
    const set = vi.spyOn(globalThis, 'setInterval');
    const clear = vi.spyOn(globalThis, 'clearInterval');
    const a = render(<Row id={1} />);
    const b = render(
      <>
        <Row id={2} />
        <Row id={3} />
      </>,
    );
    expect(set).toHaveBeenCalledTimes(1);
    a.unmount();
    expect(clear).not.toHaveBeenCalled();
    b.unmount();
    expect(clear).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
    // A later row starts a fresh one.
    const c = render(<Row id={4} />);
    expect(set).toHaveBeenCalledTimes(2);
    c.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('moves a row from just now to 1m ago', () => {
    render(<Row id={1} />);
    expect(screen.getByTestId('row1')).toHaveTextContent('just now');
    act(() => {
      vi.advanceTimersByTime(60_000);
    });
    expect(screen.getByTestId('row1')).toHaveTextContent('1m ago');
    act(() => {
      vi.advanceTimersByTime(60 * 60_000);
    });
    expect(screen.getByTestId('row1')).toHaveTextContent('1h ago');
  });

  it('pauses while the document is hidden and fires once when it is visible again', () => {
    const state = vi.spyOn(document, 'visibilityState', 'get');
    render(<Row id={1} />);
    state.mockReturnValue('hidden');
    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });
    expect(vi.getTimerCount()).toBe(0);
    act(() => {
      vi.advanceTimersByTime(5 * 60_000);
    });
    expect(screen.getByTestId('row1')).toHaveTextContent('just now');
    state.mockReturnValue('visible');
    act(() => {
      document.dispatchEvent(new Event('visibilitychange'));
    });
    expect(screen.getByTestId('row1')).toHaveTextContent('5m ago');
    expect(vi.getTimerCount()).toBe(1);
  });
});

describe('the thread row shows the relative time', () => {
  it('reads "just now", then "1m ago", keeping the full time in the title', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-11-14T12:00:00.000Z'));
    try {
      const { container } = render(
        <ThreadChip
          thread={{
            id: 't1',
            name: 'Plans',
            messageCount: 3,
            last: {
              id: '50',
              author: { id: '2', displayName: 'Bobby' },
              content: 'see you at noon',
              attachments: 0,
              createdAt: '2026-11-14T11:59:50.000000+00:00',
            },
          }}
          onOpen={() => undefined}
        />,
      );
      const time = container.querySelector('time')!;
      expect(time).toHaveTextContent('just now');
      expect(time.getAttribute('datetime')).toBe('2026-11-14T11:59:50.000000+00:00');
      expect(time.getAttribute('title')).toBeTruthy();
      act(() => {
        vi.advanceTimersByTime(60_000);
      });
      expect(container.querySelector('time')).toHaveTextContent('1m ago');
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('the "started a thread" row carries the chip (D-701)', () => {
  const last = (id: string, content: string): S.ThreadLastMessage => ({
    id,
    author: { id: '2', username: 'bob' },
    content,
    attachments: 0,
    timestamp: '2026-11-14T10:00:00.000000+00:00',
  });
  const row = (thread?: S.Message['thread']): S.Message => ({
    id: '20',
    channel_id: 'c1',
    author: bob,
    content: 'Plans',
    timestamp: '2026-11-14T09:00:00.000000+00:00',
    type: 18,
    message_reference: { message_id: 't1', channel_id: 't1' },
    ...(thread ? { thread } : {}),
  });
  const withChip = (l?: S.ThreadLastMessage): S.Message['thread'] => ({
    id: 't1',
    name: 'Plans',
    message_count: 2,
    ...(l ? { sr_last_message: l } : {}),
  });
  const created = (id: string, content: string, extra: Record<string, unknown> = {}) => ({
    id,
    channel_id: 't1',
    author: { id: '3', username: 'cat' },
    content,
    attachments: [],
    timestamp: '2026-11-14T11:00:00.000000+00:00',
    ...extra,
  });

  it('maps the REST object onto the row, and renders the chip under the sentence', () => {
    const m = toLoungeMessage(row(withChip(last('50', 'latest'))));
    expect(m.thread).toMatchObject({ id: 't1', name: 'Plans', last: { content: 'latest' } });
    render(
      <SystemMessage
        message={m}
        referenceId={m.replyToId}
        referenceChannelId={m.referenceChannelId}
        onOpenThread={() => undefined}
      />,
    );
    const chip = document.querySelector('.lng-thread-chip')!;
    expect(chip).toBeTruthy();
    expect(chip.textContent).toContain('latest');
    // The sentence keeps its text link.
    expect(screen.getAllByRole('button', { name: /Plans/ }).length).toBeGreaterThanOrEqual(2);
  });

  it('renders the plain row when the thread is gone', () => {
    const m = toLoungeMessage(row());
    expect(m.thread).toBeUndefined();
    render(
      <SystemMessage
        message={m}
        referenceId={m.replyToId}
        referenceChannelId={m.referenceChannelId}
        onOpenThread={() => undefined}
      />,
    );
    expect(document.querySelector('.lng-thread-chip')).toBeNull();
    expect(screen.getByRole('button', { name: 'Plans' })).toBeInTheDocument();
  });

  it('a live reply in the thread moves the chip of the row', () => {
    const messages = { '20': row(withChip(last('50', 'old'))) };
    const out = applyThreadPreview(messages, 'MESSAGE_CREATE', created('60', 'fresh words'));
    expect(out['20']!.thread!.sr_last_message).toMatchObject({ id: '60', content: 'fresh words' });
    // A delete of that message drops it, an edit rebuilds it.
    const gone = applyThreadPreview(out, 'MESSAGE_DELETE', { id: '60', channel_id: 't1' });
    expect(gone['20']!.thread!.sr_last_message).toBeUndefined();
    const edited = applyThreadPreview(out, 'MESSAGE_UPDATE', created('60', 'fixed'));
    expect(edited['20']!.thread!.sr_last_message!.content).toBe('fixed');
  });

  it('a row without a thread is left alone', () => {
    const messages = { '20': row() };
    expect(applyThreadPreview(messages, 'MESSAGE_CREATE', created('60', 'x'))).toBe(messages);
  });

  it('shows the chip once: under the starter when the page holds both', () => {
    const starter: S.Message = {
      id: 't1',
      channel_id: 'c1',
      author: bob,
      content: 'start',
      timestamp: 'x',
      thread: withChip(last('50', 'old')),
    };
    // The server left the chip off the system row of this page.
    const messages = { t1: starter, '20': row() };
    const out = applyThreadPreview(messages, 'MESSAGE_CREATE', created('60', 'newer'));
    expect(out.t1!.thread!.sr_last_message!.id).toBe('60');
    expect(out['20']!.thread).toBeUndefined();
    // Both holding the thread (a cached state): the starter wins, the row is not touched.
    const both = { t1: starter, '20': row(withChip(last('50', 'old'))) };
    const next = applyThreadPreview(both, 'MESSAGE_CREATE', created('60', 'newer'));
    expect(next.t1!.thread!.sr_last_message!.id).toBe('60');
    expect(next['20']).toBe(both['20']);
  });
});
