import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { groupByDay, parseLocalInput } from '../cinema/tv/ChannelPanels';
import { validTimezone } from '../cinema/tv/ChannelEditor';
import { validateRules } from '../cinema/tv/RulesEditor';
import { EMPTY_RULES, isUnavailable } from '../cinema/tv/tvApi';
import { ApiError } from '../../api/client';
import { asStaff, http, HttpResponse, nav, openAdmin, readJson, server } from './helpers';

const putFile = vi.hoisted(() => vi.fn<(url: string) => Promise<void>>(async () => undefined));
vi.mock('../../lounge/msg/upload', () => ({ putFile }));

const CURATOR = ['STAFF_BASIC', 'REVIEW_MEDIA', 'PUBLISH_MEDIA', 'MANAGE_TV'] as const;

const channel = (over: Record<string, unknown> = {}) => ({
  id: '501',
  number: 101,
  call_sign: '[adult swim]',
  name: 'Adult Swim',
  network: 'Adult Swim',
  era_label: 'Fall 2003',
  logo_url: null,
  kind: 'rule_based',
  category: 'anime',
  timezone: 'America/New_York',
  age_restricted: false,
  enabled: true,
  filler_policy: 'slate',
  rules: {
    pool: { genres: ['animation'], kinds: ['anime'], decades: [], tags: [], collection: null },
    block_minutes: 30,
    ordering: 'sequential',
    dayparts: [],
    rerun_spacing_hours: 24,
  },
  published_until_ms: null,
  ...over,
});

interface Calls {
  writes: { method: string; url: string; body: unknown }[];
  schedule: string[];
}

function useTvAdmin(
  o: { channels?: unknown[]; scheduleStatus?: number; override?: unknown | null } = {},
) {
  const calls: Calls = { writes: [], schedule: [] };
  const rec = async (method: string, request: Request) => {
    calls.writes.push({
      method,
      url: new URL(request.url).pathname.replace('/api/v1', ''),
      body: await readJson(request),
    });
  };
  const start = new Date(2026, 2, 10, 20, 0).getTime();
  let override = o.override ?? null;
  server.use(
    http.get('*/api/v1/admin/tv/channels', () =>
      HttpResponse.json({ items: o.channels ?? [channel()] }),
    ),
    http.post('*/api/v1/admin/tv/channels', async ({ request }) => {
      await rec('POST', request);
      return HttpResponse.json(channel({ id: '777', ...((await readJson(request)) as object) }));
    }),
    http.put('*/api/v1/admin/tv/channels/:id', async ({ request, params }) => {
      await rec('PUT', request);
      return HttpResponse.json(
        channel({ id: params.id, ...((await readJson(request)) as object) }),
      );
    }),
    http.delete('*/api/v1/admin/tv/channels/:id', async ({ request }) => {
      await rec('DELETE', request);
      return new HttpResponse(null, { status: 204 });
    }),
    http.get('*/api/v1/admin/tv/channels/:id/schedule', ({ request }) => {
      calls.schedule.push(new URL(request.url).search);
      if (o.scheduleStatus) return new HttpResponse(null, { status: o.scheduleStatus });
      return HttpResponse.json({
        channel_id: '501',
        from_ms: start,
        to_ms: start + 86_400_000,
        published: false,
        warnings: ['Two programmes were skipped: not in the library.'],
        blocks: [
          {
            start_ms: start,
            end_ms: start + 1_800_000,
            title: 'Cowboy Bebop',
            kind: 'anime',
            source: 'rules',
          },
          {
            start_ms: start + 1_800_000,
            end_ms: start + 3_600_000,
            title: 'Samurai Champloo',
            episode_label: 'S1 · E2',
            kind: 'anime',
            source: 'rules',
          },
          {
            start_ms: start + 86_400_000,
            end_ms: start + 88_200_000,
            title: 'Next day show',
            kind: 'series',
          },
        ],
      });
    }),
    http.post('*/api/v1/admin/tv/channels/:id/schedule/publish', async ({ request }) => {
      await rec('POST', request);
      return HttpResponse.json({
        channel_id: '501',
        from_ms: 0,
        to_ms: 1,
        published: true,
        blocks: [],
      });
    }),
    http.get('*/api/v1/admin/tv/channels/:id/override', () =>
      HttpResponse.json({
        override,
        now: { start_ms: start, end_ms: start + 1_800_000, title: 'Cowboy Bebop', kind: 'anime' },
      }),
    ),
    http.post('*/api/v1/admin/tv/channels/:id/override', async ({ request }) => {
      await rec('POST', request);
      override = { title_id: '9', title: 'Alien', started_ms: start };
      return HttpResponse.json({ override, now: null });
    }),
    http.delete('*/api/v1/admin/tv/channels/:id/override', async ({ request }) => {
      await rec('DELETE', request);
      override = null;
      return new HttpResponse(null, { status: 204 });
    }),
    http.post('*/api/v1/admin/tv/channels/:id/skip', async ({ request }) => {
      await rec('POST', request);
      return HttpResponse.json({ override: null, now: null });
    }),
    http.get('*/api/v1/admin/tv/channels/:id/aired', () =>
      HttpResponse.json({
        items: [
          {
            start_ms: start - 3_600_000,
            end_ms: start - 1_800_000,
            title: 'Earlier show',
            kind: 'anime',
            source: 'schedule',
            peak_viewers: 4,
          },
          {
            start_ms: start - 1_800_000,
            end_ms: start,
            title: 'Manual pick',
            kind: 'movie',
            source: 'override',
            peak_viewers: null,
          },
        ],
      }),
    ),
    http.get('*/api/v1/admin/cinema/titles', () =>
      HttpResponse.json({
        items: [{ id: '9', title: 'Alien', year: 1979 }],
        total: 1,
        next_cursor: null,
      }),
    ),
  );
  return calls;
}

describe('Live TV admin: access', () => {
  it('lists Live TV in the Cinema group for a Media Curator', async () => {
    asStaff({ perms: [...CURATOR] });
    useTvAdmin();
    openAdmin('cinema-live-tv');
    await screen.findByRole('heading', { name: 'Live TV' });
    const names = within(nav())
      .getAllByRole('button')
      .map((b) => b.textContent);
    expect(names).toContain('Live TV');
  });

  it('hides it from staff without MANAGE_TV', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    useTvAdmin();
    openAdmin('cinema-live-tv');
    await screen.findByRole('heading', { name: 'Dashboard' });
    expect(
      within(nav())
        .getAllByRole('button')
        .map((b) => b.textContent),
    ).not.toContain('Live TV');
  });

  it('shows a clear permission state when the API answers 403', async () => {
    asStaff({ perms: [...CURATOR] });
    server.use(
      http.get('*/api/v1/admin/tv/channels', () =>
        HttpResponse.json({ message: 'no' }, { status: 403 }),
      ),
    );
    openAdmin('cinema-live-tv');
    expect(await screen.findByText('You do not have access')).toBeInTheDocument();
  });

  it('says "Not available yet" when the server has no Live TV admin routes', async () => {
    asStaff({ perms: [...CURATOR] });
    server.use(
      http.get('*/api/v1/admin/tv/channels', () => new HttpResponse(null, { status: 404 })),
    );
    openAdmin('cinema-live-tv');
    expect(await screen.findByText('Not available yet')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'New channel' })).toBeDisabled();
  });
});

describe('Live TV admin: channels', () => {
  it('lists channels in number order with badges; passes axe', async () => {
    asStaff({ perms: [...CURATOR] });
    useTvAdmin({
      channels: [
        channel({
          id: '502',
          number: 105,
          name: 'Movie Channel',
          kind: 'manual',
          age_restricted: true,
          enabled: false,
        }),
        channel(),
      ],
    });
    const { container } = openAdmin('cinema-live-tv');
    const list = await screen.findByRole('list', { name: 'Channels' });
    const rows = within(list).getAllByRole('listitem');
    expect(rows[0]).toHaveTextContent('Adult Swim');
    expect(rows[1]).toHaveTextContent('Movie Channel');
    expect(rows[1]).toHaveTextContent('disabled');
    expect(rows[1]).toHaveTextContent('18+');
    expect(await axeViolations(container)).toEqual([]);
  });

  it('creates a rule-based channel with rules and dayparts', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin({ channels: [] });
    const user = userEvent.setup();
    openAdmin('cinema-live-tv');
    expect(await screen.findByText('No channels')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'New channel' }));
    await screen.findByRole('form', { name: 'Channel details' });
    const save = screen.getByRole('button', { name: 'Create channel' });
    expect(save).toBeDisabled();
    await user.type(screen.getByLabelText('Channel number'), '110');
    await user.type(screen.getByLabelText('Call sign'), 'RETRO');
    await user.type(screen.getByLabelText('Channel name'), 'Retro Room');
    await user.clear(screen.getByLabelText('Time zone'));
    await user.type(screen.getByLabelText('Time zone'), 'Mars/Olympus');
    expect(save).toBeDisabled();
    expect(screen.getAllByText(/IANA name/).length).toBeGreaterThan(0);
    await user.clear(screen.getByLabelText('Time zone'));
    await user.type(screen.getByLabelText('Time zone'), 'America/Chicago');
    await user.click(screen.getByRole('switch', { name: 'Anime' }));
    await user.type(screen.getByLabelText('Genres'), 'animation, comedy');
    await user.click(screen.getByRole('button', { name: 'Add daypart' }));
    expect(screen.getByLabelText('Daypart 1 start')).toHaveValue('22:00');
    await user.click(screen.getByRole('switch', { name: 'Daypart 1 age-restricted only' }));
    // Restricted dayparts need a restricted channel.
    expect(await screen.findByRole('alert')).toHaveTextContent(
      /mark the channel age-restricted first/,
    );
    expect(save).toBeDisabled();
    await user.click(screen.getByRole('switch', { name: 'Age-restricted channel' }));
    await waitFor(() => expect(save).toBeEnabled());
    await user.click(save);
    await waitFor(() => expect(calls.writes).toHaveLength(1));
    expect(calls.writes[0]).toMatchObject({ method: 'POST', url: '/admin/tv/channels' });
    expect(calls.writes[0]!.body).toMatchObject({
      number: 110,
      call_sign: 'RETRO',
      name: 'Retro Room',
      kind: 'rule_based',
      timezone: 'America/Chicago',
      age_restricted: true,
      filler_policy: 'slate',
      rules: {
        pool: { genres: ['animation', 'comedy'], kinds: ['anime'] },
        block_minutes: 30,
        ordering: 'sequential',
        dayparts: [{ start: '22:00', end: '02:00', restricted_only: true }],
      },
    });
    expect(await screen.findByText('Channel created.')).toBeInTheDocument();
    // Logo upload is offered once the channel exists.
    expect(await screen.findByText('Channel logo')).toBeInTheDocument();
  });

  it('edits an existing channel and keeps its rules', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin();
    const user = userEvent.setup();
    openAdmin('cinema-live-tv');
    await user.click(await screen.findByRole('button', { name: 'Open Adult Swim' }));
    const name = await screen.findByLabelText('Channel name');
    expect(name).toHaveValue('Adult Swim');
    expect(screen.getByLabelText('Genres')).toHaveValue('animation');
    await user.clear(name);
    await user.type(name, 'Adult Swim 2003');
    await user.click(screen.getByRole('button', { name: 'Save channel' }));
    await waitFor(() => expect(calls.writes).toHaveLength(1));
    expect(calls.writes[0]).toMatchObject({ method: 'PUT', url: '/admin/tv/channels/501' });
    expect(
      (calls.writes[0]!.body as { name: string; rules: { pool: { kinds: string[] } } }).name,
    ).toBe('Adult Swim 2003');
    expect(
      (calls.writes[0]!.body as { rules: { pool: { kinds: string[] } } }).rules.pool.kinds,
    ).toEqual(['anime']);
  });

  it('deletes a channel only after its name is typed', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin();
    const user = userEvent.setup();
    openAdmin('cinema-live-tv');
    await user.click(await screen.findByRole('button', { name: 'Open Adult Swim' }));
    await user.click(await screen.findByRole('button', { name: 'Delete channel' }));
    const dialog = await screen.findByRole('dialog', { name: 'Delete Adult Swim' });
    const confirm = within(dialog).getByRole('button', { name: 'Delete channel' });
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByRole('textbox'), 'Adult Swim');
    await user.click(confirm);
    await waitFor(() =>
      expect(calls.writes.at(-1)).toMatchObject({
        method: 'DELETE',
        url: '/admin/tv/channels/501',
      }),
    );
    expect(await screen.findByRole('list', { name: 'Channels' })).toBeInTheDocument();
  });
});

describe('Live TV admin: schedule, now control, report', () => {
  const openTab = async (tab: string) => {
    const user = userEvent.setup();
    const view = openAdmin('cinema-live-tv');
    await user.click(await screen.findByRole('button', { name: 'Open Adult Swim' }));
    await user.click(await screen.findByRole('tab', { name: tab }));
    return { user, ...view };
  };

  it('previews the generated schedule by day, simulates now and publishes after confirming', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin();
    const { user, container } = await openTab('Schedule');
    expect(await screen.findByText('Cowboy Bebop', { exact: false })).toBeInTheDocument();
    expect(screen.getByText('Samurai Champloo', { exact: false })).toBeInTheDocument();
    expect(
      screen.getByText('Two programmes were skipped: not in the library.'),
    ).toBeInTheDocument();
    expect(screen.getByText(/Draft preview/)).toBeInTheDocument();
    // two days of blocks, grouped under their own headings
    expect(screen.getAllByRole('heading', { level: 2 })).toHaveLength(2);
    expect(await axeViolations(container)).toEqual([]);

    // Simulate now passes `at` and marks the programme on air then.
    fireInput(screen.getByLabelText('Simulate now'), '2026-03-10T20:10');
    await waitFor(() => expect(calls.schedule.at(-1)).toContain('at='));
    expect(await screen.findByText('on air at that time')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Publish schedule' }));
    const dialog = await screen.findByRole('dialog', {
      name: /Publish the schedule for Adult Swim/,
    });
    await user.click(within(dialog).getByRole('button', { name: 'Publish' }));
    await waitFor(() =>
      expect(calls.writes.at(-1)).toMatchObject({
        method: 'POST',
        url: '/admin/tv/channels/501/schedule/publish',
      }),
    );
    expect(await screen.findByText('Schedule published.')).toBeInTheDocument();
  });

  it('says the preview is not available yet on a server without it', async () => {
    asStaff({ perms: [...CURATOR] });
    useTvAdmin({ scheduleStatus: 404 });
    await openTab('Schedule');
    expect(await screen.findByText('Not available yet')).toBeInTheDocument();
    expect(screen.getByText(/schedule preview needs/i)).toBeInTheDocument();
  });

  it('skips the current programme with an optional reason', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin();
    const { user } = await openTab('Now control');
    expect(await screen.findByText('Cowboy Bebop')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Skip to the next programme' }));
    const dialog = await screen.findByRole('dialog', { name: /Skip Cowboy Bebop/ });
    await user.type(within(dialog).getByLabelText(/Reason/), 'wrong episode');
    await user.click(within(dialog).getByRole('button', { name: 'Skip' }));
    await waitFor(() =>
      expect(calls.writes.at(-1)).toMatchObject({
        url: '/admin/tv/channels/501/skip',
        body: { reason: 'wrong episode' },
      }),
    );
    expect(await screen.findByText('Skipped.')).toBeInTheDocument();
  });

  it('puts a title on air, then returns to the schedule', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin();
    const { user } = await openTab('Now control');
    await user.type(await screen.findByLabelText('Find a title'), 'alien');
    await user.click(await screen.findByRole('button', { name: 'Choose Alien' }));
    await user.click(screen.getByRole('button', { name: 'Put on air now' }));
    const dialog = await screen.findByRole('dialog', { name: 'Put Alien on air' });
    await user.click(within(dialog).getByRole('button', { name: 'Put on air' }));
    await waitFor(() =>
      expect(calls.writes.at(-1)).toMatchObject({
        url: '/admin/tv/channels/501/override',
        body: { title_id: '9' },
      }),
    );
    expect(await screen.findByText('manual override')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Return to the schedule' }));
    const confirm = await screen.findByRole('dialog', { name: 'Return to the schedule' });
    await user.click(within(confirm).getByRole('button', { name: 'Return to schedule' }));
    await waitFor(() =>
      expect(calls.writes.at(-1)).toMatchObject({
        method: 'DELETE',
        url: '/admin/tv/channels/501/override',
      }),
    );
    await waitFor(() => expect(screen.queryByText('manual override')).not.toBeInTheDocument());
  });

  it('shows the what-aired report newest first; passes axe', async () => {
    asStaff({ perms: [...CURATOR] });
    useTvAdmin();
    const { container } = await openTab('What aired');
    const table = await screen.findByRole('table', { name: 'What aired' });
    const rows = within(table).getAllByRole('row');
    expect(rows[1]).toHaveTextContent('Manual pick');
    expect(rows[1]).toHaveTextContent('override');
    expect(rows[1]).toHaveTextContent('n/a');
    expect(rows[2]).toHaveTextContent('Earlier show');
    expect(rows[2]).toHaveTextContent('4');
    expect(await axeViolations(container)).toEqual([]);
  });
});

function fireInput(el: HTMLElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
  setter.call(el, value);
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

describe('Live TV admin helpers', () => {
  it('validates time zones', () => {
    expect(validTimezone('America/New_York')).toBe(true);
    expect(validTimezone('UTC')).toBe(true);
    expect(validTimezone('Mars/Olympus')).toBe(false);
    expect(validTimezone('  ')).toBe(false);
  });

  it('validates rules the way the server will', () => {
    expect(validateRules(EMPTY_RULES, false)).toBeNull();
    expect(validateRules({ ...EMPTY_RULES, block_minutes: 2 }, false)).toMatch(/Block length/);
    expect(validateRules({ ...EMPTY_RULES, rerun_spacing_hours: -1 }, false)).toMatch(/Rerun/);
    expect(
      validateRules({ ...EMPTY_RULES, pool: { ...EMPTY_RULES.pool, decades: [1995] } }, false),
    ).toMatch(/Decades/);
    expect(
      validateRules(
        { ...EMPTY_RULES, dayparts: [{ start: '22:00', end: '22:00', restricted_only: false }] },
        false,
      ),
    ).toMatch(/same time/);
    expect(
      validateRules(
        { ...EMPTY_RULES, dayparts: [{ start: '', end: '02:00', restricted_only: false }] },
        false,
      ),
    ).toMatch(/start and an end/);
    expect(
      validateRules(
        { ...EMPTY_RULES, dayparts: [{ start: '22:00', end: '02:00', restricted_only: true }] },
        true,
      ),
    ).toBeNull();
  });

  it('groups blocks by local day and parses datetime-local input', () => {
    const d = new Date(2026, 2, 10, 23, 30).getTime();
    const groups = groupByDay([
      { start_ms: d },
      { start_ms: d + 3_600_000 },
      { start_ms: d + 3_600_000 * 2 },
    ]);
    expect(groups.map((g) => g.blocks.length)).toEqual([1, 2]);
    expect(parseLocalInput('')).toBeNull();
    expect(parseLocalInput('2026-03-10T20:10')).toBe(new Date(2026, 2, 10, 20, 10).getTime());
  });

  it('reads a bodiless 404/405/501 as a missing route', () => {
    expect(isUnavailable(new ApiError(404, 'x', null))).toBe(true);
    expect(isUnavailable(new ApiError(501, 'x', null))).toBe(true);
    expect(isUnavailable(new ApiError(404, 'x', { code: 10004 }))).toBe(false);
    expect(isUnavailable(new ApiError(500, 'x', null))).toBe(false);
  });
});
