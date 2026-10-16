import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { buildMemberRows } from '../memberGroups';
import { toLoungeRole } from '../liveMap';
import type { Member, Role } from '../data';
import { at, baseHandlers, bob, me, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const HOLO = { primary_color: 11127295, secondary_color: 16759788, tertiary_color: 16761760 };

describe('role gradients on the wire (J-08)', () => {
  it('maps the colors object to up to three stops, a third only with a second', () => {
    const base = { id: 'r', guild_id: 'g1', name: 'R', position: 1, permissions: '0' };
    expect(toLoungeRole({ ...base, color: 255 })).toMatchObject({ color: '#0000ff' });
    expect(toLoungeRole({ ...base, color: 255 }).colorSecondary).toBeUndefined();
    const holo = toLoungeRole({ ...base, color: HOLO.primary_color, colors: HOLO });
    expect([holo.color, holo.colorSecondary, holo.colorTertiary]).toEqual([
      '#a9c9ff',
      '#ffbbec',
      '#ffc3a0',
    ]);
    // A stray tertiary stop without a secondary one is ignored.
    const odd = toLoungeRole({
      ...base,
      color: 1,
      colors: { primary_color: 1, tertiary_color: 5 },
    });
    expect(odd.colorTertiary).toBeUndefined();
  });
});

describe('member list groups carry the gradient stops (J-08)', () => {
  const role = (id: string, over: Partial<Role>): Role => ({
    id,
    name: id,
    hoist: false,
    position: 1,
    ...over,
  });
  const member = (id: string, roleIds: string[]): Member => ({
    user: { id, username: id, displayName: id },
    guildId: 'g1',
    roleIds,
  });
  it('lists every stop of the highest coloured role, one stop for a plain colour', () => {
    const roles = [
      role('plain', { color: '#ff0000', position: 1 }),
      role('holo', {
        color: '#a9c9ff',
        colorSecondary: '#ffbbec',
        colorTertiary: '#ffc3a0',
        position: 3,
      }),
    ];
    const rows = buildMemberRows(
      [member('a', ['plain']), member('b', ['plain', 'holo']), member('c', [])],
      roles,
      { a: { status: 'online' }, b: { status: 'online' }, c: { status: 'online' } },
    );
    const stops = Object.fromEntries(
      rows.flatMap((r) => (r.kind === 'member' ? [[r.entry.member.user.id, r.entry.stops]] : [])),
    );
    expect(stops).toEqual({
      a: ['#ff0000'],
      b: ['#a9c9ff', '#ffbbec', '#ffc3a0'],
      c: [],
    });
  });
});

describe('gradient names in the member list and in messages (J-08)', () => {
  // The shell is a lazy route; load it once up front so a cold run does not eat the first test's budget.
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => baseHandlers());

  it('paints a holographic role with three stops and a plain colour as a fallback', async () => {
    seedLounge({
      messages: [msg('100', { content: 'hello there' })],
      roles: [
        {
          id: 'r1',
          guild_id: 'g1',
          name: 'Holo',
          color: HOLO.primary_color,
          position: 2,
          permissions: '0',
          colors: HOLO,
        },
      ],
      members: {
        'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
        'g1:2': { guild_id: 'g1', user_id: '2', roles: ['r1'] },
      },
    });
    at('/channels/g1/c1');
    const log = await screen.findByRole('log', { name: 'Messages in general' });
    await within(log).findByText('hello there');
    const author = Array.from(log.querySelectorAll<HTMLElement>('.lng-msg-author')).find(
      (e) => e.textContent === bob.global_name,
    )!;
    expect(author).toHaveAttribute('data-gradient');
    // Three stops in the gradient, the first stop as the plain colour where background-clip is missing.
    expect(author.style.backgroundImage).toContain('#ffc3a0');
    expect(author.style.backgroundImage.match(/#[0-9a-f]{6}/g)).toHaveLength(3);
    expect(author.style.color).toBe('rgb(169, 201, 255)');

    // The member list paints the name the same way, and leaves the badges outside the clipped text.
    const name = await screen.findByText(bob.global_name!, { selector: '.lng-member-gradient' });
    expect(name).toHaveAttribute('data-gradient');
    expect(name.style.backgroundImage.match(/#[0-9a-f]{6}/g)).toHaveLength(3);
    expect(name.parentElement).toHaveClass('lng-member-name');
    expect(name.parentElement!.style.color).toBe('rgb(169, 201, 255)');
    void me;
  });

  it('keeps a one-colour role as a plain colour', async () => {
    seedLounge({
      messages: [msg('100', { content: 'plain hello' })],
      roles: [
        {
          id: 'r1',
          guild_id: 'g1',
          name: 'Red',
          color: 0xff0000,
          position: 2,
          permissions: '0',
          colors: { primary_color: 0xff0000 },
        },
      ],
      members: {
        'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
        'g1:2': { guild_id: 'g1', user_id: '2', roles: ['r1'] },
      },
    });
    at('/channels/g1/c1');
    const log = await screen.findByRole('log', { name: 'Messages in general' });
    await within(log).findByText('plain hello');
    const author = Array.from(log.querySelectorAll<HTMLElement>('.lng-msg-author')).find(
      (e) => e.textContent === bob.global_name,
    )!;
    expect(author).not.toHaveAttribute('data-gradient');
    expect(author.style.color).toBe('rgb(255, 0, 0)');
    expect(author.style.backgroundImage).toBe('');
  });
});

describe('server traits on the directory card (J-07)', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);

  it('shows the traits of a listed server as chips', async () => {
    baseHandlers();
    seedLounge({ messages: [] });
    server.use(
      http.get('*/api/v1/directory/categories', () => HttpResponse.json(['gaming'])),
      http.get('*/api/v1/directory', () =>
        HttpResponse.json([
          {
            id: 'd1',
            name: 'Chess Club',
            description: 'Pawns',
            icon: null,
            banner: null,
            category: 'gaming',
            featured: false,
            approximate_member_count: 4,
            approximate_presence_count: 1,
            preferred_locale: 'en-US',
            vanity_url_code: null,
            traits: [{ emoji_name: '♟', label: 'Strategy' }, { label: 'Friendly' }],
            joined: false,
          },
        ]),
      ),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Discover' }));
    const dlg = await screen.findByRole('dialog', { name: 'Server Directory' });
    const chips = await within(dlg).findByRole('list', { name: 'Traits of Chess Club' });
    expect(
      within(chips)
        .getAllByRole('listitem')
        .map((li) => li.textContent),
    ).toEqual(['♟Strategy', 'Friendly']);
  });
});
