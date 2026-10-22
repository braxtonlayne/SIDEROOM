// N-41 (D-338): a MESSAGE_CREATE with `restored: true` is a message a moderator restored from a case. It is
// inserted again, but raises no mention badge, does not move the unread marker and alerts nobody.
import { act, render, screen } from '@testing-library/react';
import { ToastProvider } from '@sideroom/ui';
import { shouldAlert } from '../../pwa/useDesktopAlerts';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { resetStore, useStore, type Message } from '../../store';
import { emitDispatch } from '../../store/events';
import { reduceDispatch } from '../../store/reducer';
import { DmToasts } from '../shell/DmToasts';
import { desktopDecision } from '../shell/desktopNotify';
import { useNotifSettings } from '../shell/notifSettings';

vi.mock('../nav', async (orig) => ({
  ...(await orig<typeof import('../nav')>()),
  useLoungeRoute: () => ({ guildId: '@me', channelId: 'other' }),
}));

const message = (over: Record<string, unknown> = {}) => ({
  id: '150',
  channel_id: 'c1',
  guild_id: 'g1',
  author: { id: 'u2', username: 'bob', global_name: 'Bob' },
  content: 'hey @me',
  timestamp: '2026-01-01T00:00:00Z',
  mentions: [{ id: 'u1', username: 'me' }],
  ...over,
});

beforeEach(() => {
  resetStore();
  useStore.setState({ currentUserId: 'u1' });
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  useNotifSettings.setState({ load: async () => undefined, byGuild: {} });
  vi.spyOn(document, 'hasFocus').mockReturnValue(true);
});
afterEach(() => vi.restoreAllMocks());

describe('restored messages (N-41, D-338)', () => {
  it('the reducer inserts a restored message without a mention bump or an unread move', () => {
    const base = {
      ...useStore.getState(),
      currentUserId: 'u1',
      channels: {
        c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', last_message_id: '100' },
      },
      readStates: { c1: { channel_id: 'c1', last_message_id: '100', mention_count: 0 } },
    } as never;
    const restored = reduceDispatch(base, 'MESSAGE_CREATE', message({ restored: true }));
    expect(restored.messages?.['150']?.content).toBe('hey @me');
    expect(restored.readStates).toBeUndefined();
    expect(restored.channels).toBeUndefined();

    const plain = reduceDispatch(base, 'MESSAGE_CREATE', message());
    expect(plain.readStates?.c1?.mention_count).toBe(1);
    expect(plain.channels?.c1?.last_message_id).toBe('150');
  });

  it('desktop notifications and the title blink skip it', () => {
    const ctx = {
      me: 'u1',
      activeChannelId: undefined,
      windowFocused: false,
      status: 'online',
      scope: undefined,
      now: 0,
    } as const;
    expect(desktopDecision(message() as unknown as Message, ctx)).toBe('mention');
    expect(desktopDecision(message({ restored: true }) as unknown as Message, ctx)).toBeNull();
    expect(shouldAlert(message() as unknown as Message, 'u1')).toBe(true);
    expect(shouldAlert(message({ restored: true }) as unknown as Message, 'u1')).toBe(false);
  });

  it('a restored DM raises no toast, a new one does', async () => {
    render(
      <ToastProvider>
        <DmToasts />
      </ToastProvider>,
    );
    const send = async (m: unknown) =>
      act(async () => {
        emitDispatch('MESSAGE_CREATE', m);
        await Promise.resolve();
        await Promise.resolve();
      });
    await send(message({ guild_id: undefined, channel_id: 'dm1', restored: true }));
    expect(screen.queryByText(/sent you a message/)).toBeNull();
    await send(message({ id: '151', guild_id: undefined, channel_id: 'dm1' }));
    expect(await screen.findByText('Bob sent you a message')).toBeInTheDocument();
  });
});
