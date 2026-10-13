import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../test/server';
import { applyThreadDispatch, threadsOf, toThread, useThreads } from '../threads/threadStore';
import { suggestThreadName } from '../threads/CreateThreadDialog';
import { at, baseHandlers, bob, calls, msg, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const wireThread = (id: string, over: Record<string, unknown> = {}) => ({
  id,
  type: 11,
  guild_id: 'g1',
  parent_id: 'c1',
  name: `thread ${id}`,
  owner_id: '1',
  message_count: 3,
  member_count: 2,
  last_message_id: `${id}9`,
  thread_metadata: { archived: false, locked: false, archive_timestamp: '2026-01-01T00:00:00Z' },
  ...over,
});

describe('thread store', () => {
  beforeEach(() => useThreads.getState().reset());

  it('maps wire threads and applies gateway events', () => {
    expect(toThread({ id: '1' } as never)).toBeNull();
    applyThreadDispatch('GUILD_CREATE', { id: 'g1', threads: [wireThread('10')] });
    applyThreadDispatch('THREAD_LIST_SYNC', {
      guild_id: 'g1',
      threads: [wireThread('11')],
      members: [{ id: '11' }],
    });
    const s = useThreads.getState().byId;
    expect(s['10']).toMatchObject({ name: 'thread 10', parentId: 'c1', joined: false });
    expect(s['11']?.joined).toBe(true);
    applyThreadDispatch('THREAD_UPDATE', wireThread('10', { name: 'renamed' }));
    expect(useThreads.getState().byId['10']?.name).toBe('renamed');
    // A partial update keeps what it does not mention.
    applyThreadDispatch('THREAD_UPDATE', { id: '10', parent_id: 'c1', message_count: 9 });
    expect(useThreads.getState().byId['10']).toMatchObject({ name: 'renamed', messageCount: 9 });
    applyThreadDispatch('THREAD_MEMBER_UPDATE', { id: '10', user_id: '1' });
    expect(useThreads.getState().byId['10']?.joined).toBe(true);
    applyThreadDispatch('THREAD_DELETE', { id: '10', parent_id: 'c1' });
    expect(useThreads.getState().byId['10']).toBeUndefined();
  });

  it('lists active and archived threads of one channel, newest activity first', () => {
    useThreads.getState().upsert([
      wireThread('10'),
      wireThread('12', { last_message_id: '129' }),
      wireThread('13', {
        thread_metadata: { archived: true, locked: false, archive_timestamp: 'x' },
      }),
      wireThread('14', { parent_id: 'c2' }),
    ]);
    const byId = useThreads.getState().byId;
    expect(threadsOf(byId, 'c1').map((t) => t.id)).toEqual(['12', '10']);
    expect(threadsOf(byId, 'c1', { archived: true }).map((t) => t.id)).toEqual(['13']);
  });

  it('suggests a thread name from the first line of the message', () => {
    expect(suggestThreadName('**Bold** idea\nmore text')).toBe('Bold idea');
    expect(suggestThreadName('')).toBe('New Thread');
    expect(suggestThreadName('x'.repeat(300))).toHaveLength(100);
  });
});

describe('threads in the Lounge', () => {
  beforeEach(() => {
    baseHandlers();
    useThreads.getState().reset();
  });

  it('lists threads from the header button and opens one beside the channel', async () => {
    seedLounge({ messages: [msg('100', { content: 'parent message' })] });
    server.use(
      http.get('*/api/v1/guilds/g1/threads/active', () =>
        HttpResponse.json({
          threads: [wireThread('t1', { name: 'Planning' })],
          members: [],
          has_more: false,
        }),
      ),
      http.get('*/api/v1/channels/t1/messages', () =>
        HttpResponse.json([{ ...msg('t11', { content: 'inside the thread' }), channel_id: 't1' }]),
      ),
      http.put('*/api/v1/channels/t1/thread-members/@me', async (ctx) => {
        await record('PUT')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const { container } = at('/channels/g1/c1');
    await screen.findByRole('log', { name: 'Messages in general' });
    await userEvent.click(screen.getByRole('button', { name: 'Threads' }));
    const row = await screen.findByRole('button', { name: /Planning/ });
    expect(row).toHaveTextContent('3 messages');
    await userEvent.click(row);
    const pane = await screen.findByRole('complementary', { name: 'Thread: Planning' });

    expect(await within(pane).findByText('inside the thread')).toBeInTheDocument();
    expect(within(pane).getByRole('textbox', { name: 'Message Planning' })).toBeInTheDocument();
    await userEvent.click(within(pane).getByRole('button', { name: 'Follow' }));
    expect(await within(pane).findByRole('button', { name: 'Following' })).toBeInTheDocument();
    expect(calls.some((c) => c.key === 'PUT /channels/t1/thread-members/@me')).toBe(true);
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(within(pane).getByRole('button', { name: 'Close thread' }));
    await waitFor(() =>
      expect(screen.queryByRole('complementary', { name: /Thread:/ })).not.toBeInTheDocument(),
    );
  });

  it('creates a thread from a message and opens it', async () => {
    seedLounge({ messages: [msg('100', { content: 'Lets plan the trip\nwith details' })] });
    server.use(
      http.post('*/api/v1/channels/c1/messages/100/threads', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(wireThread('100', { name: 'Lets plan the trip', member: {} }), {
          status: 201,
        });
      }),
      http.get('*/api/v1/channels/100/messages', () => HttpResponse.json([])),
    );
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    fireEvent.contextMenu(await within(log).findByText(/Lets plan the trip/));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Create Thread' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create Thread' });
    const name = within(dialog).getByRole('textbox', { name: 'Thread name' });
    expect(name).toHaveValue('Lets plan the trip');
    await userEvent.clear(name);
    await userEvent.type(name, 'Trip plans');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create Thread' }));
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'POST /channels/c1/messages/100/threads')?.body).toEqual({
        name: 'Trip plans',
        auto_archive_duration: 1440,
      }),
    );
    expect(await screen.findByRole('complementary', { name: /Thread:/ })).toBeInTheDocument();
  });

  it('shows a thread chip on the starter message and hides Create Thread inside a thread', async () => {
    seedLounge({
      messages: [
        msg('100', {
          content: 'has a thread',
          thread: { id: '100', name: 'Side chat', message_count: 4 },
        }),
      ],
    });
    server.use(
      http.get('*/api/v1/channels/100', () =>
        HttpResponse.json(wireThread('100', { name: 'Side chat' })),
      ),
      http.get('*/api/v1/channels/100/messages', () => HttpResponse.json([])),
    );
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    const chip = await within(log).findByRole('button', { name: /Side chat/ });
    expect(chip).toHaveTextContent('4 messages');
    fireEvent.contextMenu(within(log).getByText('has a thread'));
    expect(await screen.findByRole('menuitem', { name: 'Open Thread' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: 'Create Thread' })).not.toBeInTheDocument();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    await userEvent.click(chip);
    const pane = await screen.findByRole('complementary', { name: /Thread: Side chat/ });
    expect(pane).toBeInTheDocument();
    expect(bob.id).toBe('2');
  });
});
