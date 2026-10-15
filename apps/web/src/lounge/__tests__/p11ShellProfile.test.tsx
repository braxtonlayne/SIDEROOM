import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../test/server';
import { badgeViews } from '../profile/badgeModel';
import { decorationUrl, effectOf } from '../profile/decoration';
import { usePopout } from '../profile/popoutStore';
import { profileTheme } from '../profile/theme';
import { resetProfiles } from '../profile/useProfile';
import { useSocial } from '../shell/social';
import { at, baseHandlers, bob, calls, msg, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('profile theme (LNG-PRF-010)', () => {
  it('builds a gradient and picks readable text for it', () => {
    const dark = profileTheme([0x112244, 0x223366]);
    expect(dark?.tone).toBe('dark');
    expect(dark?.style).toMatchObject({
      '--prf-bg': 'linear-gradient(180deg, #112244, #223366)',
      '--text-normal': '#fff',
    });
    const light = profileTheme([0xeeeeee, 0xffffcc]);
    expect(light?.tone).toBe('light');
    expect(light?.style).toMatchObject({ '--text-normal': '#111' });
  });

  it('falls back to a flat accent and ignores bad input', () => {
    expect(profileTheme(null, 0x336699)?.style).toMatchObject({
      '--prf-start': '#336699',
      '--prf-end': '#336699',
    });
    expect(profileTheme(null, 0)).toBeNull();
    expect(profileTheme([-1, 0x1000000] as unknown as [number, number])).toBeNull();
    expect(profileTheme(undefined)).toBeNull();
  });
});

describe('badges (LNG-PRF-008)', () => {
  it('orders owner, platform badges and the server tag, with tiers in the label', () => {
    const views = badgeViews(
      [
        { id: 'staff' },
        { id: 'patron-gold', description: 'Gold patron since 2026' },
        { id: 'contributor_2' },
        { id: 'early-member' },
        { id: 'app-developer' },
        { id: 'staff' },
        { id: 'moon-landing', description: 'Was there' },
      ],
      { isServerOwner: true, serverTag: 'GLOOP' },
    );
    expect(views.map((v) => v.label)).toEqual([
      'Server Owner',
      'SIDE ROOM Staff',
      'Patron (Gold)',
      'Contributor (2)',
      'Early Member',
      'App Developer',
      'Was there',
      'Server Tag',
    ]);
    expect(views[2]?.description).toBe('Gold patron since 2026');
    expect(views.at(-1)?.text).toBe('GLOOP');
  });

  it('shows nothing for no badges', () => {
    expect(badgeViews(undefined)).toEqual([]);
  });
});

describe('decorations and effects (LNG-PRF-009)', () => {
  it('only accepts safe asset names and known effects', () => {
    expect(decorationUrl('neon_frame-2')).toBe('/cdn/avatar-decorations/neon_frame-2.png');
    expect(decorationUrl('../../etc/passwd')).toBeNull();
    expect(decorationUrl('a b')).toBeNull();
    expect(decorationUrl(null)).toBeNull();
    expect(effectOf({ id: 'aurora' })).toBe('aurora');
    expect(effectOf({ id: 'javascript:alert(1)' })).toBeNull();
  });
});

const base = {
  user: { id: '2', username: 'bob', global_name: 'Bob', avatar: null, banner: null },
  user_profile: { bio: 'I like **trains**.', pronouns: 'he/him', banner: null, accent_color: null },
  connected_accounts: [
    { id: 'c1', name: 'bobby', type: 'github', verified: true, stats: '42 repositories' },
  ],
  friends_since: '2025-02-03T12:00:00Z',
  mutual_friends: [{ id: '5', username: 'dana', global_name: 'Dana' }],
  mutual_friends_count: 1,
  mutual_guilds: [{ id: 'g1', nick: 'Bobby', member_since: '2025-01-10T12:00:00Z' }],
};

function setup(profile: Record<string, unknown> = base) {
  baseHandlers();
  resetProfiles();
  usePopout.setState({ profile: null, full: null });
  seedLounge({ messages: [msg('100', { content: 'hello from bob' })] });
  useSocial.getState().reset();
  server.use(
    http.get('*/api/v1/users/2/profile', async (ctx) => {
      await record('GET')(ctx);
      return HttpResponse.json(profile);
    }),
    http.get('*/api/v1/users/@me/notes/2', () => HttpResponse.json({ note: '' })),
    http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json([])),
    http.get('*/api/v1/cinema/users/2/lists', () =>
      HttpResponse.json({
        lists: [{ id: 'l1', name: 'Slow burn', description: '', item_count: 3, kind: 'custom' }],
      }),
    ),
  );
}

async function openPopout() {
  const log = await screen.findByRole('log');
  await userEvent.click(await within(log).findByRole('button', { name: 'Bob' }));
  return screen.findByRole('dialog', { name: 'User profile' });
}

describe('profile popout extras (LNG-PRF-003, 008, 009, 010, 011)', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);

  it('shows badges, a status bubble, the theme and the server profile of the viewed server', async () => {
    setup({
      ...base,
      badges: [{ id: 'staff' }, { id: 'patron-gold' }],
      user: { ...base.user, avatar_decoration_data: { asset: 'neon_frame' } },
      user_profile: {
        ...base.user_profile,
        theme_colors: [0x112244, 0x223366],
        profile_effect: { id: 'sparkle' },
      },
      guild_member_profile: {
        guild_id: 'g1',
        nick: 'Bobby',
        bio: 'Trains, but only in Store Guild.',
        pronouns: 'they/them',
        theme_colors: [0x331122, 0x662244],
      },
    });
    const { container } = at('/channels/g1/c1');
    const pop = await openPopout();
    // The request names the server so the backend can add its per-server profile.
    await waitFor(() =>
      expect(
        calls.some((c) => c.key.includes('/users/2/profile') && c.key.includes('guild_id=g1')),
      ).toBe(true),
    );
    expect(await within(pop).findByText('Trains, but only in Store Guild.')).toBeInTheDocument();
    expect(within(pop).getByText(/they\/them/)).toBeInTheDocument();
    expect(within(pop).queryByText(/he\/him/)).toBeNull();
    const badges = within(pop).getByRole('list', { name: 'Badges' });
    expect(within(badges).getByRole('img', { name: 'SIDE ROOM Staff' })).toBeInTheDocument();
    expect(within(badges).getByRole('img', { name: 'Patron (Gold)' })).toBeInTheDocument();
    // The server profile's colours win over the account's.
    const themed = pop.querySelector('[data-themed]') as HTMLElement;
    expect(themed.style.getPropertyValue('--prf-start')).toBe('#331122');
    expect(pop.querySelector('.prf-effect[data-effect="sparkle"]')).not.toBeNull();
    expect(pop.querySelector('img.prf-deco')).toHaveAttribute(
      'src',
      '/cdn/avatar-decorations/neon_frame.png',
    );
    expect(await axeViolations(container)).toEqual([]);
  });

  it('shows the custom status in a bubble', async () => {
    setup();
    const { useStore } = await import('../../store');
    useStore.setState({
      presences: {
        '2': {
          user_id: '2',
          status: 'online',
          activities: [{ type: 4, name: 'Custom Status', state: 'riding the rails' }],
        },
      },
    });
    at('/channels/g1/c1');
    const pop = await openPopout();
    expect(await within(pop).findByText('riding the rails')).toHaveClass('prf-bubble');
  });
});

describe('full profile (LNG-PRF-004..007)', () => {
  beforeEach(() => setup());

  async function openFull() {
    const pop = await openPopout();
    await userEvent.click(within(pop).getByRole('button', { name: 'View Full Profile' }));
    return screen.findByRole('dialog', { name: 'Bob' });
  }

  it('lays out header, left column and tabs', async () => {
    const { useStore } = await import('../../store');
    useStore.setState({
      presences: {
        '2': {
          user_id: '2',
          status: 'online',
          activities: [
            { type: 3, name: 'SIDE ROOM Cinema', details: 'Akira', state: 'S1 E3 · Tetsuo' },
          ],
        },
      },
    });
    at('/channels/g1/c1');
    const modal = await openFull();
    expect(await within(modal).findByText(/bob • he\/him/)).toBeInTheDocument();
    // Left column
    expect(await within(modal).findByText(/42 repositories/)).toBeInTheDocument();
    expect(within(modal).getByText('Friends Since')).toBeInTheDocument();
    expect(within(modal).getByText(/Feb 3, 2025/)).toBeInTheDocument();
    expect(within(modal).getByText(/Joined Store Guild on Jan 10, 2025/)).toBeInTheDocument();
    expect(
      within(modal).getByRole('textbox', { name: 'Note (only visible to you)' }),
    ).toBeInTheDocument();
    // Tabs
    expect(within(modal).getByText('Watching Akira')).toBeInTheDocument();
    await userEvent.click(within(modal).getByRole('tab', { name: 'Watching & Reading' }));
    expect(await within(modal).findByText('Slow burn')).toBeInTheDocument();
    await userEvent.click(within(modal).getByRole('tab', { name: 'Reviews' }));
    expect(await within(modal).findByText('No reviews yet')).toBeInTheDocument();
    expect(await axeViolations(modal)).toEqual([]);
  });

  it('shows the empty activity state with a Message button that opens the DM', async () => {
    server.use(
      http.post('*/api/v1/users/@me/channels', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json({ id: '900', type: 1, recipients: [bob] });
      }),
    );
    at('/channels/g1/c1');
    const modal = await openFull();
    expect(await within(modal).findByText('So mysterious!')).toBeInTheDocument();
    const buttons = within(modal).getAllByRole('button', { name: 'Message' });
    await userEvent.click(buttons[buttons.length - 1]!);
    await waitFor(() => expect(calls.some((c) => c.key === 'POST /users/@me/channels')).toBe(true));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Bob' })).toBeNull());
  });

  it('has the profile menu with every entry and copies the username', async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json([
          { id: '2', type: 1, since: '2025-02-03T12:00:00Z', user: { id: '2', username: 'bob' } },
        ]),
      ),
    );
    at('/channels/g1/c1');
    const modal = await openFull();
    await userEvent.click(within(modal).getByRole('button', { name: 'More actions for Bob' }));
    const menu = await screen.findByRole('menu');
    for (const name of [
      'Copy Username',
      'Invite to Server',
      'Add Note',
      'Remove Friend',
      'Ignore',
      'Block',
      'Copy User ID',
    ])
      expect(within(menu).getByRole('menuitem', { name })).toBeTruthy();
    await userEvent.click(within(menu).getByRole('menuitem', { name: 'Copy Username' }));
    expect(writeText).toHaveBeenCalledWith('bob');
  });

  it('sends a friend request from the header', async () => {
    server.use(
      http.put('*/api/v1/users/@me/relationships/2', async (ctx) => {
        await record('PUT')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    const modal = await openFull();
    await userEvent.click(await within(modal).findByRole('button', { name: 'Add friend' }));
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'PUT /users/@me/relationships/2')).toBe(true),
    );
  });
});
