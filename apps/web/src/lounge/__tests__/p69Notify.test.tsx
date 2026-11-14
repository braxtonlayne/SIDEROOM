// SEC-AUD-227 / 228 / 229 (D-716, D-717, D-712): the desktop notification through the real hook, with a fake
// `Notification`: plain text, nothing from a blocked or ignored account, the text under a reaction.
import { act, renderHook, waitFor } from '@testing-library/react';
import { emitDispatch } from '../../store/events';
import { resetStore, useStore } from '../../store';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { setMarkdownEngine, testFallbackEngine } from '../markdown';
import { useDesktopNotify } from '../shell/desktopNotify';
import { useIgnores } from '../shell/ignores';
import { useNotifSettings } from '../shell/notifSettings';
import { resetReactionTextCache } from '../shell/notifyText';
import { useSocial } from '../shell/social';

const navigate = vi.fn();
vi.mock('@tanstack/react-router', async (orig) => ({
  ...(await orig<typeof import('@tanstack/react-router')>()),
  useNavigate: () => navigate,
}));
vi.mock('../nav', async (orig) => ({
  ...(await orig<typeof import('../nav')>()),
  useLoungeRoute: () => ({ channelId: undefined }),
}));
const apiFetch = vi.fn();
vi.mock('../../api/client', async (orig) => ({
  ...(await orig<typeof import('../../api/client')>()),
  apiFetch: (...a: unknown[]) => apiFetch(...a),
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

const message = (over: Record<string, unknown> = {}) => ({
  id: 'm2',
  channel_id: 'c1',
  guild_id: 'g1',
  author: { id: '2', username: 'bob', global_name: 'Bob' },
  content: 'psst ||the ending|| <@1>',
  timestamp: '2026-01-01T00:00:00Z',
  mentions: [{ id: '1', username: 'me' }],
  ...over,
});
const reaction = (over: Record<string, unknown> = {}) => ({
  user_id: '2',
  channel_id: 'c1',
  message_id: 'old1',
  guild_id: 'g1',
  emoji: { id: null, name: '\u{1f44d}' },
  message_author_id: '1',
  burst: false,
  ...over,
});
// The blip module keeps its first AudioContext, so count oscillators (one per tone), not constructions.
const audio = vi.fn();
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
    audio();
    return { frequency: { value: 0 }, connect: (g: unknown) => g, start() {}, stop() {} };
  }
}

beforeEach(() => {
  setMarkdownEngine(testFallbackEngine);
  made.length = 0;
  apiFetch.mockReset();
  resetReactionTextCache();
  resetStore();
  useSocial.setState({ relationships: [] });
  useIgnores.setState({ ids: [] });
  useStore.getState().applyDispatch('READY', {
    user: { id: '1', username: 'me', global_name: 'Me' },
    guilds: [],
    private_channels: [],
  });
  usePrefsStore.getState().adopt({ ...DEFAULT_PREFS, desktopNotifications: true });
  useNotifSettings.setState({ load: async () => true, byGuild: {} });
  vi.stubGlobal('Notification', FakeNotification);
  vi.stubGlobal('AudioContext', FakeAudioContext);
  audio.mockClear();
  window.focus = vi.fn();
});
afterEach(() => vi.unstubAllGlobals());

const fire = async (event: string, data: unknown) => {
  act(() => emitDispatch(event, data));
  await new Promise((r) => setTimeout(r, 20));
};

describe('a message notification is plain text', () => {
  it('shows neither a spoiler text nor an id', async () => {
    useStore
      .getState()
      .applyDispatch('USER_UPDATE', { id: '1', username: 'me', global_name: 'Me' });
    renderHook(() => useDesktopNotify());
    await fire('MESSAGE_CREATE', message());
    await waitFor(() => expect(made).toHaveLength(1));
    const body = made[0]?.opts.body ?? '';
    expect(body).toBe('psst [spoiler] @Me');
    expect(body).not.toContain('ending');
    expect(body).not.toContain('<@');
  });
  it('has an empty body, not the raw text, when the parser is not loaded', async () => {
    setMarkdownEngine(null);
    renderHook(() => useDesktopNotify());
    await fire('MESSAGE_CREATE', message());
    await waitFor(() => expect(made).toHaveLength(1));
    expect(made[0]?.opts.body).toBe('');
  });
});

describe('nothing from a blocked or ignored account', () => {
  it('a blocked author mention raises no notification and no sound', async () => {
    useSocial.setState({
      relationships: [
        { id: '2', type: 2, since: '2026-01-01', user: { id: '2', username: 'bob' } },
      ],
    } as never);
    renderHook(() => useDesktopNotify());
    await fire('MESSAGE_CREATE', message());
    expect(made).toHaveLength(0);
    expect(audio).not.toHaveBeenCalled();
  });
  it('an ignored author mention raises no notification and no sound', async () => {
    useIgnores.setState({ ids: ['2'] });
    renderHook(() => useDesktopNotify());
    await fire('MESSAGE_CREATE', message());
    expect(made).toHaveLength(0);
    expect(audio).not.toHaveBeenCalled();
  });
  it('the same mention from somebody else still notifies and sounds', async () => {
    useIgnores.setState({ ids: ['7'] });
    renderHook(() => useDesktopNotify());
    await fire('MESSAGE_CREATE', message());
    await waitFor(() => expect(made).toHaveLength(1));
    expect(audio).toHaveBeenCalled();
  });
  it('a reaction by an ignored account raises nothing and reads nothing', async () => {
    useIgnores.setState({ ids: ['2'] });
    renderHook(() => useDesktopNotify());
    await fire('MESSAGE_REACTION_ADD', reaction());
    expect(made).toHaveLength(0);
    expect(apiFetch).not.toHaveBeenCalled();
  });
});

describe('the reaction notification says which message', () => {
  it('reads an unloaded message once and shows its text', async () => {
    apiFetch.mockResolvedValue({ content: 'my older **message**' });
    renderHook(() => useDesktopNotify());
    await fire('MESSAGE_REACTION_ADD', reaction());
    await waitFor(() => expect(made).toHaveLength(1));
    expect(made[0]?.opts.body).toBe('my older message');
    expect(apiFetch).toHaveBeenCalledTimes(1);
    await fire('MESSAGE_REACTION_ADD', reaction({ user_id: '3' }));
    await waitFor(() => expect(made).toHaveLength(2));
    expect(apiFetch).toHaveBeenCalledTimes(1);
  });
  it('makes no request for the own reaction, a switch that is off, or no permission', async () => {
    renderHook(() => useDesktopNotify());
    await fire('MESSAGE_REACTION_ADD', reaction({ user_id: '1' }));
    usePrefsStore
      .getState()
      .adopt({ ...DEFAULT_PREFS, desktopNotifications: true, reactionNotifications: false });
    await fire('MESSAGE_REACTION_ADD', reaction());
    usePrefsStore.getState().adopt({ ...DEFAULT_PREFS, desktopNotifications: false });
    await fire('MESSAGE_REACTION_ADD', reaction());
    usePrefsStore.getState().adopt({ ...DEFAULT_PREFS, desktopNotifications: true });
    FakeNotification.permission = 'denied';
    await fire('MESSAGE_REACTION_ADD', reaction());
    FakeNotification.permission = 'granted';
    expect(made).toHaveLength(0);
    expect(apiFetch).not.toHaveBeenCalled();
  });
});
