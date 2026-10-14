import { screen, within } from '@testing-library/react';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { DEFAULT_PREFS, usePrefsStore, type UserPrefs } from '../../settings/user/prefs';
import { resetStore } from '../../store';
import type * as S from '../../store/types';
import { formatClock, formatFullTime, formatMessageTime } from '../time';
import { at, baseHandlers, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const use = (over: Partial<UserPrefs>) =>
  usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, ...over } });

const MODS = {
  id: 'r1',
  guild_id: 'g1',
  name: 'Mods',
  color: 0xff0000,
  position: 2,
  permissions: '0',
};
const withMods = {
  roles: [MODS],
  members: {
    'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
    'g1:2': { guild_id: 'g1', user_id: '2', roles: ['r1'] },
  },
};

const IMG = {
  id: 'a1',
  filename: 'cat.png',
  size: 2048,
  url: '/cdn/attachments/c1/a1/cat.png',
  content_type: 'image/png',
  width: 100,
  height: 80,
};

describe('preferences that change how messages look', () => {
  beforeEach(() => baseHandlers());
  afterEach(() => {
    resetStore();
    usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  });

  async function openChat() {
    const view = at('/channels/g1/c1');
    const log = await screen.findByRole('log', { name: 'Messages in general' });
    return { log, ...view };
  }

  it('colours the author name with the role colour by default', async () => {
    seedLounge({ messages: [msg('100', { content: 'hello there' })], ...withMods });
    const { log } = await openChat();
    await within(log).findByText('hello there');
    const author = log.querySelector<HTMLElement>('.lng-msg-author')!;
    expect(author.style.color).toBe('rgb(255, 0, 0)');
    expect(log.querySelector('.lng-role-dot')).toBeNull();
  });

  it('shows the role colour as a dot and leaves the name plain', async () => {
    seedLounge({ messages: [msg('100', { content: 'hello there' })], ...withMods });
    use({ roleColors: 'dot' });
    const { log } = await openChat();
    await within(log).findByText('hello there');
    expect(log.querySelector<HTMLElement>('.lng-msg-author')!.style.color).toBe('');
    expect(log.querySelector<HTMLElement>('.lng-role-dot')!.style.background).toBe(
      'rgb(255, 0, 0)',
    );
  });

  it('can turn role colours off', async () => {
    seedLounge({ messages: [msg('100', { content: 'hello there' })], ...withMods });
    use({ roleColors: 'off' });
    const { log } = await openChat();
    await within(log).findByText('hello there');
    expect(log.querySelector<HTMLElement>('.lng-msg-author')!.style.color).toBe('');
    expect(log.querySelector('.lng-role-dot')).toBeNull();
  });

  it('hides emoji reactions when asked to', async () => {
    const reacted = msg('100', {
      content: 'react to me',
      reactions: [{ emoji: { name: '\u{1F44D}' }, count: 5, me: false }],
    });
    seedLounge({ messages: [reacted] });
    const first = await (async () => {
      const { log, unmount } = await openChat();
      const found = within(log).queryAllByRole('button', { name: /5 reactions/ }).length;
      unmount();
      return found;
    })();
    expect(first).toBe(1);
    seedLounge({ messages: [reacted] });
    use({ showReactions: false });
    const { log } = await openChat();
    await within(log).findByText('react to me');
    expect(within(log).queryAllByRole('button', { name: /5 reactions/ })).toHaveLength(0);
  });

  it('shows pictures as file cards when inline attachments are off', async () => {
    const m = msg('100', { content: 'look', attachments: [IMG] });
    seedLounge({ messages: [m] });
    use({ inlineAttachments: false });
    const { log } = await openChat();
    await within(log).findByText('look');
    expect(log.querySelector('.lng-mosaic')).toBeNull();
    expect(within(log).getByText('cat.png')).toBeInTheDocument();
  });

  it('drops link previews and app embeds independently', async () => {
    const embeds: S.RawEmbed[] = [
      { type: 'article', title: 'A web page', url: 'https://example.com/p' },
      { type: 'rich', title: 'From a bot' },
    ];
    seedLounge({ messages: [msg('100', { content: 'see this', embeds })] });
    use({ linkPreviews: false });
    const first = await openChat();
    await within(first.log).findByText('see this');
    expect(within(first.log).queryByText('A web page')).toBeNull();
    expect(within(first.log).getByText('From a bot')).toBeInTheDocument();
    first.unmount();

    seedLounge({ messages: [msg('100', { content: 'see this', embeds })] });
    use({ showEmbeds: false });
    const second = await openChat();
    await within(second.log).findByText('see this');
    expect(within(second.log).getByText('A web page')).toBeInTheDocument();
    expect(within(second.log).queryByText('From a bot')).toBeNull();
  });

  it('hides the send button when the setting is off', async () => {
    seedLounge({ messages: [msg('100', { content: 'hi' })] });
    const first = await openChat();
    expect(await screen.findByRole('button', { name: 'Send message' })).toBeInTheDocument();
    first.unmount();
    seedLounge({ messages: [msg('100', { content: 'hi' })] });
    use({ showSendButton: false });
    await openChat();
    await screen.findByRole('textbox', { name: /Message/ });
    expect(screen.queryByRole('button', { name: 'Send message' })).toBeNull();
  });

  it('puts the message gap, data attributes and per-section theme on the Lounge root', async () => {
    seedLounge({ messages: [msg('100', { content: 'hi' })] });
    use({
      messageGap: 8,
      underlineLinks: true,
      dyslexiaFont: true,
      highContrast: true,
      sectionThemes: { lounge: 'lounge-onyx' },
    });
    const { container } = await openChat();
    const root = container.querySelector<HTMLElement>('.lng-root')!;
    expect(root.style.getPropertyValue('--lng-message-gap')).toBe('8px');
    expect(root).toHaveAttribute('data-underline-links', 'true');
    expect(root).toHaveAttribute('data-dyslexia', 'true');
    expect(root).toHaveAttribute('data-contrast', 'high');
    expect(root).toHaveAttribute('data-theme', 'lounge-onyx');
    expect(await axeViolations(root)).toEqual([]);
  });
});

describe('time format and region', () => {
  const noon = new Date(2026, 0, 31, 15, 5);
  afterEach(() => usePrefsStore.setState({ prefs: DEFAULT_PREFS }));

  it('follows the 12 or 24-hour choice', () => {
    use({ timeFormat: '24h', locale: 'en-US' });
    expect(formatClock(noon)).toBe('15:05');
    use({ timeFormat: '12h', locale: 'en-US' });
    expect(formatClock(noon)).toMatch(/3:05\s?PM/);
    expect(formatFullTime(noon.toISOString())).toMatch(/3:05\s?PM/);
  });

  it('uses the chosen region for dates', () => {
    use({ locale: 'de', timeFormat: '24h' });
    const old = new Date(2020, 11, 24, 9, 0).toISOString();
    expect(formatMessageTime(old, new Date(2026, 0, 31))).toBe('24.12.2020');
    use({ locale: 'en-US' });
    expect(formatMessageTime(old, new Date(2026, 0, 31))).toBe('12/24/2020');
  });
});
