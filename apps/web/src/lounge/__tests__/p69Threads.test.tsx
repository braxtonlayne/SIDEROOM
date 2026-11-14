// SEC-AUD-232..233 (wave 69, D-715, D-718): the live "started a thread" row has its chip, and a thread's count
// follows live and reads "50+" at the server's cap.
import { act, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { resetStore, useStore } from '../../store';
import type * as S from '../../store/types';
import { applyThreadCreatedRow, startPreviewsLive } from '../previewLive';
import {
  applyThreadDispatch,
  COUNT_CAP,
  startThreadsLive,
  useThreads,
  type ThreadInfo,
} from '../threads/threadStore';
import { threadCountLabel } from '../threads/threadCount';
import { ThreadChip } from '../threads/ThreadChip';
import { SystemMessage } from '../msg/SystemMessage';
import { toLoungeMessage } from '../liveMap';
import { t } from '../../i18n/runtime';
import { at, baseHandlers, bob, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const wire = (id: string, over: Record<string, unknown> = {}) => ({
  id,
  type: 11,
  guild_id: 'g1',
  parent_id: 'c1',
  name: `thread ${id}`,
  owner_id: '2',
  message_count: 0,
  member_count: 1,
  thread_metadata: { archived: false, locked: false, archive_timestamp: '2026-01-01T00:00:00Z' },
  ...over,
});

const info = (id: string, over: Partial<ThreadInfo> = {}): ThreadInfo => ({
  id,
  parentId: 'c1',
  guildId: 'g1',
  name: `thread ${id}`,
  ownerId: '2',
  archived: false,
  locked: false,
  messageCount: 0,
  memberCount: 1,
  joined: false,
  lastMessageId: null,
  archiveTimestamp: null,
  isPrivate: false,
  slowmode: 0,
  autoArchive: 1440,
  appliedTags: [],
  pinned: false,
  ...over,
});

const row = (id: string, ref: string, extra: Partial<S.Message> = {}): S.Message => ({
  id,
  channel_id: 'c1',
  author: bob,
  content: 'Plans',
  timestamp: '2026-11-14T09:00:00.000000+00:00',
  type: 18,
  message_reference: { message_id: ref, channel_id: ref },
  ...extra,
});

const starter = (id: string, extra: Partial<S.Message> = {}): S.Message => ({
  id,
  channel_id: 'c1',
  author: bob,
  content: 'start here',
  timestamp: '2026-11-14T08:00:00.000000+00:00',
  ...extra,
});

const holds = (...ts: ThreadInfo[]) => Object.fromEntries(ts.map((x) => [x.id, x]));

describe('the live "started a thread" row has its chip (D-715)', () => {
  beforeEach(() => {
    resetStore();
    useThreads.getState().reset();
    useStore.setState({
      currentUserId: '1',
      users: { '1': { id: '1', username: 'me' }, '2': bob },
    });
  });

  it('the gateway gives a type 18 row no thread; the live listeners fill it from the thread', () => {
    const stops = [startThreadsLive(), startPreviewsLive()];
    try {
      const { applyDispatch } = useStore.getState();
      // THREAD_CREATE first, then the row (routes/threads/tests.rs: "thread", "message").
      act(() => {
        applyDispatch('THREAD_CREATE', wire('t1', { name: 'Plans' }));
        applyDispatch('MESSAGE_CREATE', row('20', 't1'));
      });
      expect(useStore.getState().messages['20']!.thread).toEqual({
        id: 't1',
        name: 'Plans',
        message_count: 0,
      });
    } finally {
      stops.forEach((s) => s());
    }
  });

  it('gives the row the chip from the thread the store holds, in the wire shape', () => {
    const messages = { '20': row('20', 't1') };
    const out = applyThreadCreatedRow(
      messages,
      holds(info('t1', { name: 'Plans', messageCount: 2 })),
      'MESSAGE_CREATE',
      messages['20'],
    );
    expect(out['20']!.thread).toEqual({ id: 't1', name: 'Plans', message_count: 2 });
  });

  it('works in both orders: thread first, then the row; or the row first, then the thread', () => {
    const first = applyThreadCreatedRow(
      { '20': row('20', 't1') },
      holds(info('t1', { name: 'Plans' })),
      'MESSAGE_CREATE',
      row('20', 't1'),
    );
    expect(first['20']!.thread?.id).toBe('t1');
    // Row first: the store does not hold the thread yet, so nothing; THREAD_CREATE then finds the row.
    const messages = { '20': row('20', 't1') };
    expect(applyThreadCreatedRow(messages, {}, 'MESSAGE_CREATE', messages['20'])).toBe(messages);
    const later = applyThreadCreatedRow(
      messages,
      {},
      'THREAD_CREATE',
      wire('t1', { name: 'Plans' }),
    );
    expect(later['20']!.thread).toEqual({ id: 't1', name: 'Plans', message_count: 0 });
  });

  it('puts the chip under the starter when the page holds it, and not on the row', () => {
    const messages = { t1: starter('t1'), '20': row('20', 't1') };
    const out = applyThreadCreatedRow(
      messages,
      holds(info('t1')),
      'MESSAGE_CREATE',
      messages['20'],
    );
    expect(out.t1!.thread?.id).toBe('t1');
    expect(out['20']!.thread).toBeUndefined();
    // THREAD_CREATE with the starter on the page: the same.
    const early = applyThreadCreatedRow({ t1: starter('t1') }, {}, 'THREAD_CREATE', wire('t1'));
    expect(early.t1!.thread?.id).toBe('t1');
  });

  it('a starter that gets its thread later makes the row give up its own', () => {
    const messages = {
      '20': row('20', 't1', { thread: { id: 't1', name: 'Plans', message_count: 0 } }),
    };
    const out = applyThreadCreatedRow(messages, {}, 'MESSAGE_UPDATE', {
      id: 't1',
      channel_id: 'c1',
      thread: { id: 't1', name: 'Plans' },
    });
    expect('thread' in out['20']!).toBe(false);
    // An update of another message, or one without a thread, changes nothing.
    expect(
      applyThreadCreatedRow(messages, {}, 'MESSAGE_UPDATE', { id: 't9', thread: { id: 't9' } }),
    ).toBe(messages);
    expect(applyThreadCreatedRow(messages, {}, 'MESSAGE_UPDATE', { id: 't1', content: 'x' })).toBe(
      messages,
    );
  });

  it('through the store: the starter arriving with its thread leaves one chip', () => {
    const stop = startPreviewsLive();
    try {
      const { applyDispatch } = useStore.getState();
      act(() => {
        useThreads.getState().upsert([wire('t1', { name: 'Plans' })]);
        applyDispatch('MESSAGE_CREATE', row('20', 't1'));
      });
      expect(useStore.getState().messages['20']!.thread?.id).toBe('t1');
      act(() => {
        useStore.setState((s) => ({ messages: { ...s.messages, t1: starter('t1') } }));
        applyDispatch('MESSAGE_UPDATE', {
          id: 't1',
          channel_id: 'c1',
          content: 'start here',
          thread: { id: 't1', name: 'Plans', message_count: 0 },
        });
      });
      const m = useStore.getState().messages;
      expect(m.t1!.thread?.id).toBe('t1');
      expect(m['20']!.thread).toBeUndefined();
    } finally {
      stop();
    }
  });

  it('draws the row without a chip while the starter on the page shows one', () => {
    const m = toLoungeMessage(row('20', 't1', { thread: { id: 't1', name: 'Plans' } }));
    useStore.setState({ messages: { t1: starter('t1', { thread: { id: 't1', name: 'Plans' } }) } });
    render(
      <SystemMessage
        message={m}
        referenceId={m.replyToId}
        referenceChannelId={m.referenceChannelId}
        onOpenThread={() => undefined}
      />,
    );
    expect(document.querySelector('.lng-thread-chip')).toBeNull();
  });

  it('leaves private threads, held chips, unknown threads and other rows alone', () => {
    const messages = {
      '20': row('20', 't1'),
      '21': row('21', 't2', { thread: { id: 't2', name: 'kept', message_count: 5 } }),
      '22': row('22', 't3'),
      '23': msg('23', { type: 0, content: 'plain' }),
    };
    const threads = holds(info('t1', { isPrivate: true }), info('t2', { name: 'other' }));
    for (const id of ['20', '21', '22', '23'] as const) {
      expect(applyThreadCreatedRow(messages, threads, 'MESSAGE_CREATE', messages[id])).toBe(
        messages,
      );
    }
    // THREAD_CREATE of a private thread (type 12) never gives a chip either.
    expect(applyThreadCreatedRow(messages, {}, 'THREAD_CREATE', wire('t1', { type: 12 }))).toBe(
      messages,
    );
  });

  it('a live reply then moves the chip on the row (applyThreadPreview finds it)', () => {
    const stop = startPreviewsLive();
    try {
      const { applyDispatch } = useStore.getState();
      act(() => {
        useThreads.getState().upsert([wire('t1', { name: 'Plans' })]);
        applyDispatch('MESSAGE_CREATE', row('20', 't1'));
        applyDispatch('MESSAGE_CREATE', {
          id: '60',
          channel_id: 't1',
          author: { id: '3', username: 'cat' },
          content: 'fresh words',
          attachments: [],
          timestamp: '2026-11-14T11:00:00.000000+00:00',
        });
      });
      expect(useStore.getState().messages['20']!.thread!.sr_last_message).toMatchObject({
        id: '60',
        content: 'fresh words',
      });
    } finally {
      stop();
    }
  });

  it('a deleted thread takes the chip away, from the row and from the starter', () => {
    const chip = { id: 't1', name: 'Plans', message_count: 2 };
    const messages = {
      t1: starter('t1', { thread: chip }),
      '20': row('20', 't1', { thread: chip }),
      '30': row('30', 't2', { thread: { id: 't2', name: 'other' } }),
    };
    const out = applyThreadCreatedRow(messages, {}, 'THREAD_DELETE', { id: 't1' });
    expect(out.t1!.thread).toBeUndefined();
    expect(out['20']!.thread).toBeUndefined();
    expect(out['30']).toBe(messages['30']);
    expect(applyThreadCreatedRow(messages, {}, 'THREAD_DELETE', { id: 'zz' })).toBe(messages);
  });
});

describe('the count follows live and reads "50+" (D-718)', () => {
  beforeEach(() => useThreads.getState().reset());
  const hold = (n: number, last: string | null = null) =>
    useThreads.getState().upsert([wire('t1', { message_count: n, last_message_id: last })]);
  const count = () => useThreads.getState().byId['t1']?.messageCount;
  const created = (id: string, ch = 't1') =>
    applyThreadDispatch('MESSAGE_CREATE', { id, channel_id: ch, type: 0 });

  it('counts a message written in the thread, and moves the latest id', () => {
    hold(3, '100');
    created('101');
    expect(count()).toBe(4);
    expect(useThreads.getState().byId['t1']!.lastMessageId).toBe('101');
    // The same message again (a replay) is not counted twice.
    created('101');
    expect(count()).toBe(4);
  });

  it('counts every kind of message, as the server does', () => {
    hold(0);
    applyThreadDispatch('MESSAGE_CREATE', { id: '5', channel_id: 't1', type: 19 });
    applyThreadDispatch('MESSAGE_CREATE', { id: '6', channel_id: 't1', type: 21 });
    expect(count()).toBe(2);
  });

  it('takes one off for a delete and the number of ids for a bulk delete, never below 0', () => {
    hold(3, '100');
    applyThreadDispatch('MESSAGE_DELETE', { id: '90', channel_id: 't1' });
    expect(count()).toBe(2);
    applyThreadDispatch('MESSAGE_DELETE_BULK', { ids: ['1', '2', '3'], channel_id: 't1' });
    expect(count()).toBe(0);
    applyThreadDispatch('MESSAGE_DELETE', { id: '91', channel_id: 't1' });
    expect(count()).toBe(0);
  });

  it('stops at the cap: not raised over it, not lowered by a delete', () => {
    hold(COUNT_CAP - 1, '100');
    created('101');
    expect(count()).toBe(COUNT_CAP);
    created('102');
    expect(count()).toBe(COUNT_CAP);
    applyThreadDispatch('MESSAGE_DELETE', { id: '101', channel_id: 't1' });
    applyThreadDispatch('MESSAGE_DELETE_BULK', { ids: ['1', '2'], channel_id: 't1' });
    expect(count()).toBe(COUNT_CAP);
  });

  it('a THREAD_UPDATE (or a load) with a count replaces whatever the store has', () => {
    hold(COUNT_CAP);
    applyThreadDispatch('THREAD_UPDATE', wire('t1', { message_count: 7 }));
    expect(count()).toBe(7);
    created('500');
    expect(count()).toBe(8);
    useThreads.getState().upsert([wire('t1', { message_count: 20 })]);
    expect(count()).toBe(20);
  });

  it('a message in a channel that is not a held thread changes nothing', () => {
    hold(3);
    const before = useThreads.getState().byId;
    created('9', 'c1');
    applyThreadDispatch('MESSAGE_DELETE', { id: '9', channel_id: 'c1' });
    applyThreadDispatch('MESSAGE_DELETE_BULK', { ids: ['9'], channel_id: 'dm' });
    expect(useThreads.getState().byId).toBe(before);
  });

  it('the label has three forms', () => {
    expect(threadCountLabel(t, 1)).toBe('1 message');
    expect(threadCountLabel(t, 12)).toBe('12 messages');
    expect(threadCountLabel(t, 0)).toBe('0 messages');
    expect(threadCountLabel(t, 49)).toBe('49 messages');
    expect(threadCountLabel(t, 50)).toBe('50+ messages');
    expect(threadCountLabel(t, 400)).toBe('50+ messages');
  });

  it('the chip under a message shows the store count after a live message', () => {
    useThreads.getState().upsert([wire('t1', { message_count: 2, last_message_id: '100' })]);
    const m = toLoungeMessage(
      starter('t1', { thread: { id: 't1', name: 'Plans', message_count: 2 } }),
    );
    render(<ThreadChip thread={m.thread!} onOpen={() => undefined} />);
    expect(screen.getByRole('button')).toHaveTextContent('2 messages');
    act(() => applyThreadDispatch('MESSAGE_CREATE', { id: '101', channel_id: 't1' }));
    expect(screen.getByRole('button')).toHaveTextContent('3 messages');
    act(() => useThreads.getState().upsert([wire('t1', { message_count: 50 })]));
    expect(screen.getByRole('button')).toHaveTextContent('50+ messages');
  });

  it('a chip for a thread the store does not hold reads its own count', () => {
    const m = toLoungeMessage(
      starter('t9', { thread: { id: 't9', name: 'Other', message_count: 1 } }),
    );
    render(<ThreadChip thread={m.thread!} onOpen={() => undefined} />);
    expect(screen.getByRole('button')).toHaveTextContent('1 message');
    expect(screen.getByRole('button')).not.toHaveTextContent('1 messages');
  });
});

describe('the Threads list and a forum row show the live count', () => {
  beforeEach(() => {
    baseHandlers();
    useThreads.getState().reset();
  });

  it('the Threads popover moves with a live message and reads 50+ at the cap', async () => {
    seedLounge({ messages: [msg('100', { content: 'parent message' })] });
    server.use(
      http.get('*/api/v1/guilds/g1/threads/active', () =>
        HttpResponse.json({
          threads: [
            wire('t1', { name: 'Planning', message_count: 3, last_message_id: '9' }),
            wire('t2', { name: 'Huge', message_count: 50, last_message_id: '8' }),
          ],
          members: [],
          has_more: false,
        }),
      ),
    );
    at('/channels/g1/c1');
    await screen.findByRole('log', { name: 'Messages in general' });
    await userEvent.click(screen.getByRole('button', { name: 'Threads' }));
    const planning = await screen.findByRole('button', { name: /Planning/ });
    expect(planning).toHaveTextContent('3 messages');
    expect(screen.getByRole('button', { name: /Huge/ })).toHaveTextContent('50+ messages');
    act(() => applyThreadDispatch('MESSAGE_CREATE', { id: '10', channel_id: 't1' }));
    expect(screen.getByRole('button', { name: /Planning/ })).toHaveTextContent('4 messages');
  });

  it('a forum post row reads the same store', async () => {
    seedLounge({
      messages: [],
      channels: {
        f1: { id: 'f1', type: 15, guild_id: 'g1', name: 'help', topic: 'x', position: 1 },
      },
    });
    server.use(
      http.get('*/api/v1/channels/f1', () =>
        HttpResponse.json({ id: 'f1', type: 15, available_tags: [], flags: 0 }),
      ),
      http.get('*/api/v1/guilds/g1/threads/active', () =>
        HttpResponse.json({
          threads: [
            wire('p1', { parent_id: 'f1', name: 'How do I log in?', message_count: 1 }),
            wire('p2', { parent_id: 'f1', name: 'Crash', message_count: 50 }),
          ],
          members: [],
          has_more: false,
        }),
      ),
      http.get('*/api/v1/channels/f1/threads/archived/public', () =>
        HttpResponse.json({ threads: [], members: [], has_more: false }),
      ),
      http.get('*/api/v1/channels/:tid/messages', () => HttpResponse.json([])),
    );
    at('/channels/g1/f1');
    const list = await screen.findByRole('list', { name: 'Posts' });
    const first = (await within(list).findByText('How do I log in?')).closest('li')!;
    expect(first).toHaveTextContent('1 message');
    expect(within(list).getByText('Crash').closest('li')).toHaveTextContent('50+ messages');
    act(() => applyThreadDispatch('MESSAGE_CREATE', { id: '7', channel_id: 'p1' }));
    expect(first).toHaveTextContent('2 messages');
  });
});
