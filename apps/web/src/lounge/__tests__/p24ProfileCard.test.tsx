import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { MdNode } from '@sideroom/markdown-react';
import { loadMarkdown } from '@sideroom/wasm';
import { http, HttpResponse, server } from '../../test/server';
import { usePopout } from '../profile/popoutStore';
import { ProfileBio, profileSubset } from '../profile/profileMarkdown';
import { resetProfiles } from '../profile/useProfile';
import { currentMarkdownEngine, setMarkdownEngine, testFallbackEngine } from '../markdown';
import { at, baseHandlers, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const text = (t: string): MdNode => ({ type: 'text', text: t });

describe('About Me markdown subset (RDY-PAR-003)', () => {
  it('keeps inline formatting, links and emoji, and flattens block nodes and mentions', () => {
    const out = profileSubset([
      { type: 'heading', level: 1, children: [text('Hi')] },
      { type: 'bold', children: [{ type: 'heading', level: 2, children: [text('deep')] }] },
      { type: 'quote', children: [text('said')] },
      { type: 'list', ordered: false, start: null, items: [[text('one')], [text('two')]] },
      { type: 'code_block', lang: null, code: 'let x' },
      { type: 'user_mention', id: '7' },
      { type: 'everyone' },
      { type: 'timestamp', unix: 1, style: null },
      { type: 'link', url: 'https://example.com', kind: 'auto', children: [text('example')] },
      { type: 'emoji', name: 'fire', emoji: '\u{1F525}' },
      { type: 'spoiler', children: [text('secret')] },
    ]);
    expect(out.map((n) => n.type)).toEqual([
      'text',
      'line_break',
      'bold',
      'text',
      'line_break',
      'text',
      'line_break',
      'text',
      'line_break',
      'text',
      'line_break',
      'link',
      'emoji',
      'spoiler',
    ]);
  });

  it('drops trailing breaks and renders through the shared bio component', () => {
    const { container } = render(<ProfileBio text="hello **bold**" />);
    expect(container.querySelector('[data-profile-bio]')).not.toBeNull();
    expect(container).toHaveTextContent('hello');
    expect(render(<ProfileBio text="" />).container.firstChild).toBeNull();
  });
});

const wasmPath = resolve(process.cwd(), '../../packages/wasm/dist/markdown/markdown_bg.wasm');
describe.skipIf(!existsSync(wasmPath))('About Me with the real sr-markdown parser', () => {
  const prev = currentMarkdownEngine();
  afterEach(() => setMarkdownEngine(prev ?? testFallbackEngine));

  it('renders bold, links and spoilers, and no heading or list', async () => {
    const md = await loadMarkdown(readFileSync(wasmPath) as unknown as URL);
    setMarkdownEngine({
      parse: (t) => md.parse(t) as unknown as MdNode[],
      isJumbo: (t) => md.isJumbo(t),
      expandShortcodes: (t) => md.expandShortcodes(t),
    });
    const { container } = render(
      <ProfileBio text={'# Title\n**bold** and ||secret|| https://example.com\n- item'} />,
    );
    expect(container.querySelector('strong')).toHaveTextContent('bold');
    expect(container.querySelector('a')?.getAttribute('href')).toBe('https://example.com');
    expect(container.querySelector('h1')).toBeNull();
    expect(container.querySelector('ul')).toBeNull();
    expect(container).toHaveTextContent('Title');
    expect(container).toHaveTextContent('item');
  });
});

describe('the profile card is the same in the popout and the full profile (RDY-PAR-003)', () => {
  beforeEach(() => {
    baseHandlers();
    resetProfiles();
    usePopout.setState({ profile: null, full: null });
    seedLounge({
      messages: [msg('100', { content: 'hello from bob' })],
      roles: [],
      members: {
        'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
        'g1:2': { guild_id: 'g1', user_id: '2', roles: [] },
      },
    });
    server.use(
      http.get('*/api/v1/users/2/profile', () =>
        HttpResponse.json({
          user: { id: '2', username: 'bob', global_name: 'Bob', avatar: null, banner: null },
          user_profile: {
            bio: 'Trains **and** buses',
            pronouns: null,
            banner: null,
            accent_color: null,
            theme_colors: [0x203040, 0xc2b08f],
          },
          connected_accounts: [],
          mutual_friends: [],
          mutual_friends_count: 0,
          mutual_guilds: [],
        }),
      ),
      http.get('*/api/v1/users/@me/notes/2', () => HttpResponse.json({ note: '' })),
    );
  });

  it('shows the same About Me markup and the same theme colours', async () => {
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    await userEvent.click(await within(log).findByRole('button', { name: 'Bob' }));
    const pop = await screen.findByRole('dialog', { name: 'User profile' });
    const popBio = await waitForBio(pop);
    const popTheme = themeOf(pop);
    expect(popTheme).toContain('#203040');
    await userEvent.click(within(pop).getByRole('button', { name: 'View Full Profile' }));
    const modal = await screen.findByRole('dialog', { name: 'Bob' });
    const fullBio = await waitForBio(modal);
    expect(fullBio.innerHTML).toBe(popBio.innerHTML);
    expect(themeOf(modal)).toBe(popTheme);
  });
});

async function waitForBio(root: HTMLElement): Promise<HTMLElement> {
  return (await within(root).findByText(/Trains/)).closest('[data-profile-bio]') as HTMLElement;
}
function themeOf(root: HTMLElement): string {
  const el = root.querySelector<HTMLElement>('.prf-themed[data-themed]');
  return `${el?.style.getPropertyValue('--prf-start')}|${el?.style.getPropertyValue('--prf-end')}`;
}
