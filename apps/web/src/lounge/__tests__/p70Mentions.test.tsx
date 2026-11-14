// SEC-AUD-237 / 240 / 241 (D-721, D-726, D-727): the Lounge's mention gate is registered at boot and follows the
// block and ignore lists and the suppress switches while the app is open; a role mention raises the desktop
// notification with the "mentioned you" title and the mention sound.
import { act, renderHook, waitFor } from '@testing-library/react';
import { emitDispatch } from '../../store/events';
import { resetStore, useStore } from '../../store';
import { reduceDispatch } from '../../store/reducer';
import { mentionCounts } from '../../store/mentionGate';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { setMarkdownEngine, testFallbackEngine } from '../markdown';
import { useDesktopNotify } from '../shell/desktopNotify';
import { useIgnores } from '../shell/ignores';
import { useNotifSettings, type GuildNotif } from '../shell/notifSettings';
import { useShellBoot } from '../shell/useShellBoot';
import { useSocial } from '../shell/social';

vi.mock('@tanstack/react-router', async (orig) => ({
  ...(await orig<typeof import('@tanstack/react-router')>()),
  useNavigate: () => vi.fn(),
}));
vi.mock('../nav', async (orig) => ({
  ...(await orig<typeof import('../nav')>()),
  useLoungeRoute: () => ({ channelId: undefined }),
}));
vi.mock('../../api/client', async (orig) => ({
  ...(await orig<typeof import('../../api/client')>()),
  apiFetch: () => new Promise(() => undefined),
}));

const made: { title: string; opts: NotificationOptions }[] = [];
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
const tones = vi.fn();
class FakeAudioContext {
  currentTime = 0;
  destination = {};
  createGain() {
    return {
      gain: { setValueAtTime() {}, exponentialRampToValueAtTime() {} },
      connect: () => ({}),
    };
  }
  createOscillator() {
    tones();
    return { frequency: { value: 0 }, connect: (g: unknown) => g, start() {}, stop() {} };
  }
}

const scope = (over: Partial<GuildNotif> = {}): GuildNotif => ({
  muted: false,
  endsAt: null,
  level: null,
  suppressEveryone: false,
  suppressRoles: false,
  suppressHighlights: false,
  mobilePush: true,
  channels: {},
  ...over,
});

const message = (over: Record<string, unknown> = {}) => ({
  id: '150',
  channel_id: 'c1',
  guild_id: 'g1',
  author: { id: '2', username: 'bob', global_name: 'Bob' },
  content: 'ping',
  timestamp: '2026-01-01T00:00:00Z',
  mentions: [{ id: '1', username: 'me' }],
  ...over,
});

const mentionCount = () => useStore.getState().readStates.c1?.mention_count ?? 0;
const deliver = (over: Record<string, unknown> = {}) =>
  act(() => useStore.getState().applyDispatch('MESSAGE_CREATE', message(over)));

beforeEach(() => {
  setMarkdownEngine(testFallbackEngine);
  made.length = 0;
  resetStore();
  useSocial.setState({ relationships: [] });
  useIgnores.setState({ ids: [] });
  useStore.getState().applyDispatch('READY', {
    user: { id: '1', username: 'me', global_name: 'Me' },
    guilds: [],
    private_channels: [],
  });
  useStore.setState({
    channels: { c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general' } },
    members: { 'g1:1': { guild_id: 'g1', user_id: '1', roles: ['r1'] } },
  } as never);
  usePrefsStore.getState().adopt({ ...DEFAULT_PREFS, desktopNotifications: true });
  useNotifSettings.setState({ load: async () => true, byGuild: {} });
  vi.stubGlobal('Notification', FakeNotification);
  vi.stubGlobal('AudioContext', FakeAudioContext);
  tones.mockClear();
  window.focus = vi.fn();
});
afterEach(() => vi.unstubAllGlobals());

describe('the shell boot registers the mention gate', () => {
  it('registers it, removes it on clean-up, and a block made while the app is open stops the count', () => {
    const { unmount } = renderHook(() => useShellBoot());
    deliver();
    expect(mentionCount()).toBe(1);
    // Blocking someone while the app is open: their next mention does not count.
    act(() =>
      useSocial.setState({
        relationships: [
          { id: '2', type: 2, since: '2026-01-01', user: { id: '2', username: 'bob' } },
        ],
      } as never),
    );
    deliver({ id: '151' });
    expect(mentionCount()).toBe(1);
    // Ignoring counts the same; somebody else's mention still does.
    act(() => useSocial.setState({ relationships: [] }));
    act(() => useIgnores.setState({ ids: ['2'] }));
    deliver({ id: '152' });
    expect(mentionCount()).toBe(1);
    deliver({ id: '153', author: { id: '3', username: 'cy' } });
    expect(mentionCount()).toBe(2);
    // The channel is still unread for a blocked author's message.
    expect(useStore.getState().channels.c1?.last_message_id).toBe('153');
    unmount();
    // Removed: back to the plain rule.
    expect(mentionCounts(message() as never, { direct: true, everyone: false, role: false })).toBe(
      true,
    );
  });

  it('counts a role mention live, and keeps a suppressed kind off', () => {
    renderHook(() => useShellBoot());
    deliver({ mentions: [], mention_roles: ['r1'] });
    expect(mentionCount()).toBe(1);
    act(() =>
      useNotifSettings.setState({
        byGuild: { g1: scope({ suppressRoles: true, suppressEveryone: true }) },
      }),
    );
    deliver({ id: '151', mentions: [], mention_roles: ['r1'] });
    deliver({ id: '152', mentions: [], mention_everyone: true });
    expect(mentionCount()).toBe(1);
    // A direct mention still counts for a person who suppresses both.
    deliver({ id: '153' });
    expect(mentionCount()).toBe(2);
    // Both kinds on the message, one switch only: the other kind reaches them.
    act(() => useNotifSettings.setState({ byGuild: { g1: scope({ suppressEveryone: true }) } }));
    deliver({ id: '154', mentions: [], mention_everyone: true, mention_roles: ['r1'] });
    expect(mentionCount()).toBe(3);
  });

  it('a count the server sent is replaced by it, not added to', () => {
    renderHook(() => useShellBoot());
    deliver();
    const out = reduceDispatch(useStore.getState(), 'READY', {
      user: { id: '1', username: 'me' },
      guilds: [],
      private_channels: [],
      read_state: { entries: [{ id: 'c1', last_message_id: '150', mention_count: 0 }] },
    });
    expect(out.readStates?.c1?.mention_count).toBe(0);
  });
});

describe('a role mention raises the desktop notification', () => {
  it('shows the "mentioned you" title and plays the mention sound', async () => {
    renderHook(() => useDesktopNotify());
    act(() => emitDispatch('MESSAGE_CREATE', message({ mentions: [], mention_roles: ['r1'] })));
    await waitFor(() => expect(made).toHaveLength(1));
    expect(made[0]?.title).toBe('Bob mentioned you');
    // The mention sound is two tones, a plain message's one.
    expect(tones).toHaveBeenCalledTimes(2);
  });

  it('a role the viewer lacks raises nothing', async () => {
    renderHook(() => useDesktopNotify());
    act(() => emitDispatch('MESSAGE_CREATE', message({ mentions: [], mention_roles: ['r9'] })));
    await new Promise((r) => setTimeout(r, 30));
    expect(made).toHaveLength(0);
  });

  it('a muted category silences it', async () => {
    useStore.setState({
      channels: { c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', parent_id: 'cat' } },
    } as never);
    useNotifSettings.setState({
      byGuild: {
        g1: scope({ channels: { cat: { muted: true, endsAt: null, level: null } } }),
      },
    });
    renderHook(() => useDesktopNotify());
    act(() => emitDispatch('MESSAGE_CREATE', message()));
    await new Promise((r) => setTimeout(r, 30));
    expect(made).toHaveLength(0);
  });
});
