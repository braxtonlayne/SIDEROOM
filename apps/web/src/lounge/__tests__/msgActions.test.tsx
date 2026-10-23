import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { useStore } from '../../store';
import { holdInteractions } from '../../test/interactions';
import { http, HttpResponse, server } from '../../test/server';
import { warmLoungeRoute } from '../../test/warmRoutes';
import { interactionConfig } from '../msg/Components';
import { findMessageLinks } from '../msg/MessageLinkCard';
import { reactorSentence, resetReactorNames } from '../msg/Reactions';
import { JOIN_LINES, joinLineIndex } from '../msg/SystemMessage';
import { useSaved } from '../shell/savedMessages';
import { at, baseHandlers, bob, calls, me, msg, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const THUMBS_UP = '\u{1F44D}';

// The first test renders the lazy Lounge route: load its module graph once, outside every test's budget.
warmLoungeRoute();

describe('reactions, menu actions, system rows, selects and links', () => {
  beforeEach(() => {
    baseHandlers();
    resetReactorNames();
    useSaved.getState().reset();
    interactionConfig.timeoutMs = 60;
  });

  it('shows who reacted in a tooltip and adds a reaction from the inline button', async () => {
    seedLounge({
      messages: [
        msg('100', {
          content: 'react to me',
          reactions: [{ emoji: { name: THUMBS_UP }, count: 5, me: false }],
        }),
      ],
    });
    server.use(
      http.get('*/api/v1/channels/c1/messages/100/reactions/:emoji', () =>
        HttpResponse.json([bob, me, { id: '3', username: 'cy' }, { id: '4', username: 'di' }]),
      ),
      http.put('*/api/v1/channels/c1/messages/100/reactions/:emoji/@me', async (ctx) => {
        await record('PUT')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    const pill = await within(log).findByRole('button', { name: /5 reactions/ });
    await userEvent.hover(pill);
    expect(
      await screen.findByText(
        `Bob, Me Myself, cy and 2 others reacted with ${THUMBS_UP}`,
        {},
        { timeout: 3000 },
      ),
    ).toBeInTheDocument();
    await userEvent.click(
      within(log).getByRole('button', { name: 'Add reaction to this message' }),
    );
    await userEvent.click(await screen.findByRole('button', { name: 'fire' }));
    await waitFor(() =>
      expect(calls.some((c) => c.key.startsWith('PUT /channels/c1/messages/100/reactions/'))).toBe(
        true,
      ),
    );
  });

  it('formats the reactor sentence', () => {
    expect(reactorSentence([], 0, 'x')).toBe('');
    expect(reactorSentence(['Ann'], 1, 'x')).toBe('Ann reacted with x');
    expect(reactorSentence(['Ann', 'Bob'], 2, 'x')).toBe('Ann and Bob reacted with x');
    expect(reactorSentence(['Ann', 'Bob', 'Cy', 'Di'], 7, 'x')).toBe(
      'Ann, Bob, Cy and 4 others reacted with x',
    );
    expect(reactorSentence([], 3, 'x')).toBe('3 people reacted with x');
  });

  it('marks a message unread: manual ack, the divider moves, acks hold', async () => {
    seedLounge({ messages: [msg('100', { content: 'first' }), msg('101', { content: 'second' })] });
    server.use(
      http.post('*/api/v1/channels/c1/messages/:mid/ack', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json({ token: null });
      }),
    );
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    await within(log).findByText('second');
    fireEvent.contextMenu(within(log).getByText('second'));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Mark Unread' }));
    await waitFor(() =>
      expect(
        calls.some(
          (c) =>
            c.key === 'POST /channels/c1/messages/100/ack' &&
            JSON.stringify(c.body) === JSON.stringify({ manual: true, mention_count: 0 }),
        ),
      ).toBe(true),
    );
    expect(await within(log).findByRole('separator', { name: 'New messages' })).toBeInTheDocument();
    expect(useStore.getState().readStates['c1']?.last_message_id).toBe('100');
  });

  it('bookmarks and sets a reminder through the server', async () => {
    seedLounge({ messages: [msg('100', { content: 'remember this' })] });
    const calls: string[] = [];
    let reminder: Record<string, unknown> = {};
    server.use(
      http.put('*/api/v1/channels/c1/messages/100/bookmark', () => {
        calls.push('PUT bookmark');
        return HttpResponse.json({
          message_id: '100',
          channel_id: 'c1',
          guild_id: 'g1',
          note: '',
          created_at: '',
        });
      }),
      http.delete('*/api/v1/users/@me/bookmarks/100', () => {
        calls.push('DELETE bookmark');
        return new HttpResponse(null, { status: 204 });
      }),
      http.post('*/api/v1/users/@me/reminders', async ({ request }) => {
        reminder = (await request.json()) as Record<string, unknown>;
        calls.push('POST reminder');
        return HttpResponse.json({
          id: '1',
          note: reminder.note,
          remind_at: reminder.remind_at,
          status: 'pending',
          channel_id: 'c1',
          guild_id: 'g1',
          message_id: '100',
          created_at: '',
        });
      }),
    );
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    fireEvent.contextMenu(await within(log).findByText('remember this'));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Bookmark Message' }));
    await waitFor(() => expect(Object.keys(useSaved.getState().bookmarks)).toEqual(['100']));
    fireEvent.contextMenu(within(log).getByText('remember this'));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Remove Bookmark' }));
    await waitFor(() => expect(useSaved.getState().bookmarks).toEqual({}));
    fireEvent.contextMenu(within(log).getByText('remember this'));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Remind Me' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'in 1 hour' }));
    await waitFor(() => expect(useSaved.getState().reminders).toHaveLength(1));
    expect(Date.parse(reminder.remind_at as string) - Date.now()).toBeGreaterThan(55 * 60_000);
    expect(reminder).toMatchObject({ note: 'remember this' });
    expect(calls).toEqual(['PUT bookmark', 'DELETE bookmark', 'POST reminder']);
  });

  it('renders thread, call, follow, support and join rows', async () => {
    seedLounge({
      messages: [
        msg('100', {
          type: 18,
          content: 'Planning',
          message_reference: { message_id: '555', channel_id: '555' },
        }),
        msg('101', { type: 3 }),
        msg('102', { type: 12, content: 'News' }),
        msg('103', { type: 8 }),
        msg('104', { type: 7 }),
      ],
    });
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    expect(await within(log).findByRole('button', { name: 'Planning' })).toBeInTheDocument();
    expect(within(log).getByText(/started a call/)).toBeInTheDocument();
    expect(within(log).getByText(/Its announcements will be posted here/)).toBeInTheDocument();
    expect(within(log).getByText(/supported this server/)).toBeInTheDocument();
    expect(log.querySelectorAll('[data-system-type]')).toHaveLength(5);
  });

  it('picks join lines from the message id and covers the whole set', () => {
    const idx = new Set(Array.from({ length: 200 }, (_, i) => joinLineIndex(String(1000 + i))));
    expect(idx.size).toBe(JOIN_LINES.length);
    expect(joinLineIndex('104')).toBe(joinLineIndex('104'));
  });

  it('lists members, roles and channels in the select menus, with a loading state', async () => {
    seedLounge({
      messages: [
        msg('100', {
          content: 'pick',
          components: [
            {
              type: 1,
              components: [
                { type: 5, custom_id: 'u', placeholder: 'Choose a person' },
                { type: 6, custom_id: 'r', placeholder: 'Choose a role' },
                { type: 8, custom_id: 'c', placeholder: 'Choose a channel' },
              ],
            },
          ],
        }),
      ],
      roles: [{ id: 'r1', guild_id: 'g1', name: 'Mods', color: 0, position: 1, permissions: '0' }],
    });
    const held = holdInteractions();
    const { container } = at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    const person = await within(log).findByRole('combobox', { name: 'Choose a person' });
    expect(within(person).getByRole('option', { name: 'Bob' })).toBeInTheDocument();
    const role = within(log).getByRole('combobox', { name: 'Choose a role' });
    expect(within(role).getByRole('option', { name: 'Mods' })).toBeInTheDocument();
    expect(within(role).queryByRole('option', { name: '@everyone' })).not.toBeInTheDocument();
    const channel = within(log).getByRole('combobox', { name: 'Choose a channel' });
    expect(within(channel).getByRole('option', { name: '#random' })).toBeInTheDocument();
    expect(
      within(channel).queryByRole('option', { name: 'Text Channels' }),
    ).not.toBeInTheDocument();
    await userEvent.selectOptions(channel, '#random');
    expect(channel).toHaveAttribute('aria-busy', 'true');
    await held.fail();
    expect(await screen.findByText('This interaction failed')).toBeInTheDocument();
    await waitFor(() => expect(channel).not.toHaveAttribute('aria-busy'));
    expect(await axeViolations(container)).toEqual([]);
  });

  it('previews a linked message and hides the card when it cannot be read', async () => {
    seedLounge({
      messages: [
        msg('100', {
          content: `see ${location.origin}/channels/g1/c2/900 and ${location.origin}/channels/g1/c2/901`,
        }),
      ],
    });
    server.use(
      http.get('*/api/v1/channels/c2/messages/900', () =>
        HttpResponse.json({
          ...msg('900', { content: 'the linked words', channel_id: 'c2' }),
          author: bob,
        }),
      ),
      http.get('*/api/v1/channels/c2/messages/901', () => new HttpResponse(null, { status: 403 })),
    );
    at('/channels/g1/c1');
    const card = await screen.findByRole('region', { name: 'Linked message from Bob' });
    expect(within(card).getByText('the linked words')).toBeInTheDocument();
    expect(within(card).getByText('#random')).toBeInTheDocument();
    expect(screen.getAllByRole('region', { name: /Linked message/ })).toHaveLength(1);
    expect(findMessageLinks('x https://other.example/channels/1/2/3', location.origin)).toEqual([]);
  });
});
