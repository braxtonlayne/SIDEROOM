// SEC-AUD-262..264 (wave 71, D-737, D-739, D-740): a pin is drawn as a message and a search line never falls back to
// typed text; search on a phone; a relative timestamp in a one-line text is a date.
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { formatTimestamp, type MdNode } from '@sideroom/markdown-react';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { resetStore, useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { ChannelHeader } from '../ChannelHeader';
import { PinsButton } from '../header/PinsButton';
import { setMarkdownEngine, testFallbackEngine } from '../markdown';
import { singleLineText } from '../msg/SingleLine';
import { SearchBox } from '../SearchBox';
import { resetHitContext } from '../searchContext';
import { useIgnores } from '../shell/ignores';
import { useSocial } from '../shell/social';
import { notificationText } from '../shell/notifyText';

const navigate = vi.fn();
vi.mock('@tanstack/react-router', async (orig) => ({
  ...(await orig<typeof import('@tanstack/react-router')>()),
  useNavigate: () => navigate,
}));
vi.mock('../nav', async (orig) => ({
  ...(await orig<typeof import('../nav')>()),
  useLoungeRoute: () => ({ guildId: 'g1', channelId: 'c1' }),
  useLoungeGoto: () => vi.fn(),
  useLoungeGotoMessage: () => vi.fn(),
}));

const RAW = 'psst ||the ending|| <@123> **bold**';
const bob = { id: '2', username: 'bob', global_name: 'Bob' };
const carol = { id: '3', username: 'carol', global_name: 'Carol' };

const block = (id: string) =>
  useSocial.setState({
    relationships: [{ id, type: 2, since: '', user: { id, username: 'x' } }],
  } as never);

beforeEach(() => {
  navigate.mockClear();
  resetStore();
  setMarkdownEngine(testFallbackEngine);
  useSocial.getState().reset();
  useIgnores.getState().reset();
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  useStore.setState({
    currentUserId: '1',
    guilds: { g1: { id: 'g1', name: 'G', owner_id: '1' } },
    channels: { c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general' } },
    channelIdsByGuild: { g1: ['c1'] },
    users: {
      '1': { id: '1', username: 'me' },
      '2': bob,
      '3': carol,
      '123': { id: '123', username: 'dana', global_name: 'Dana' },
    },
  } as never);
  server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
});

describe('the pins panel draws a pin as a message (D-739)', () => {
  const wire = (author: unknown, over: Record<string, unknown> = {}) => ({
    id: 'p1',
    channel_id: 'c1',
    author,
    content: RAW,
    timestamp: '2026-01-01T00:00:00Z',
    pinned: true,
    ...over,
  });
  async function openPins(list: unknown[]) {
    server.use(http.get('*/api/v1/channels/c1/pins', () => HttpResponse.json(list)));
    const view = render(<PinsButton guildId="g1" channelId="c1" canUnpin={false} />);
    await userEvent.click(view.getByRole('button', { name: 'Pinned Messages' }));
    return view;
  }

  it('hides the spoiler until it is clicked, names the mention and draws bold without stars', async () => {
    await openPins([wire(bob)]);
    const spoiler = await screen.findByRole('button', { name: 'Spoiler, click to reveal' });
    expect(spoiler).toHaveAttribute('data-revealed', 'false');
    const body = spoiler.closest('.lng-pin-body') as HTMLElement;
    expect(body.textContent).toContain('@Dana');
    expect(body.textContent).not.toContain('<@123>');
    expect(body.textContent).not.toContain('**');
    expect(body.querySelector('strong')?.textContent).toBe('bold');
    await userEvent.click(spoiler);
    expect(body.querySelector('.sr-md-spoiler')).toHaveAttribute('data-revealed', 'true');
    expect(within(body).getByText('the ending')).toBeInTheDocument();
  });

  it('draws a code block as a block', async () => {
    await openPins([wire(bob, { content: 'look\n```js\nlet x = 1;\n```' })]);
    const body = (await screen.findByText(/let x = 1;/)).closest('.lng-pin-body') as HTMLElement;
    expect(body.querySelector('pre')).not.toBeNull();
    expect(body.textContent).not.toContain('```');
  });

  it('keeps the attachment and no-text fall-backs', async () => {
    await openPins([
      wire(bob, { id: 'a', content: '', attachments: [{ id: 'f', filename: 'x.png', size: 1 }] }),
      wire(carol, { id: 'b', content: '' }),
    ]);
    expect(await screen.findByText('Attachment')).toBeInTheDocument();
    expect(screen.getByText('No text')).toBeInTheDocument();
  });

  it('never draws the typed text while the parser is not loaded', async () => {
    setMarkdownEngine(null);
    await openPins([wire(bob)]);
    await screen.findByText('No text');
    expect(document.body.textContent).not.toContain('the ending');
    expect(document.body.textContent).not.toContain('<@123>');
    expect(document.body.textContent).not.toContain('**');
  });

  it('still reads "1 blocked message" with Show message for a blocked author', async () => {
    block('2');
    await openPins([wire(bob)]);
    expect(await screen.findByText('1 blocked message')).toBeInTheDocument();
    expect(document.body.textContent).not.toContain('the ending');
    expect(screen.queryByText('Bob')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Show message' }));
    expect(screen.getByText('Bob')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Spoiler, click to reveal' })).toBeInTheDocument();
  });
});

describe('a search line never falls back to typed text (D-739)', () => {
  const hit = (id: string, content: string, withSnippet: boolean) => ({
    id,
    channel_id: 'c1',
    guild_id: 'g1',
    author: carol,
    content,
    timestamp: '2026-01-01T00:00:00Z',
    hit: true,
    ...(withSnippet ? { sr_snippet: { text: 'plain snippet', highlights: [] } } : {}),
  });
  const around = (id: string, content: string) => ({
    id,
    channel_id: 'c1',
    author: bob,
    content,
    timestamp: '2026-01-01T00:00:00Z',
  });
  async function search(hits: unknown[]) {
    resetHitContext();
    server.use(
      http.get('*/api/v1/channels/c1/messages', () =>
        HttpResponse.json([
          around('1000000000000000012', 'after ||secret|| <@123>'),
          around('1000000000000000011', 'x'),
          around('1000000000000000010', 'before **loud** ||hidden||'),
        ]),
      ),
      http.get('*/api/v1/guilds/g1/messages/search', () =>
        HttpResponse.json({ total_results: hits.length, messages: hits.map((h) => [h]) }),
      ),
    );
    const view = render(<SearchBox guildId="g1" channelId="c1" label="Search #general" />);
    await userEvent.type(view.getByRole('searchbox', { name: 'Search #general' }), 'x{Enter}');
    return screen.findByRole('region', { name: 'Search results' });
  }

  it('shows plain text for a hit without a snippet', async () => {
    const panel = await search([
      hit('1000000000000000011', 'see ||the ending|| <@123> **b**', false),
    ]);
    const row = await within(panel).findByText(/see/);
    expect(row.textContent).toContain('@Dana');
    expect(panel.textContent).not.toContain('the ending');
    expect(panel.textContent).not.toContain('<@123>');
    expect(panel.textContent).not.toContain('**');
  });

  it('shows plain text in the lines either side of a hit', async () => {
    const panel = await search([hit('1000000000000000011', 'x', true)]);
    await waitFor(() => expect(panel.textContent).toContain('before loud'));
    expect(panel.textContent).toContain('after');
    expect(panel.textContent).toContain('@Dana');
    expect(panel.textContent).not.toContain('secret');
    expect(panel.textContent).not.toContain('hidden');
    expect(panel.textContent).not.toContain('**');
    expect(panel.textContent).not.toContain('<@123>');
  });

  it('keeps the server snippet as it is', async () => {
    const panel = await search([hit('1000000000000000011', 'raw ||typed||', true)]);
    expect(await within(panel).findByText('plain snippet')).toBeInTheDocument();
    expect(panel.textContent).not.toContain('typed');
  });

  it('leaves no typed text while the parser is not loaded', async () => {
    setMarkdownEngine(null);
    const panel = await search([hit('1000000000000000011', 'raw ||typed||', false)]);
    await waitFor(() => expect(panel.querySelector('.lng-search-hit')).not.toBeNull());
    expect(panel.textContent).not.toContain('typed');
    expect(panel.textContent).not.toContain('||');
  });
});

describe('search on a phone (D-740)', () => {
  const header = (onBack?: () => void) =>
    render(
      <ChannelHeader
        icon="hash"
        title="general"
        membersOpen={false}
        onToggleMembers={vi.fn()}
        onBack={onBack}
        searchLabel="Search #general"
        guildId="g1"
        channelId="c1"
      />,
    );

  beforeEach(() => {
    resetHitContext();
    server.use(
      http.get('*/api/v1/channels/c1/messages', () => HttpResponse.json([])),
      http.get('*/api/v1/guilds/g1/messages/search', () =>
        HttpResponse.json({
          total_results: 1,
          messages: [
            [
              {
                id: '1000000000000000011',
                channel_id: 'c1',
                guild_id: 'g1',
                author: carol,
                content: 'hello world',
                timestamp: '2026-01-01T00:00:00Z',
                hit: true,
                sr_snippet: { text: 'hello world', highlights: [] },
              },
            ],
          ],
        }),
      ),
    );
  });

  it('has the button only at phone width (where the header is given a back button)', () => {
    header().unmount();
    expect(screen.queryByRole('button', { name: 'Search this conversation' })).toBeNull();
    header(vi.fn());
    expect(screen.getByRole('button', { name: 'Search this conversation' })).toBeInTheDocument();
  });

  it('opens the box focused, finds a hit, closes on the jump and goes to the message', async () => {
    header(vi.fn());
    const button = screen.getByRole('button', { name: 'Search this conversation' });
    await userEvent.click(button);
    const dialog = screen.getByRole('dialog', { name: 'Search' });
    const box = within(dialog).getByRole('searchbox', { name: 'Search #general' });
    expect(box).toHaveFocus();
    // The filters are all reachable.
    expect(within(dialog).getByRole('button', { name: 'has:link' })).toBeInTheDocument();
    await userEvent.type(box, 'hello{Enter}');
    await userEvent.click(await within(dialog).findByText('hello world'));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({
        replace: true,
        params: expect.objectContaining({ channelId: 'c1', messageId: '1000000000000000011' }),
      }),
    );
  });

  it('closes with Escape and returns focus to the button', async () => {
    header(vi.fn());
    const button = screen.getByRole('button', { name: 'Search this conversation' });
    await userEvent.click(button);
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(button).toHaveFocus();
  });

  it('closes with its close button', async () => {
    header(vi.fn());
    await userEvent.click(screen.getByRole('button', { name: 'Search this conversation' }));
    await userEvent.click(screen.getByRole('button', { name: 'Close search' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('closes with the back gesture (popstate)', async () => {
    header(vi.fn());
    await userEvent.click(screen.getByRole('button', { name: 'Search this conversation' }));
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    act(() => {
      window.dispatchEvent(new PopStateEvent('popstate'));
    });
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });
});

describe('a relative timestamp in a one-line text is a date (D-737)', () => {
  const T = 1700000000;
  const line = (style: string | null) =>
    singleLineText(
      [{ type: 'timestamp', unix: T, style } as unknown as MdNode],
      undefined,
      'spoiler',
    );

  it('writes the relative style as the short date and time', () => {
    expect(line('relative')).toBe(formatTimestamp(T, 'short_date_time'));
    expect(line('relative')).toBe(line('short_date_time'));
    expect(line('relative')).not.toMatch(/ago|in \d/);
  });

  it('leaves the other styles as they were', () => {
    expect(line('short_time')).toBe(formatTimestamp(T, 'short_time'));
    expect(line('long_date')).toBe(formatTimestamp(T, 'long_date'));
    expect(line('short_date')).toBe(formatTimestamp(T, 'short_date'));
    expect(line('long_date_time')).toBe(formatTimestamp(T, 'long_date_time'));
    expect(line(null)).toBe(formatTimestamp(T, null));
  });

  it('is what the notification text says too', () => {
    expect(notificationText('at <t:1700000000:R>', undefined, 'spoiler')).toBe(
      `at ${formatTimestamp(T, 'short_date_time')}`,
    );
  });
});
