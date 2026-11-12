// SEC-AUD-036 (wave 54): a failed member action tells the member and leaves the screen in its true state; a failed
// history load is not shown as an empty channel.
import { act, render, renderHook, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { MessageKey } from '../../i18n';
import { useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { warmLoungeRoute } from '../../test/warmRoutes';
import { onActionError } from '../actionError';
import { ForYou } from '../inbox/ForYou';
import { LIVE_ACTIONS } from '../live';
import { MAX_MESSAGE_LENGTH } from '../composerText';
import { InlineEdit } from '../msg/InlineEdit';
import { PollView } from '../msg/Poll';
import { reactorName, reactorSentence } from '../msg/Reactions';
import { SystemMessage } from '../msg/SystemMessage';
import { NotificationDialog } from '../shell/NotificationDialog';
import { useNotifSettings } from '../shell/notifSettings';
import { useUserMenuEntries } from '../shell/userMenu';
import { RelationshipConfirmDialog } from '../shell/ConfirmAction';
import { useLoungeUi } from '../store';
import { at, baseHandlers, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

warmLoungeRoute();

const fail500 = () => HttpResponse.json({ message: 'boom' }, { status: 500 });

let reported: MessageKey[];
let unsubscribe: () => void;
beforeEach(() => {
  baseHandlers();
  reported = [];
  unsubscribe = onActionError((k) => reported.push(k));
});
afterEach(() => unsubscribe());

describe('message actions that fail', () => {
  it('a failed delete puts the message back, keeps messages that arrived meanwhile, and shows a toast', async () => {
    seedLounge({ messages: [msg('100', { content: 'keep me' })] });
    let release!: () => void;
    server.use(
      http.delete('*/api/v1/channels/c1/messages/100', async () => {
        await new Promise<void>((r) => (release = r));
        return fail500();
      }),
    );
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    expect(await within(log).findByText('keep me')).toBeInTheDocument();
    act(() => LIVE_ACTIONS.deleteMessage?.('c1', '100'));
    await waitFor(() => expect(within(log).queryByText('keep me')).toBeNull());
    // A message arrives while the delete is out.
    act(() =>
      useStore.getState().applyDispatch('MESSAGE_CREATE', {
        ...msg('101', { content: 'arrived meanwhile' }),
        guild_id: 'g1',
      }),
    );
    await waitFor(() => expect(release).toBeTypeOf('function'));
    release();
    expect(await screen.findByText('The message was not deleted. Try again.')).toBeInTheDocument();
    expect(await within(log).findByText('keep me')).toBeInTheDocument();
    expect(within(log).getByText('arrived meanwhile')).toBeInTheDocument();
    expect(useStore.getState().windows.c1!.ids).toEqual(['100', '101']);
  });

  it('a failed pin restores only the pin flag and reports it', async () => {
    seedLounge({ messages: [msg('100', { content: 'pin me', pinned: false })] });
    let release!: () => void;
    server.use(
      http.put('*/api/v1/channels/c1/pins/100', async () => {
        await new Promise<void>((r) => (release = r));
        return fail500();
      }),
    );
    LIVE_ACTIONS.pinMessage?.('c1', '100', true);
    expect(useStore.getState().messages['100']!.pinned).toBe(true);
    // An edit lands while the pin is out; the rollback must not undo it.
    useStore
      .getState()
      .applyDispatch('MESSAGE_UPDATE', { id: '100', channel_id: 'c1', content: 'edited' });
    await waitFor(() => expect(release).toBeTypeOf('function'));
    release();
    await waitFor(() => expect(reported).toEqual(['lounge.actionError.pin']));
    expect(useStore.getState().messages['100']!.pinned).toBe(false);
    expect(useStore.getState().messages['100']!.content).toBe('edited');
  });

  it('a failed reaction is rolled back and reported instead of being dropped silently', async () => {
    seedLounge({ messages: [msg('100', { content: 'x' })] });
    server.use(http.put('*/api/v1/channels/c1/messages/100/reactions/:emoji/@me', fail500));
    LIVE_ACTIONS.toggleReaction('c1', '100', 'x');
    await waitFor(() => expect(reported).toEqual(['lounge.actionError.react']));
    expect(useStore.getState().messages['100']!.reactions ?? []).toEqual([]);
  });

  it('a failed vote, end poll and edit are each reported', async () => {
    seedLounge({
      messages: [
        msg('100', {
          content: 'q',
          poll: {
            question: { text: 'Q' },
            answers: [{ answer_id: 1, poll_media: { text: 'A' } }],
            allow_multiselect: false,
            expiry: null,
          },
        } as never),
      ],
    });
    server.use(
      http.put('*/api/v1/channels/c1/polls/100/answers/1/voters/@me', fail500),
      http.post('*/api/v1/channels/c1/polls/100/expire', fail500),
      http.patch('*/api/v1/channels/c1/messages/100', fail500),
    );
    LIVE_ACTIONS.votePoll?.('c1', '100', 1, true);
    LIVE_ACTIONS.endPoll?.('c1', '100');
    LIVE_ACTIONS.editMessage?.('c1', '100', 'new text');
    await waitFor(() =>
      expect([...reported].sort()).toEqual([
        'lounge.actionError.edit',
        'lounge.actionError.endPoll',
        'lounge.actionError.vote',
      ]),
    );
  });
});

describe('history load', () => {
  it('a failed first load says so with Try again instead of "the start of #channel"', async () => {
    seedLounge({ messages: [] });
    useStore.setState((s) => {
      const windows = { ...s.windows };
      delete windows.c1;
      return { windows };
    });
    let fails = true;
    server.use(
      http.get('*/api/v1/channels/c1/messages', () =>
        fails ? fail500() : HttpResponse.json([msg('100', { content: 'hello again' })]),
      ),
    );
    at('/channels/g1/c1');
    expect(await screen.findByText('Messages could not be loaded.')).toBeInTheDocument();
    expect(screen.queryByText(/Welcome to #general/)).toBeNull();
    fails = false;
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByText('hello again')).toBeInTheDocument();
    expect(screen.queryByText('Messages could not be loaded.')).toBeNull();
  });
});

describe('menus and rows about other people', () => {
  it('Block, Ignore and Remove Friend failures from the user menu are reported', async () => {
    seedLounge({ messages: [] });
    server.use(
      http.put('*/api/v1/users/@me/relationships/2', fail500),
      http.put('*/api/v1/users/@me/ignores/2', fail500),
    );
    const { result } = renderHook(() => useUserMenuEntries({ id: '2', name: 'Bob' }));
    const byLabel = (label: string) =>
      result.current.relationship.find((e) => 'label' in e && e.label === label) as {
        onSelect: () => void;
      };
    byLabel('Block').onSelect();
    // The menu asks first (SEC-AUD-147): the confirm host does the block.
    expect(useLoungeUi.getState().dialog).toMatchObject({
      kind: 'confirmRelationship',
      action: 'block',
      userId: '2',
    });
    render(
      <RelationshipConfirmDialog action="block" userId="2" name="Bob" onClose={() => undefined} />,
    );
    await userEvent.click(
      within(await screen.findByRole('dialog')).getByRole('button', { name: 'Block' }),
    );
    await waitFor(() => expect(reported).toContain('lounge.friends.blockError'));
    byLabel('Ignore').onSelect();
    await waitFor(() => expect(reported).toContain('lounge.friends.ignoreError'));
  });

  it('a failed friend accept in the inbox keeps the row and reports it', async () => {
    seedLounge({ messages: [] });
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json([{ id: '7', type: 3, user: { id: '7', username: 'zed' } }]),
      ),
      http.get('*/api/v1/library/updates', () => HttpResponse.json({ items: [] })),
      http.get('*/api/v1/submissions', () =>
        HttpResponse.json({ items: [], total: 0, limit: 20, offset: 0 }),
      ),
      http.get('*/api/v1/sr/invites', fail500),
      http.put('*/api/v1/users/@me/relationships/7', fail500),
    );
    render(<ForYou onGo={() => undefined} />);
    await userEvent.click(
      await screen.findByRole('button', { name: 'Accept the friend request from zed' }),
    );
    await waitFor(() => expect(reported).toEqual(['lounge.friends.acceptError']));
    expect(screen.getByText('zed')).toBeInTheDocument();
  });
});

describe('notification settings dialog', () => {
  beforeEach(() => {
    seedLounge({ messages: [] });
    useNotifSettings.getState().reset();
  });

  it('a failed load shows an error with Try again instead of spinning forever', async () => {
    let fails = true;
    server.use(
      http.get('*/api/v1/users/@me/guilds/g1/settings', () =>
        fails ? fail500() : HttpResponse.json({ muted: false }),
      ),
    );
    render(<NotificationDialog guildId="g1" onClose={() => undefined} />);
    expect(
      await screen.findByText('Your notification settings for this server could not be loaded.'),
    ).toBeInTheDocument();
    fails = false;
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByRole('switch', { name: /Mute/ })).toBeInTheDocument();
  });

  it('a read that left before a mute cannot put the older settings back', async () => {
    let release!: () => void;
    server.use(
      http.get('*/api/v1/users/@me/guilds/g1/settings', async () => {
        await new Promise<void>((r) => (release = r));
        return HttpResponse.json({ muted: false });
      }),
      http.patch('*/api/v1/users/@me/guilds/g1/settings', () => HttpResponse.json({ muted: true })),
    );
    const load = useNotifSettings.getState().load('g1', true);
    await waitFor(() => expect(release).toBeTypeOf('function'));
    await useNotifSettings.getState().patchGuild('g1', { mute: { muted: true, seconds: 3600 } });
    expect(useNotifSettings.getState().byGuild.g1?.muted).toBe(true);
    release();
    expect(await load).toBe(true);
    expect(useNotifSettings.getState().byGuild.g1?.muted).toBe(true);
  });
});

describe('lists that failed are not shown as empty', () => {
  it('poll voters: loading and a failure are not "Nobody"', async () => {
    seedLounge({ messages: [] });
    server.use(http.get('*/api/v1/channels/c1/polls/100/answers/1', fail500));
    render(
      <PollView
        poll={{
          question: 'Q',
          answers: [{ id: 1, text: 'A', count: 2, me: true }],
          multi: false,
          expiresAt: null,
          finalized: false,
        }}
        channelId="c1"
        messageId="100"
        isOwn={false}
        onVote={() => undefined}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: /Voters/ }));
    expect(await screen.findByText('The voters could not be loaded.')).toBeInTheDocument();
    expect(screen.queryByText('Nobody to show.')).toBeNull();
  });

  it('a thread starter that failed to load is not called deleted', async () => {
    seedLounge({ messages: [] });
    server.use(http.get('*/api/v1/channels/c1/messages/55', fail500));
    render(
      <SystemMessage
        message={
          { id: '100', type: 21, content: '', author: { id: '2', displayName: 'Bob' } } as never
        }
        referenceId="55"
        referenceChannelId="c1"
      />,
    );
    expect(
      await screen.findByText('The message this thread started from could not be loaded.'),
    ).toBeInTheDocument();
    expect(screen.queryByText(/was deleted/)).toBeNull();
  });
});

describe('inline edit over the length limit', () => {
  it('says why Enter does nothing instead of ignoring it', async () => {
    const onSave = vi.fn();
    render(
      <InlineEdit
        initial={'x'.repeat(MAX_MESSAGE_LENGTH + 5)}
        onSave={onSave}
        onCancel={() => undefined}
        onEmpty={() => undefined}
      />,
    );
    expect(screen.getByRole('alert')).toHaveTextContent('5 characters over the limit');
    await userEvent.type(screen.getByRole('textbox'), '{Enter}');
    expect(onSave).not.toHaveBeenCalled();
  });
});

describe('reaction names', () => {
  it('the reactions route answers ids only: names come from the users the store knows', () => {
    seedLounge({ messages: [] });
    expect(reactorName({ id: '2' })).toBe('Bob');
    expect(reactorSentence([reactorName({ id: '2' })], 1, 'x')).toBe('Bob reacted with x');
    expect(reactorName({ id: '99' })).toBe('99');
  });
});
