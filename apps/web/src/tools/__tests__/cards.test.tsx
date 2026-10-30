// UTL-CRD-001: the invite card tool and the `card:` buttons of the card message.
import { ToastProvider } from '@sideroom/ui';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ComponentRows } from '../../lounge/msg/Components';
import { INVITE_CARD, isSystemMessage } from '../../lounge/msg/SystemMessage';
import { http, HttpResponse, server } from '../../test/server';
import InviteTool, { channelFromPath, parseCardLink } from '../cards/InviteTool';
import { isCardButton, parseCardButton } from '../cards/press';
import { zonedToUtcMs } from '../cards/time';
import { getTool, registryProblems } from '../registry';

vi.mock('@tanstack/react-router', async (orig) => ({
  ...(await orig<typeof import('@tanstack/react-router')>()),
  useRouterState: (o: { select: (s: unknown) => unknown }) =>
    o.select({ location: { pathname: '/channels/g1/c1' } }),
}));
vi.mock('../../store', async (orig) => ({
  ...(await orig<typeof import('../../store')>()),
  useChannels: () => [
    { id: 'c1', type: 0, name: 'general', guild_id: 'g1' },
    { id: 'c2', type: 0, name: 'movies', guild_id: 'g1' },
  ],
}));

beforeEach(() => {
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    // The component rows look up the server's members, roles and channels for their select menus.
    http.get('*/api/v1/guilds/g1', () => HttpResponse.json({ id: 'g1', name: 'Server' })),
    http.get('*/api/v1/guilds/g1/members', () => HttpResponse.json([])),
    http.get('*/api/v1/guilds/g1/roles', () => HttpResponse.json([])),
    http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json([])),
  );
});

const card = (over: Record<string, unknown> = {}) => ({
  id: '900',
  channel_id: 'c1',
  guild_id: 'g1',
  kind: 'game_night',
  title: 'Catan',
  note: '',
  starts_at: 1,
  duration_min: 120,
  timezone: 'UTC',
  seats: 4,
  taken: 1,
  title_id: null,
  link_path: null,
  created_by: '5',
  status: 'open',
  claimants: [],
  mine: { remind: true, reminder_deliverable: true },
  can_start_party: false,
  can_manage: false,
  ...over,
});

describe('the registry entry', () => {
  it('registers the invite tool without clashing with another id', () => {
    expect(getTool('invite')?.group).toBe('collab');
    expect(registryProblems()).toEqual([]);
  });
});

describe('time zones', () => {
  it('turns a wall time in a zone into the right instant, summer and winter', () => {
    // Berlin is UTC+2 in July, UTC+1 in December.
    expect(zonedToUtcMs('2026-07-04T20:00', 'Europe/Berlin')).toBe(Date.UTC(2026, 6, 4, 18, 0));
    expect(zonedToUtcMs('2026-12-04T20:00', 'Europe/Berlin')).toBe(Date.UTC(2026, 11, 4, 19, 0));
    expect(zonedToUtcMs('2026-07-04T20:00', 'America/New_York')).toBe(Date.UTC(2026, 6, 5, 0, 0));
    expect(zonedToUtcMs('2026-07-04T20:00', 'UTC')).toBe(Date.UTC(2026, 6, 4, 20, 0));
    expect(zonedToUtcMs('2026-07-04T20:00', 'Asia/Kolkata')).toBe(Date.UTC(2026, 6, 4, 14, 30));
  });

  it('resolves the hour that repeats when clocks go back to the earlier one', () => {
    // 2026-11-01 01:30 happens twice in New York; the first is still daylight time (UTC-4).
    expect(zonedToUtcMs('2026-11-01T01:30', 'America/New_York')).toBe(Date.UTC(2026, 10, 1, 5, 30));
  });

  it('refuses an unreadable value or an unknown zone', () => {
    expect(zonedToUtcMs('', 'UTC')).toBeNull();
    expect(zonedToUtcMs('tomorrow', 'UTC')).toBeNull();
    expect(zonedToUtcMs('2026-07-04T20:00', 'Not/AZone')).toBeNull();
  });
});

describe('button ids and links', () => {
  it('reads card ids and nothing else', () => {
    expect(parseCardButton('card:claim:900')).toEqual({ action: 'claim', cardId: '900' });
    expect(parseCardButton('card:ics:1')).toEqual({ action: 'ics', cardId: '1' });
    for (const bad of [
      'tool_room:claim:9',
      'sr:offer-accept:1',
      'card:claim:',
      'card:nope:1',
      'card:claim:1:2',
      undefined,
    ])
      expect(parseCardButton(bad)).toBeNull();
    expect(isCardButton('card:party:3')).toBe(true);
    expect(isCardButton('x')).toBe(false);
  });

  it('draws type 201 as a message, not a one-line notice', () => {
    expect(isSystemMessage({ type: INVITE_CARD })).toBe(false);
  });

  it('keeps only addresses inside this app and finds the title of a Cinema page', () => {
    const origin = 'https://side.example';
    expect(parseCardLink('', origin)).toEqual({ path: '', titleId: null });
    expect(parseCardLink('/cinema/tv', origin)).toEqual({ path: '/cinema/tv', titleId: null });
    expect(parseCardLink('/cinema/t/55', origin)).toEqual({ path: '/cinema/t/55', titleId: '55' });
    expect(parseCardLink('https://side.example/cinema/t/55?x=1', origin)).toEqual({
      path: '/cinema/t/55?x=1',
      titleId: '55',
    });
    for (const bad of ['https://evil.example/x', '//evil.example', 'javascript:alert(1)', '/a b'])
      expect(parseCardLink(bad, origin)).toBeNull();
    expect(channelFromPath('/channels/g1/c1')).toEqual({ guildId: 'g1', channelId: 'c1' });
    expect(channelFromPath('/cinema')).toBeNull();
  });
});

const rows = [
  {
    type: 1,
    components: [
      { type: 2, style: 3, label: 'Claim a seat', custom_id: 'card:claim:900' },
      { type: 2, style: 2, label: 'Give up my seat', custom_id: 'card:unclaim:900' },
      { type: 2, style: 2, label: 'Add to calendar', custom_id: 'card:ics:900' },
    ],
  },
];

const view = () =>
  render(
    <ToastProvider>
      <ComponentRows rows={rows} guildId="g1" context={{ channelId: 'c1', messageId: '900' }} />
    </ToastProvider>,
  );

describe('card buttons on the message', () => {
  it('claim a seat through the card route and say reminders are on', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post('*/api/v1/channels/c1/cards/900/claim', async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json(card());
      }),
    );
    view();
    await userEvent.click(screen.getByRole('button', { name: 'Claim a seat' }));
    expect(await screen.findByText('You have a seat.')).toBeInTheDocument();
    expect(bodies).toEqual([{ remind: true }]);
    expect(screen.getByText(/an hour and ten minutes before/)).toBeInTheDocument();
  });

  it('turns reminders off with a second request that keeps the seat', async () => {
    const bodies: { remind: boolean }[] = [];
    server.use(
      http.post('*/api/v1/channels/c1/cards/900/claim', async ({ request }) => {
        const b = (await request.json()) as { remind: boolean };
        bodies.push(b);
        return HttpResponse.json(card({ mine: { remind: b.remind, reminder_deliverable: true } }));
      }),
    );
    view();
    await userEvent.click(screen.getByRole('button', { name: 'Claim a seat' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Turn off reminders' }));
    expect(await screen.findByText('Reminders are off for this card.')).toBeInTheDocument();
    expect(bodies.map((b) => b.remind)).toEqual([true, false]);
  });

  it('warns when the member DM settings would stop the reminder', async () => {
    server.use(
      http.post('*/api/v1/channels/c1/cards/900/claim', () =>
        HttpResponse.json(card({ mine: { remind: true, reminder_deliverable: false } })),
      ),
    );
    view();
    await userEvent.click(screen.getByRole('button', { name: 'Claim a seat' }));
    expect(await screen.findByText(/message settings stop reminders/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Turn off reminders' })).toBeNull();
  });

  it('shows the server message when the card is full', async () => {
    server.use(
      http.post('*/api/v1/channels/c1/cards/900/claim', () =>
        HttpResponse.json({ code: 900010, message: 'All the seats are taken.' }, { status: 409 }),
      ),
    );
    view();
    await userEvent.click(screen.getByRole('button', { name: 'Claim a seat' }));
    expect(await screen.findByText('All the seats are taken.')).toBeInTheDocument();
  });

  it('gives a seat back', async () => {
    let called = 0;
    server.use(
      http.delete('*/api/v1/channels/c1/cards/900/claim', () => {
        called += 1;
        return HttpResponse.json(card({ mine: null, taken: 0 }));
      }),
    );
    view();
    await userEvent.click(screen.getByRole('button', { name: 'Give up my seat' }));
    expect(await screen.findByText('Your seat is free again.')).toBeInTheDocument();
    expect(called).toBe(1);
  });

  it('downloads the calendar file', async () => {
    const created: Blob[] = [];
    URL.createObjectURL = (b: Blob | MediaSource) => {
      created.push(b as Blob);
      return 'blob:ics';
    };
    URL.revokeObjectURL = () => undefined;
    const downloads: string[] = [];
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      downloads.push(this.download);
    });
    server.use(
      http.get(
        '*/api/v1/channels/c1/cards/900/ics',
        () =>
          new HttpResponse('BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n', {
            headers: { 'content-type': 'text/calendar; charset=utf-8' },
          }),
      ),
    );
    view();
    await userEvent.click(screen.getByRole('button', { name: 'Add to calendar' }));
    await waitFor(() => expect(created).toHaveLength(1));
    expect(created[0]!.size).toBe('BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n'.length);
    expect(created[0]!.type.replace(' ', '')).toBe('text/calendar;charset=utf-8');
    expect(downloads).toEqual(['invite-900.ics']);
  });
});

describe('the invite card tool', () => {
  it('posts a card with the instant, the zone, the seats and the title of a linked page', async () => {
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post('*/api/v1/channels/c1/cards', async ({ request }) => {
        bodies.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json(card({ title: 'Spirited Away' }), { status: 201 });
      }),
    );
    render(
      <ToastProvider>
        <InviteTool host="drawer" />
      </ToastProvider>,
    );
    // The channel the member is in is chosen already.
    expect(screen.getByRole('combobox', { name: 'Post in' })).toHaveValue('c1');
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'What is it?' }), 'cinema');
    await userEvent.type(screen.getByRole('textbox', { name: 'Title' }), 'Spirited Away');
    const when = new Date(Date.now() + 3 * 86_400_000);
    const pad = (n: number) => String(n).padStart(2, '0');
    const local = `${when.getFullYear()}-${pad(when.getMonth() + 1)}-${pad(when.getDate())}T20:00`;
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Time zone' }), 'UTC');
    // A datetime-local input takes its value directly.
    const input = screen.getByLabelText('Starts') as HTMLInputElement;
    fireEvent.change(input, { target: { value: local } });
    await userEvent.clear(screen.getByRole('spinbutton', { name: 'Seats' }));
    await userEvent.type(screen.getByRole('spinbutton', { name: 'Seats' }), '8');
    await userEvent.type(
      screen.getByRole('textbox', { name: 'Link inside the app (optional)' }),
      '/cinema/t/55',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Post the card' }));
    expect(await screen.findByText('Posted "Spirited Away". Seats are open.')).toBeInTheDocument();
    expect(bodies).toHaveLength(1);
    expect(bodies[0]).toMatchObject({
      kind: 'cinema',
      title: 'Spirited Away',
      starts_at: Date.UTC(when.getFullYear(), when.getMonth(), when.getDate(), 20, 0),
      timezone: 'UTC',
      seats: 8,
      duration_min: 120,
      link_path: '/cinema/t/55',
      title_id: '55',
    });
  });

  it('says what is missing before it asks the server', async () => {
    let posts = 0;
    server.use(
      http.post('*/api/v1/channels/c1/cards', () => {
        posts += 1;
        return HttpResponse.json(card(), { status: 201 });
      }),
    );
    render(
      <ToastProvider>
        <InviteTool host="drawer" />
      </ToastProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Post the card' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Give the card a title.');
    await userEvent.type(screen.getByRole('textbox', { name: 'Title' }), 'Catan');
    await userEvent.click(screen.getByRole('button', { name: 'Post the card' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Pick a start date and time.');
    expect(posts).toBe(0);
  });

  it('shows the permission message when the server refuses', async () => {
    server.use(
      http.post('*/api/v1/channels/c1/cards', () =>
        HttpResponse.json({ code: 50013, message: 'Missing permissions' }, { status: 403 }),
      ),
    );
    render(
      <ToastProvider>
        <InviteTool host="drawer" />
      </ToastProvider>,
    );
    await userEvent.type(screen.getByRole('textbox', { name: 'Title' }), 'Catan');
    const input = screen.getByLabelText('Starts') as HTMLInputElement;
    const d = new Date(Date.now() + 2 * 86_400_000);
    const pad = (n: number) => String(n).padStart(2, '0');
    fireEvent.change(input, {
      target: { value: `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T18:00` },
    });
    await userEvent.click(screen.getByRole('button', { name: 'Post the card' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'You need permission to send messages in that channel.',
    );
  });
});
