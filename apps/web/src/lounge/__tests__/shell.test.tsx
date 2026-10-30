import { act, fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { afterAll, beforeAll, vi } from 'vitest';
import { createAppRouter } from '../../router';
import { resetComposerDrafts } from '../Composer';
import { setMarkdownEngine, testFallbackEngine } from '../markdown';
import { useLoungeUi } from '../store';
import { setVoiceController, VoiceController } from '../../voice/controller';
import { resetSession } from '../../voice/session';
import { warmLoungeRoute } from '../../test/warmRoutes';

// These tests drive the Lounge against the deterministic mock seed (opt-in via VITE_SR_MOCK=1).
beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

beforeEach(() => {
  resetComposerDrafts();
  localStorage.clear();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  resetSession();
  // No gateway or media in these tests: the controller sends nowhere and its engine never connects.
  const sent: unknown[] = [];
  setVoiceController(
    new VoiceController({
      createEngine: () => ({
        status: 'idle',
        connect: () => new Promise<void>(() => undefined),
        disconnect: () => undefined,
        setTransmitting: () => undefined,
        setDeafened: () => undefined,
        setUserVolume: () => undefined,
        setUserMuted: () => undefined,
        setMasterVolume: () => undefined,
        setInputVolume: () => undefined,
        setInputDevice: async () => undefined,
        setOutputDevice: async () => undefined,
        restartIce: () => undefined,
      }),
      sendGateway: (_op, d) => (sent.push(d), true),
      subscribe: () => () => undefined,
      currentUserId: () => 'me',
      voiceStatesOf: () => ({}),
      audio: {
        getUserMedia: () => Promise.reject(new Error('no media in tests')),
        createAudioContext: () => null,
        createAudioElement: () => document.createElement('audio'),
        enumerateDevices: async () => [],
        onDeviceChange: () => () => undefined,
        queryMicPermission: async () => 'prompt',
      },
      now: () => Date.now(),
    }),
  );
});
afterEach(() => setVoiceController(null));

// After the env stubs above, so the warmed modules see the same env as the tests.
warmLoungeRoute();

describe('Lounge shell', () => {
  it('renders rail, channel sidebar, header, messages and composer for a channel', async () => {
    at('/channels/1001/2001');
    expect(await screen.findByRole('navigation', { name: 'Servers' })).toBeInTheDocument();
    expect(await screen.findByRole('heading', { level: 1, name: 'general' })).toBeInTheDocument();
    const sidebar = screen.getByTestId('channel-sidebar');
    expect(within(sidebar).getByText('Side Room HQ')).toBeInTheDocument();
    expect(within(sidebar).getByRole('button', { name: /^Text Channels$/ })).toHaveAttribute(
      'aria-expanded',
      'true',
    );
    expect(screen.getByRole('log', { name: 'Messages in general' })).toBeInTheDocument();
    expect(screen.getAllByRole('article').length).toBeGreaterThan(3);
    expect(screen.getByRole('textbox', { name: 'Message #general' })).toBeInTheDocument();
    expect(screen.getAllByRole('navigation', { name: 'Sections' }).length).toBeGreaterThan(0);
  });

  it('virtualizes: only a window of the loaded messages is mounted', async () => {
    at('/channels/1001/2001');
    await screen.findByRole('log');
    const n = screen.getAllByRole('article').length;
    expect(n).toBeGreaterThan(0);
    expect(n).toBeLessThan(50);
  });

  it('shows unread pills, mention badges and a folder aggregate on the rail', async () => {
    at('/channels/1001/2001');
    const rail = await screen.findByRole('navigation', { name: 'Servers' });
    expect(
      within(rail).getByRole('link', { name: /Reading Club, 12 mentions/ }),
    ).toBeInTheDocument();
    expect(within(rail).getByRole('link', { name: 'Watch Club, unread' })).toBeInTheDocument();
    const folder = within(rail).getByRole('button', { name: /Games & study folder, unread/ });
    expect(folder).toHaveAttribute('aria-expanded', 'false');
    await userEvent.click(folder);
    expect(within(rail).getByRole('link', { name: 'Gaming, unread' })).toBeInTheDocument();
    expect(within(rail).getByRole('button', { name: 'Add a Server' })).toBeInTheDocument();
  });

  it('collapses categories and keeps unread/selected channels visible', async () => {
    at('/channels/1001/2001');
    const sidebar = await screen.findByTestId('channel-sidebar');
    await userEvent.click(within(sidebar).getByRole('button', { name: /^Text Channels$/ }));
    expect(within(sidebar).getByRole('button', { name: /^Text Channels$/ })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
    expect(within(sidebar).getByRole('link', { name: /^general/ })).toBeInTheDocument();
    expect(within(sidebar).getByRole('link', { name: /watch-club, unread/ })).toBeInTheDocument();
    expect(within(sidebar).queryByRole('link', { name: /house-notes/ })).not.toBeInTheDocument();
  });

  it('lists voice channels with connected users and joins voice', async () => {
    at('/channels/1001/2001');
    const sidebar = await screen.findByTestId('channel-sidebar');
    expect(within(sidebar).getByText('Mara')).toBeInTheDocument();
    expect(within(sidebar).getByText('3/10')).toBeInTheDocument();
    await userEvent.click(within(sidebar).getByRole('button', { name: /Movie night/ }));
    const panel = screen.getByRole('region', { name: 'Voice connection' });
    expect(panel).toHaveTextContent('Movie night');
    expect(panel).toHaveTextContent('RTC Connecting');
    // Clicking a voice channel also opens its view.
    expect(await screen.findByRole('button', { name: 'Join Voice' }).catch(() => null)).toBeNull();
    expect(screen.getByRole('toolbar', { name: 'Call controls' })).toBeInTheDocument();
  });

  it('sends a message from the composer with Enter and renders markdown', async () => {
    at('/channels/1001/2001');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.type(box, 'hello **world**');
    await userEvent.keyboard('{Enter}');
    expect(box).toHaveValue('');
    const strong = await screen.findByText('world');
    expect(strong.tagName).toBe('STRONG');
  });

  it('expands :shortcodes: through the engine when the message is sent', async () => {
    setMarkdownEngine({
      ...testFallbackEngine,
      expandShortcodes: (t) => t.replace(':smile:', '😄'),
    });
    try {
      at('/channels/1001/2001');
      const box = await screen.findByRole('textbox', { name: 'Message #general' });
      await userEvent.type(box, 'hi :smile:');
      await userEvent.keyboard('{Enter}');
      expect(await screen.findByText(/hi 😄/)).toBeInTheDocument();
      expect(screen.queryByText(/:smile:/)).not.toBeInTheDocument();
    } finally {
      setMarkdownEngine(testFallbackEngine);
    }
  });

  it('Shift+Enter inserts a newline instead of sending', async () => {
    at('/channels/1001/2001');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.type(box, 'a{Shift>}{Enter}{/Shift}b');
    expect(box).toHaveValue('a\nb');
  });

  it('applies markdown shortcuts (Ctrl+B) to the selection', async () => {
    at('/channels/1001/2001');
    const box = (await screen.findByRole('textbox', {
      name: 'Message #general',
    })) as HTMLTextAreaElement;
    await userEvent.type(box, 'bold');
    box.setSelectionRange(0, 4);
    await userEvent.keyboard('{Control>}b{/Control}');
    expect(box).toHaveValue('**bold**');
  });

  it('inserts an emoji from the picker', async () => {
    at('/channels/1001/2001');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.click(screen.getByRole('button', { name: 'Open emoji picker' }));
    await userEvent.click(await screen.findByRole('button', { name: 'fire' }));
    expect(box).toHaveValue('🔥');
  });

  it('shows a locked composer where the viewer cannot send', async () => {
    at('/channels/1001/2004');
    expect(await screen.findByText(/do not have permission to send messages/)).toBeInTheDocument();
    expect(screen.queryByRole('textbox', { name: /Message/ })).not.toBeInTheDocument();
  });

  it('shows the typing indicator', async () => {
    at('/channels/1001/2001');
    expect(await screen.findByText('Doge is typing…')).toBeInTheDocument();
  });

  it('edits the last own message with ArrowUp in an empty composer', async () => {
    at('/channels/1001/2001');
    const box = await screen.findByRole('textbox', { name: 'Message #general' });
    await userEvent.type(box, 'my draft{Enter}');
    await userEvent.click(box);
    await userEvent.keyboard('{ArrowUp}');
    const editor = await screen.findByRole('textbox', { name: 'Edit message' });
    expect(editor).toHaveValue('my draft');
    await userEvent.clear(editor);
    await userEvent.type(editor, 'edited text{Enter}');
    expect(await screen.findByText('edited text')).toBeInTheDocument();
    expect(screen.getByText('edited text').closest('article')).toHaveTextContent('(edited)');
  });

  it('renders member list role groups with counts and presence', async () => {
    at('/channels/1001/2001');
    const members = await screen.findByRole('list', { name: 'Members' });
    expect(within(members).getByText(/Staff — \d+/)).toBeInTheDocument();
    expect(within(members).getByText(/Online — \d+/)).toBeInTheDocument();
    expect(within(members).getByText(/Offline — \d+/)).toBeInTheDocument();
    expect(within(members).getByText('Cowboy Bebop')).toBeInTheDocument();
  });

  it('toggles the member list from the header', async () => {
    at('/channels/1001/2001');
    await screen.findByRole('list', { name: 'Members' });
    await userEvent.click(screen.getByRole('button', { name: 'Member List' }));
    expect(screen.queryByRole('list', { name: 'Members' })).not.toBeInTheDocument();
  });

  it('opens the drawer on its Go to tab with Ctrl+K (UTL-SHL-001); the search itself is tested in tools/__tests__/drawer.test.tsx', async () => {
    at('/channels/1001/2001');
    await screen.findByRole('log');
    fireEvent.keyDown(document, { key: 'k', ctrlKey: true });
    const dialog = await screen.findByRole('dialog', { name: 'Drawer' }, { timeout: 20_000 });
    expect(within(dialog).getByRole('tab', { name: 'Go to', selected: true })).toBeInTheDocument();
    expect(within(dialog).getByRole('combobox')).toBeInTheDocument();
    await userEvent.keyboard('{Escape}');
    await vi.waitFor(() => expect(screen.queryByRole('combobox')).not.toBeInTheDocument());
  }, 60_000);

  it('opens the shortcut sheet with Ctrl+/', async () => {
    at('/channels/1001/2001');
    await screen.findByRole('log');
    fireEvent.keyDown(document, { key: '/', ctrlKey: true });
    const dialog = await screen.findByRole('dialog', { name: 'Keyboard shortcuts' });
    expect(within(dialog).getByText(/Open the drawer/)).toBeInTheDocument();
  });

  it('moves between channels with Alt+ArrowDown', async () => {
    const router = at('/channels/1001/2001');
    await screen.findByRole('log');
    fireEvent.keyDown(document, { key: 'ArrowDown', altKey: true });
    await vi.waitFor(() => expect(router.state.location.pathname).toBe('/channels/1001/2002'));
  });

  it('remembers the last channel per guild and the last location', async () => {
    const router = at('/channels/1001/2003');
    await screen.findByRole('log');
    expect(useLoungeUi.getState().lastChannelByGuild['1001']).toBe('2003');
    await act(async () => {
      await router.navigate({ to: '/channels/$guildId', params: { guildId: '1001' } });
    });
    await vi.waitFor(() => expect(router.state.location.pathname).toBe('/channels/1001/2003'));
    await act(async () => {
      await router.navigate({ to: '/channels' });
    });
    await vi.waitFor(() => expect(router.state.location.pathname).toBe('/channels/1001/2003'));
  });

  it('home shows DMs sorted by recency and opens a DM', async () => {
    const router = at('/channels/@me');
    const home = await screen.findByTestId('home-sidebar');
    expect(
      within(home).getByRole('button', { name: 'Find or start a conversation' }),
    ).toBeInTheDocument();
    expect(within(home).getByRole('link', { name: /Friends/ })).toBeInTheDocument();
    const dm = await within(home).findByRole('link', { name: /Mara, unread/ });
    await userEvent.click(dm);
    await vi.waitFor(() => expect(router.state.location.pathname).toBe('/channels/@me/3001'));
    expect(await screen.findByRole('textbox', { name: 'Message Mara' })).toBeInTheDocument();
  });

  it('shows a not-found state for an unknown channel', async () => {
    at('/channels/1001/nope');
    expect(await screen.findByText("This channel doesn't exist")).toBeInTheDocument();
  });

  it('collapses the section bar from the title strip', async () => {
    at('/channels/1001/2001');
    await screen.findByRole('log');
    await userEvent.click(screen.getByRole('button', { name: 'Hide section bar' }));
    expect(screen.queryByRole('banner', { name: 'Sections' })).not.toBeInTheDocument();
  });
});

describe('Lounge mobile layout', () => {
  beforeEach(() => {
    window.matchMedia = ((q: string) => ({
      matches: q.includes('max-width'),
      media: q,
      addEventListener() {},
      removeEventListener() {},
    })) as unknown as typeof window.matchMedia;
  });
  afterEach(() => {
    // @ts-expect-error restore jsdom default (no matchMedia)
    delete window.matchMedia;
  });

  it('uses panes and a bottom tab bar, and reveals the chat when a channel opens', async () => {
    const router = at('/channels/1001');
    const tabs = await screen.findByRole('navigation', { name: 'Primary' });
    expect(
      within(tabs)
        .getAllByRole('button')
        .map((b) => b.textContent),
    ).toEqual(['Home', 'Notifications', 'You']);
    const root = document.querySelector('.lng-root')!;
    expect(root).toHaveAttribute('data-mobile', 'true');
    expect(root).toHaveAttribute('data-pane', 'list');
    await userEvent.click(
      within(screen.getByTestId('channel-sidebar')).getByRole('link', { name: /^general/ }),
    );
    await vi.waitFor(() => expect(router.state.location.pathname).toBe('/channels/1001/2001'));
    await vi.waitFor(() => expect(root).toHaveAttribute('data-pane', 'chat'));
    await userEvent.click(screen.getByRole('button', { name: 'Back to channels' }));
    expect(root).toHaveAttribute('data-pane', 'list');
  });

  it('swipes between panes', async () => {
    at('/channels/1001/2001');
    const body = (await screen.findByRole('log')).closest('.lng-body')!;
    const root = document.querySelector('.lng-root')!;
    expect(root).toHaveAttribute('data-pane', 'chat');
    fireEvent.touchStart(body, { touches: [{ clientX: 300, clientY: 100 }] });
    fireEvent.touchEnd(body, { changedTouches: [{ clientX: 100, clientY: 100 }] });
    expect(root).toHaveAttribute('data-pane', 'members');
    fireEvent.touchStart(body, { touches: [{ clientX: 100, clientY: 100 }] });
    fireEvent.touchEnd(body, { changedTouches: [{ clientX: 300, clientY: 100 }] });
    expect(root).toHaveAttribute('data-pane', 'chat');
  });
});
