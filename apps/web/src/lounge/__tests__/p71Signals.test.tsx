// SEC-AUD-258 / 259 / 260 (wave 71, D-733, D-732, D-735): the open app's badge gate on a role the person lacks, the
// desktop notification that follows the person's own default level, and a preview cut that closes the markup it
// opened (the cases the server runs too, `crates/sr-chat/src/preview_cases.json`).
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { act, renderHook, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { MdNode } from '@sideroom/markdown-react';
import { loadMarkdown } from '@sideroom/wasm';
import { http, HttpResponse, server } from '../../test/server';
import { emitDispatch } from '../../store/events';
import { resetStore, useStore, type Message } from '../../store';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { openSettings, resetPrefs, signedIn } from '../../settings/user/__tests__/helpers';
import {
  currentMarkdownEngine,
  parseMarkdown,
  setMarkdownEngine,
  testFallbackEngine,
} from '../markdown';
import { excerpt } from '../msg/excerpt';
import { singleLineText } from '../msg/SingleLine';
import { useDesktopNotify } from '../shell/desktopNotify';
import { loungeMentionGate } from '../shell/mentionRules';
import { startNotifDefaultsLive, useNotifDefaults } from '../shell/notifDefaults';
import { useIgnores } from '../shell/ignores';
import { useNotifSettings, type GuildNotif } from '../shell/notifSettings';
import { useSocial } from '../shell/social';

vi.mock('@tanstack/react-router', async (orig) => ({
  ...(await orig<typeof import('@tanstack/react-router')>()),
  useNavigate: () => vi.fn(),
}));
vi.mock('../nav', async (orig) => ({
  ...(await orig<typeof import('../nav')>()),
  useLoungeRoute: () => ({ channelId: undefined }),
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
    return { frequency: { value: 0 }, connect: (g: unknown) => g, start() {}, stop() {} };
  }
}

const scope = (over: Partial<GuildNotif> = {}): GuildNotif => ({
  level: null,
  muted: false,
  endsAt: null,
  suppressEveryone: false,
  suppressRoles: false,
  suppressHighlights: false,
  mobilePush: true,
  channels: {},
  ...over,
});
const plainMessage = (over: Record<string, unknown> = {}) =>
  ({
    id: 'm2',
    channel_id: 'c1',
    guild_id: 'g1',
    author: { id: '2', username: 'bob', global_name: 'Bob' },
    content: 'hello there',
    timestamp: '2026-01-01T00:00:00Z',
    mentions: [],
    ...over,
  }) as unknown as Message;

describe('the badge gate with a role the person does not hold (D-733)', () => {
  beforeEach(() => {
    resetStore();
    useSocial.setState({ relationships: [] });
    useIgnores.setState({ ids: [] });
  });
  it('a role mention the viewer does not hold is not a kind of this message for them', () => {
    // The store hands the gate `kind.role` from the viewer's own roles: no role held, no `role` kind. With
    // `@everyone` suppressed and roles not, a message with both kinds does not count for a non-holder.
    useNotifSettings.setState({ byGuild: { g1: scope({ suppressEveryone: true }) } });
    const msg = plainMessage();
    const everyoneOnly = { direct: false, everyone: true, role: false };
    expect(loungeMentionGate(msg, everyoneOnly)).toBe(false);
    // A holder: the role still reaches them, so the badge counts.
    expect(loungeMentionGate(msg, { ...everyoneOnly, role: true })).toBe(true);
    // Both suppressed: nothing counts, holder or not.
    useNotifSettings.setState({
      byGuild: { g1: scope({ suppressEveryone: true, suppressRoles: true }) },
    });
    expect(loungeMentionGate(msg, { ...everyoneOnly, role: true })).toBe(false);
    // A direct mention always counts.
    expect(loungeMentionGate(msg, { direct: true, everyone: true, role: true })).toBe(true);
  });
});

describe("the open app reads the person's own default level (D-732)", () => {
  const fire = async (event: string, data: unknown) => {
    act(() => emitDispatch(event, data));
    await new Promise((r) => setTimeout(r, 20));
  };
  const readDefault = (level: number | null) =>
    server.use(
      http.get('*/api/v1/users/@me/notification-defaults', () =>
        HttpResponse.json({ message_notifications: level }),
      ),
    );
  beforeEach(() => {
    made.length = 0;
    resetStore();
    useNotifDefaults.getState().reset();
    useSocial.setState({ relationships: [] });
    useIgnores.setState({ ids: [] });
    useStore.getState().applyDispatch('READY', {
      user: { id: '1', username: 'me', global_name: 'Me' },
      guilds: [],
      private_channels: [],
    });
    // The server suggests "all messages" and the person chose nothing for it.
    useStore.setState(
      (s) =>
        ({
          guilds: { ...s.guilds, g1: { id: 'g1', default_message_notifications: 0 } },
        }) as never,
    );
    usePrefsStore.getState().adopt({ ...DEFAULT_PREFS, desktopNotifications: true });
    useNotifSettings.setState({ load: async () => true, byGuild: {} });
    setMarkdownEngine(testFallbackEngine);
    vi.stubGlobal('Notification', FakeNotification);
    vi.stubGlobal('AudioContext', FakeAudioContext);
    window.focus = vi.fn();
  });
  afterEach(() => vi.unstubAllGlobals());

  it('confirm: a person whose default is "nothing" gets no notification from a server that suggests all', async () => {
    readDefault(2);
    const stop = startNotifDefaultsLive();
    await waitFor(() => expect(useNotifDefaults.getState().level).toBe(2));
    renderHook(() => useDesktopNotify());
    await fire('MESSAGE_CREATE', plainMessage());
    expect(made).toHaveLength(0);
    stop();
  });

  it('not loaded yet: the decision runs without it, as before', async () => {
    renderHook(() => useDesktopNotify());
    expect(useNotifDefaults.getState().level).toBeUndefined();
    await fire('MESSAGE_CREATE', plainMessage());
    await waitFor(() => expect(made).toHaveLength(1));
  });

  it('a failed read leaves it unloaded and is not retried in a loop', async () => {
    let reads = 0;
    server.use(
      http.get('*/api/v1/users/@me/notification-defaults', () => {
        reads++;
        return HttpResponse.json({ message: 'down' }, { status: 500 });
      }),
    );
    const stop = startNotifDefaultsLive();
    await new Promise((r) => setTimeout(r, 50));
    expect(useNotifDefaults.getState().level).toBeUndefined();
    expect(reads).toBe(1);
    stop();
  });

  it('the update event changes the next decision without a reload', async () => {
    readDefault(null);
    const stop = startNotifDefaultsLive();
    await waitFor(() => expect(useNotifDefaults.getState().level).toBeNull());
    renderHook(() => useDesktopNotify());
    await fire('MESSAGE_CREATE', plainMessage());
    await waitFor(() => expect(made).toHaveLength(1));
    await fire('SR_NOTIFICATION_DEFAULTS_UPDATE', { message_notifications: 2 });
    expect(useNotifDefaults.getState().level).toBe(2);
    await fire('MESSAGE_CREATE', plainMessage({ id: 'm3' }));
    expect(made).toHaveLength(1);
    await fire('SR_NOTIFICATION_DEFAULTS_UPDATE', { message_notifications: null });
    await fire('MESSAGE_CREATE', plainMessage({ id: 'm4' }));
    await waitFor(() => expect(made).toHaveLength(2));
    stop();
  });

  it('a read that was in flight does not overwrite a newer event', async () => {
    let release: () => void = () => undefined;
    server.use(
      http.get('*/api/v1/users/@me/notification-defaults', async () => {
        await new Promise<void>((r) => (release = r));
        return HttpResponse.json({ message_notifications: 0 });
      }),
    );
    const stop = startNotifDefaultsLive();
    await fire('SR_NOTIFICATION_DEFAULTS_UPDATE', { message_notifications: 2 });
    await waitFor(() => expect(useNotifDefaults.getState().level).toBe(2));
    release();
    await new Promise((r) => setTimeout(r, 30));
    expect(useNotifDefaults.getState().level).toBe(2);
    stop();
  });
});

describe('Settings > Notifications writes the open app its new default at once (D-732)', () => {
  beforeEach(() => {
    resetPrefs();
    signedIn();
    useNotifDefaults.getState().reset();
  });
  it('a saved level is in the store before any event arrives', async () => {
    server.use(
      http.get('*/api/v1/users/@me/guilds', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/notification-defaults', () =>
        HttpResponse.json({ message_notifications: 1 }),
      ),
      http.patch('*/api/v1/users/@me/notification-defaults', () =>
        HttpResponse.json({ message_notifications: 2 }),
      ),
    );
    const user = userEvent.setup();
    openSettings('notifications');
    await user.click(await screen.findByRole('radio', { name: 'Nothing' }));
    await waitFor(() => expect(useNotifDefaults.getState().level).toBe(2));
  });
});

describe('a preview closes the markup its cut opened (D-735)', () => {
  const line = (text: string) => singleLineText(parseMarkdown(text), undefined, '[spoiler]');
  const wasmPath = resolve(process.cwd(), '../../packages/wasm/dist/markdown/markdown_bg.wasm');
  const prev = currentMarkdownEngine();
  const words = 'word '.repeat(59);

  describe.skipIf(!existsSync(wasmPath))('with the real sr-markdown parser', () => {
    beforeEach(async () => {
      const md = await loadMarkdown(readFileSync(wasmPath) as unknown as URL);
      setMarkdownEngine({
        parse: (t) => md.parse(t) as unknown as MdNode[],
        isJumbo: (t) => md.isJumbo(t),
        expandShortcodes: (t) => md.expandShortcodes(t),
      });
      return () => setMarkdownEngine(prev ?? testFallbackEngine);
    });
    it.each(['**', '*', '__', '_', '~~', '||', '`'])(
      'one long run of %s reads as its first words and ends in its closer',
      (mark) => {
        const content = `${mark}${words}end${mark}`;
        const got = excerpt(content);
        expect(got).not.toBeNull();
        expect(got!.endsWith(`${mark}…`)).toBe(true);
        expect(got).not.toContain(` ${mark}`);
        const shown = line(got!.slice(0, -1));
        if (mark === '||') expect(shown).toBe('[spoiler]');
        else expect(shown.startsWith('word word')).toBe(true);
      },
    );
    it('a spoiler inside bold needs two closers: the cut moves back and no hidden word is plain text', () => {
      const content = `**a ||${'secret '.repeat(40)}|| tail**`;
      const got = excerpt(content);
      if (got !== null) expect(line(got.replace(/…$/, ''))).not.toContain('secret');
    });
  });
});
