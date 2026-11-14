// SEC-AUD-221 (D-703): reaction notifications through the real hook, with a fake `Notification`.
import { act, renderHook, waitFor } from '@testing-library/react';
import { emitDispatch } from '../../store/events';
import { resetStore, useStore } from '../../store';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { useDesktopNotify } from '../shell/desktopNotify';
import { useNotifSettings } from '../shell/notifSettings';
import { setMarkdownEngine, testFallbackEngine } from '../markdown';

const navigate = vi.fn();
vi.mock('@tanstack/react-router', async (orig) => ({
  ...(await orig<typeof import('@tanstack/react-router')>()),
  useNavigate: () => navigate,
}));
vi.mock('../nav', async (orig) => ({
  ...(await orig<typeof import('../nav')>()),
  useLoungeRoute: () => ({ channelId: undefined }),
}));

const made: { title: string; opts: NotificationOptions; onclick: (() => void) | null }[] = [];
class FakeNotification {
  static permission = 'granted';
  onclick: (() => void) | null = null;
  constructor(
    public title: string,
    public opts: NotificationOptions,
  ) {
    made.push(this);
  }
  close() {}
}

const reaction = (over: Record<string, unknown> = {}) => ({
  user_id: '2',
  channel_id: 'c1',
  message_id: 'm1',
  guild_id: 'g1',
  emoji: { id: null, name: '\u{1f44d}' },
  message_author_id: '1',
  burst: false,
  ...over,
});

beforeEach(() => {
  // D-716: the body is plain text, which needs the parser.
  setMarkdownEngine(testFallbackEngine);
  made.length = 0;
  navigate.mockClear();
  resetStore();
  useStore.getState().applyDispatch('READY', {
    user: { id: '1', username: 'me' },
    guilds: [],
    private_channels: [],
  });
  useStore
    .getState()
    .applyDispatch('USER_UPDATE', { id: '2', username: 'bob', global_name: 'Bob' });
  useStore.getState().applyDispatch('MESSAGE_CREATE', {
    id: 'm1',
    channel_id: 'c1',
    guild_id: 'g1',
    author: { id: '1', username: 'me' },
    content: 'my own message',
    timestamp: '2026-01-01T00:00:00Z',
  });
  usePrefsStore.getState().adopt({ ...DEFAULT_PREFS, desktopNotifications: true });
  useNotifSettings.setState({ load: async () => true, byGuild: {} });
  vi.stubGlobal('Notification', FakeNotification);
  window.focus = vi.fn();
  const audio = vi.fn();
  vi.stubGlobal('AudioContext', audio);
});
afterEach(() => vi.unstubAllGlobals());

const fire = async (data: unknown) => {
  act(() => emitDispatch('MESSAGE_REACTION_ADD', data));
  await new Promise((r) => setTimeout(r, 20));
};

describe('useDesktopNotify and reactions', () => {
  it('raises one notification for a reaction on my message: title, body, tag, no sound', async () => {
    const { unmount } = renderHook(() => useDesktopNotify());
    await fire(reaction());
    await waitFor(() => expect(made).toHaveLength(1));
    expect(made[0]?.title).toBe('Bob reacted \u{1f44d} to your message');
    expect(made[0]?.opts.body).toBe('my own message');
    expect(made[0]?.opts.tag).toBe('react-m1');
    expect(AudioContext).not.toHaveBeenCalled();
    made[0]?.onclick?.();
    expect(navigate).toHaveBeenCalledWith({
      to: '/channels/$guildId/$channelId/$messageId',
      params: { guildId: 'g1', channelId: 'c1', messageId: 'm1' },
    });
    unmount();
  });

  it('shows a custom emoji as :name:', async () => {
    renderHook(() => useDesktopNotify());
    await fire(reaction({ emoji: { id: '77', name: 'party' } }));
    await waitFor(() => expect(made).toHaveLength(1));
    expect(made[0]?.title).toBe('Bob reacted :party: to your message');
  });

  it("stays quiet for someone else's message, my own reaction, an unknown author and the switch off", async () => {
    renderHook(() => useDesktopNotify());
    await fire(reaction({ message_author_id: '3' }));
    await fire(reaction({ user_id: '1' }));
    await fire(reaction({ message_author_id: undefined }));
    usePrefsStore
      .getState()
      .adopt({ ...DEFAULT_PREFS, desktopNotifications: true, reactionNotifications: false });
    await fire(reaction());
    expect(made).toHaveLength(0);
  });
});
