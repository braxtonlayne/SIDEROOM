import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { ApiError } from '../../api/client';
import { groupByDay, parseLocalInput } from '../cinema/tv/ChannelPanels';
import { validTimezone } from '../cinema/tv/ChannelEditor';
import { validateRules } from '../cinema/tv/RulesEditor';
import { EMPTY_RULES, isUnavailable, logoKeyFromUrl, newDaypart } from '../cinema/tv/tvApi';
import { asStaff, http, HttpResponse, nav, openAdmin, readJson, server } from './helpers';

const putFile = vi.hoisted(() => vi.fn<(url: string) => Promise<void>>(async () => undefined));
vi.mock('../../lounge/msg/upload', () => ({ putFile }));

const CURATOR = ['STAFF_BASIC', 'REVIEW_MEDIA', 'PUBLISH_MEDIA', 'MANAGE_TV'] as const;
const BROWSER_TZ = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';

const rules = () => ({
  grid_minutes: 30,
  no_repeat_window: 8,
  dayparts: [
    {
      name: 'Evening',
      start: '18:00',
      end: '02:00',
      days: [],
      mode: 'shuffle',
      fit: 'overrun',
      pool: {
        kinds: ['anime'],
        genres: ['animation'],
        tags: [],
        title_ids: [],
        year_from: null,
        year_to: null,
        collection_id: null,
        min_runtime_min: null,
        max_runtime_min: null,
        restricted: 'exclude',
      },
    },
  ],
});

const channel = (over: Record<string, unknown> = {}) => ({
  id: '501',
  number: 101,
  call_sign: '[adult swim]',
  name: 'Adult Swim',
  era_label: 'Fall 2003',
  logo_url: null,
  kind: 'rule_based',
  timezone: BROWSER_TZ,
  age_restricted: false,
  enabled: true,
  filler_policy: 'slate',
  theme: null,
  seed: 7,
  viewers: 0,
  rules: rules(),
  ...over,
});

const start = new Date(2026, 2, 10, 20, 0).getTime();
const wireBlock = (o: Record<string, unknown> = {}) => ({
  id: '9001',
  channel_id: '501',
  start_ms: start,
  end_ms: start + 1_800_000,
  kind: 'program',
  title: 'Cowboy Bebop',
  media_kind: 'anime',
  season_number: 1,
  episode_number: 4,
  episode_title: 'Gateway Shuffle',
  title_id: '3',
  age_restricted: false,
  playable: true,
  source: 'generated',
  ...o,
});

interface Calls {
  writes: { method: string; url: string; body: unknown }[];
  gets: string[];
}

function useTvAdmin(
  o: {
    channels?: unknown[];
    previewStatus?: number;
    now?: Record<string, unknown>;
  } = {},
) {
  const calls: Calls = { writes: [], gets: [] };
  const rec = async (method: string, request: Request) => {
    calls.writes.push({
      method,
      url: new URL(request.url).pathname.replace('/api/v1', ''),
      body: await readJson(request),
    });
  };
  const nowState = {
    channel_id: '501',
    at_ms: start,
    offset_ms: 600_000,
    current: wireBlock(),
    next: wireBlock({
      id: '9002',
      start_ms: start + 1_800_000,
      end_ms: start + 3_600_000,
      title: 'Samurai Champloo',
      episode_number: 5,
    }),
    viewers: 3,
    ...(o.now ?? {}),
  };
  server.use(
    http.get('*/api/v1/tv/timezones', () =>
      HttpResponse.json({ items: ['America/Chicago', 'America/New_York', 'UTC', BROWSER_TZ] }),
    ),
    http.get('*/api/v1/admin/tv/channels', () =>
      HttpResponse.json({ items: o.channels ?? [channel()] }),
    ),
    http.post('*/api/v1/admin/tv/channels', async ({ request }) => {
      await rec('POST', request);
      return HttpResponse.json(channel({ id: '777', ...((await readJson(request)) as object) }), {
        status: 201,
      });
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
    http.post('*/api/v1/admin/tv/channels/:id/preview', async ({ request }) => {
      await rec('POST', request);
      if (o.previewStatus) return new HttpResponse(null, { status: o.previewStatus });
      return HttpResponse.json({
        warnings: ['Two programmes were skipped: no asset.'],
        blocks: [
          wireBlock({ id: '0' }),
          wireBlock({
            id: '0',
            start_ms: start + 1_800_000,
            end_ms: start + 3_600_000,
            title: 'Samurai Champloo',
            episode_number: 5,
          }),
          wireBlock({
            id: '0',
            start_ms: start + 86_400_000,
            end_ms: start + 88_200_000,
            title: 'Next day show',
            media_kind: 'series',
          }),
        ],
      });
    }),
    http.get('*/api/v1/admin/tv/channels/:id/schedule', ({ request }) => {
      calls.gets.push(new URL(request.url).pathname + new URL(request.url).search);
      return HttpResponse.json({ items: [wireBlock({ source: 'manual' })] });
    }),
    http.post('*/api/v1/admin/tv/channels/:id/publish', async ({ request }) => {
      await rec('POST', request);
      return HttpResponse.json({
        from_ms: start,
        to_ms: start + 7 * 86_400_000,
        written: 12,
        removed: 3,
        warnings: [],
      });
    }),
    http.get('*/api/v1/admin/tv/channels/:id/now', ({ request }) => {
      calls.gets.push(new URL(request.url).pathname + new URL(request.url).search);
      return HttpResponse.json(nowState);
    }),
    http.put('*/api/v1/admin/tv/channels/:id/now', async ({ request }) => {
      await rec('PUT', request);
      return HttpResponse.json(wireBlock({ title: 'Alien', source: 'override' }));
    }),
    http.post('*/api/v1/admin/tv/channels/:id/blocks', async ({ request }) => {
      await rec('POST', request);
      return HttpResponse.json(wireBlock({ source: 'manual' }), { status: 201 });
    }),
    http.delete('*/api/v1/admin/tv/blocks/:id', async ({ request }) => {
      await rec('DELETE', request);
      return new HttpResponse(null, { status: 204 });
    }),
    http.get('*/api/v1/admin/tv/report', ({ request }) => {
      calls.gets.push(new URL(request.url).pathname + new URL(request.url).search);
      return HttpResponse.json({
        from_ms: start - 86_400_000,
        to_ms: start,
        channels: [
          {
            channel_id: '501',
            number: 101,
            name: 'Adult Swim',
            aired_ms: 7_200_000,
            programmes: 4,
            overrides: 1,
          },
        ],
        items: [
          {
            block: wireBlock({
              id: '1',
              title: 'Earlier show',
              start_ms: start - 7_200_000,
              end_ms: start - 5_400_000,
            }),
            aired_ms: 1_800_000,
          },
          {
            block: wireBlock({
              id: '2',
              title: 'Manual pick',
              start_ms: start - 5_400_000,
              end_ms: start - 3_600_000,
              source: 'override',
            }),
            aired_ms: 1_200_000,
          },
        ],
      });
    }),
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

  it('labels a daypart whose end equals its start as all day (G-11)', async () => {
    asStaff({ perms: [...CURATOR] });
    useTvAdmin({ channels: [] });
    const user = userEvent.setup();
    openAdmin('cinema-live-tv');
    await screen.findByText('No channels');
    await user.click(screen.getByRole('button', { name: 'New channel' }));
    await screen.findByRole('form', { name: 'Channel details' });
    fireInput(screen.getByLabelText('Daypart 1 start'), '06:00');
    fireInput(screen.getByLabelText('Daypart 1 end'), '22:00');
    expect(
      screen.getByText(/An end at or before the start wraps past midnight/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/^All day:/)).toBeNull();
    fireInput(screen.getByLabelText('Daypart 1 end'), '06:00');
    expect(screen.getByText(/^All day: the start and end are the same/)).toBeInTheDocument();
  });

  it('creates a rule-based channel with dayparts and a pool', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin({ channels: [] });
    const user = userEvent.setup();
    const { container } = openAdmin('cinema-live-tv');
    expect(await screen.findByText('No channels')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'New channel' }));
    await screen.findByRole('form', { name: 'Channel details' });
    const save = screen.getByRole('button', { name: 'Create channel' });
    expect(save).toBeDisabled();
    await user.type(screen.getByLabelText('Channel number'), '110');
    await user.type(screen.getByLabelText('Call sign'), 'RETRO');
    await user.type(screen.getByLabelText('Channel name'), 'Retro Room');
    await user.click(screen.getByRole('switch', { name: 'Daypart 1 Anime' }));
    await user.type(screen.getByLabelText('Daypart 1 genres'), 'animation, comedy');
    await user.type(screen.getByLabelText('Daypart 1 year from'), '1990');
    await user.type(screen.getByLabelText('Daypart 1 year to'), '1980');
    expect(await screen.findByRole('alert')).toHaveTextContent(/first year is after the last/);
    expect(save).toBeDisabled();
    await user.clear(screen.getByLabelText('Daypart 1 year to'));
    await user.type(screen.getByLabelText('Daypart 1 year to'), '2009');

    // A second daypart with the same name is refused; renaming fixes it.
    await user.click(screen.getByRole('button', { name: 'Add daypart' }));
    await user.clear(screen.getByLabelText('Daypart 2 name'));
    await user.type(screen.getByLabelText('Daypart 2 name'), 'All day');
    expect(await screen.findByRole('alert')).toHaveTextContent(/names must be unique/);
    await user.clear(screen.getByLabelText('Daypart 2 name'));
    await user.type(screen.getByLabelText('Daypart 2 name'), 'Late night');
    fireInput(screen.getByLabelText('Daypart 2 start'), '22:00');
    fireInput(screen.getByLabelText('Daypart 2 end'), '02:00');
    await user.click(screen.getByRole('switch', { name: 'Daypart 2 Sat' }));
    await waitFor(() => expect(save).toBeEnabled());
    expect(await axeViolations(container)).toEqual([]);

    await user.click(save);
    await waitFor(() => expect(calls.writes).toHaveLength(1));
    expect(calls.writes[0]).toMatchObject({ method: 'POST', url: '/admin/tv/channels' });
    expect(calls.writes[0]!.body).toMatchObject({
      number: 110,
      call_sign: 'RETRO',
      name: 'Retro Room',
      kind: 'rule_based',
      timezone: BROWSER_TZ,
      age_restricted: false,
      enabled: true,
      filler_policy: 'slate',
      rules: {
        grid_minutes: 30,
        no_repeat_window: 8,
        dayparts: [
          {
            name: 'All day',
            start: '00:00',
            end: '00:00',
            days: [],
            mode: 'shuffle',
            fit: 'overrun',
            pool: {
              kinds: ['anime'],
              genres: ['animation', 'comedy'],
              year_from: 1990,
              year_to: 2009,
              restricted: 'exclude',
            },
          },
          { name: 'Late night', start: '22:00', end: '02:00', days: [5] },
        ],
      },
    });
    expect(await screen.findByText('Channel created.')).toBeInTheDocument();
    // Logo upload is not on the server yet and the screen says so.
    expect(screen.getByText(/Logo upload is not available yet/)).toBeInTheDocument();
  });

  it('edits an existing channel, keeping its rules and its logo key', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin({
      channels: [channel({ logo_url: '/cdn/tv/logos/as.webp?ex=1&sig=abc' })],
    });
    const user = userEvent.setup();
    openAdmin('cinema-live-tv');
    await user.click(await screen.findByRole('button', { name: 'Open Adult Swim' }));
    const name = await screen.findByLabelText('Channel name');
    expect(name).toHaveValue('Adult Swim');
    expect(screen.getByLabelText('Daypart 1 name')).toHaveValue('Evening');
    expect(screen.getByLabelText('Daypart 1 genres')).toHaveValue('animation');
    expect(screen.getByRole('img', { name: 'Channel logo' })).toBeInTheDocument();
    await user.clear(name);
    await user.type(name, 'Adult Swim 2003');
    await user.click(screen.getByRole('button', { name: 'Save channel' }));
    await waitFor(() => expect(calls.writes).toHaveLength(1));
    expect(calls.writes[0]).toMatchObject({ method: 'PUT', url: '/admin/tv/channels/501' });
    const body = calls.writes[0]!.body as {
      name: string;
      logo_key: string;
      rules: { dayparts: { name: string }[] };
    };
    expect(body.name).toBe('Adult Swim 2003');
    expect(body.logo_key).toBe('tv/logos/as.webp');
    expect(body.rules.dayparts[0]!.name).toBe('Evening');
  });

  it('refuses to save a channel without a daypart', async () => {
    asStaff({ perms: [...CURATOR] });
    useTvAdmin();
    const user = userEvent.setup();
    openAdmin('cinema-live-tv');
    await user.click(await screen.findByRole('button', { name: 'Open Adult Swim' }));
    await user.click(await screen.findByRole('button', { name: 'Remove daypart 1' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/at least one daypart/);
    expect(screen.getByRole('button', { name: 'Save channel' })).toBeDisabled();
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

function fireInput(el: HTMLElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!;
  setter.call(el, value);
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

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
    expect(await screen.findByText(/Cowboy Bebop · S1 · E4 · Gateway Shuffle/)).toBeInTheDocument();
    expect(screen.getByText(/Samurai Champloo/)).toBeInTheDocument();
    expect(screen.getByText('Two programmes were skipped: no asset.')).toBeInTheDocument();
    expect(screen.getByText(/Draft preview generated from the current rules/)).toBeInTheDocument();
    const preview = calls.writes.find((w) => w.url.endsWith('/preview'))!;
    expect(preview.body).toMatchObject({ from_ms: expect.any(Number), to_ms: expect.any(Number) });
    // two days of blocks, grouped under their own headings
    expect(screen.getAllByRole('heading', { level: 2 })).toHaveLength(2);
    expect(await axeViolations(container)).toEqual([]);

    // Simulate now asks the server what the stored schedule has at that moment.
    fireInput(screen.getByLabelText('Simulate now'), '2026-03-10T20:10');
    const sim = await screen.findByRole('region', { name: 'Simulated now' });
    expect(within(sim).getByText(/Cowboy Bebop/)).toBeInTheDocument();
    expect(calls.gets.some((g) => g.includes('/now?at_ms='))).toBe(true);

    await user.click(screen.getByRole('button', { name: 'Publish schedule' }));
    const dialog = await screen.findByRole('dialog', {
      name: /Publish the schedule for Adult Swim/,
    });
    await user.click(within(dialog).getByRole('button', { name: 'Publish' }));
    await waitFor(() =>
      expect(calls.writes.find((w) => w.url.endsWith('/publish'))).toMatchObject({
        method: 'POST',
        url: '/admin/tv/channels/501/publish',
        body: { horizon_hours: 168 },
      }),
    );
    expect(await screen.findByText('Schedule published.')).toBeInTheDocument();
    expect(screen.getByText(/Wrote 12 blocks and removed 3/)).toBeInTheDocument();
  });

  it('says the preview is not available yet on a server without it', async () => {
    asStaff({ perms: [...CURATOR] });
    useTvAdmin({ previewStatus: 404 });
    await openTab('Schedule');
    expect(await screen.findByText('Not available yet')).toBeInTheDocument();
    expect(screen.getByText(/schedule preview needs/i)).toBeInTheDocument();
  });

  it('shows what is on air and removes an override block', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin({ now: { current: wireBlock({ id: '9100', source: 'override' }) } });
    const { user } = await openTab('Now control');
    const dl = await screen.findByLabelText('On air now');
    expect(dl).toHaveTextContent('Cowboy Bebop');
    expect(dl).toHaveTextContent('override');
    expect(dl).toHaveTextContent('Samurai Champloo');
    await user.click(screen.getByRole('button', { name: 'Remove this override block' }));
    const confirm = await screen.findByRole('dialog', { name: 'Remove this block' });
    await user.click(within(confirm).getByRole('button', { name: 'Remove block' }));
    await waitFor(() =>
      expect(calls.writes.at(-1)).toMatchObject({ method: 'DELETE', url: '/admin/tv/blocks/9100' }),
    );
    expect(await screen.findByText('Block removed.')).toBeInTheDocument();
  });

  it('puts a title on air with an episode and an offset', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin();
    const { user } = await openTab('Now control');
    // A generated block cannot be removed here, only replaced.
    await screen.findByLabelText('On air now');
    expect(screen.queryByRole('button', { name: /Remove this/ })).not.toBeInTheDocument();
    const section = screen.getByRole('region', { name: 'Put a title on air' });
    await user.type(within(section).getByLabelText('Find a title'), 'alien');
    await user.click(await within(section).findByRole('button', { name: 'Choose Alien' }));
    await user.type(within(section).getByLabelText('Episode id (series and anime)'), '77');
    await user.type(within(section).getByLabelText('Start this many minutes in'), '17');
    await user.click(within(section).getByRole('button', { name: 'Put on air now' }));
    const dialog = await screen.findByRole('dialog', { name: 'Put Alien on air' });
    await user.click(within(dialog).getByRole('button', { name: 'Put on air' }));
    await waitFor(() =>
      expect(calls.writes.at(-1)).toMatchObject({
        method: 'PUT',
        url: '/admin/tv/channels/501/now',
        body: { title_id: '9', episode_id: '77', offset_ms: 1_020_000 },
      }),
    );
    expect(await screen.findByText('On air.')).toBeInTheDocument();
  });

  it('places a programme at a time', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin();
    const { user } = await openTab('Now control');
    const section = await screen.findByRole('region', { name: 'Place a programme' });
    const place = within(section).getByRole('button', { name: 'Place programme' });
    expect(place).toBeDisabled();
    await user.type(within(section).getByLabelText('Find a title to place'), 'alien');
    await user.click(await within(section).findByRole('button', { name: 'Choose Alien' }));
    fireInput(within(section).getByLabelText('Starts at'), '2026-03-12T21:00');
    await waitFor(() => expect(place).toBeEnabled());
    await user.click(place);
    await waitFor(() =>
      expect(calls.writes.at(-1)).toMatchObject({
        method: 'POST',
        url: '/admin/tv/channels/501/blocks',
        body: { title_id: '9', start_ms: new Date(2026, 2, 12, 21, 0).getTime() },
      }),
    );
    expect(await screen.findByText('Programme placed.')).toBeInTheDocument();
  });

  it('shows the what-aired report newest first with its summary; passes axe', async () => {
    asStaff({ perms: [...CURATOR] });
    const calls = useTvAdmin();
    const { container } = await openTab('What aired');
    const table = await screen.findByRole('table', { name: 'What aired' });
    const rows = within(table).getAllByRole('row');
    expect(rows[1]).toHaveTextContent('Manual pick');
    expect(rows[1]).toHaveTextContent('override');
    expect(rows[2]).toHaveTextContent('Earlier show');
    expect(screen.getByText(/4 programmes, 1 overrides, 2h on air/)).toBeInTheDocument();
    expect(screen.getByText(/Viewer figures are not kept/)).toBeInTheDocument();
    expect(calls.gets.some((g) => g.includes('channel=501'))).toBe(true);
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe('Live TV admin helpers', () => {
  it('validates time zones against the server list, with Intl as the fallback', () => {
    expect(validTimezone('America/New_York', ['America/New_York', 'UTC'])).toBe(true);
    expect(validTimezone('Europe/Paris', ['America/New_York', 'UTC'])).toBe(false);
    expect(validTimezone('UTC')).toBe(true);
    expect(validTimezone('Mars/Olympus')).toBe(false);
    expect(validTimezone('  ', null)).toBe(false);
  });

  it('validates rules the way the server will', () => {
    expect(validateRules(EMPTY_RULES)).toBeNull();
    expect(validateRules({ ...EMPTY_RULES, grid_minutes: -1 })).toMatch(/grid/);
    expect(validateRules({ ...EMPTY_RULES, no_repeat_window: 65 })).toMatch(/no-repeat/);
    expect(validateRules({ ...EMPTY_RULES, dayparts: [] })).toMatch(/at least one daypart/);
    expect(
      validateRules({ ...EMPTY_RULES, dayparts: [{ ...newDaypart('A'), start: '' }] }),
    ).toMatch(/start and an end/);
    expect(validateRules({ ...EMPTY_RULES, dayparts: [newDaypart('A'), newDaypart('A')] })).toMatch(
      /unique/,
    );
    const bad = newDaypart('A');
    bad.pool = { ...bad.pool, min_runtime_min: 90, max_runtime_min: 30 };
    expect(validateRules({ ...EMPTY_RULES, dayparts: [bad] })).toMatch(/minimum runtime/);
    // an end at or before the start is a daypart that wraps past midnight, not an error
    expect(
      validateRules({
        ...EMPTY_RULES,
        dayparts: [{ ...newDaypart('Late'), start: '22:00', end: '02:00' }],
      }),
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

  it('recovers the logo key from a signed URL', () => {
    expect(logoKeyFromUrl('/cdn/tv/logos/a.webp?ex=1&sig=x')).toBe('tv/logos/a.webp');
    expect(logoKeyFromUrl('/cdn/cinema/10/poster.webp')).toBeNull();
    expect(logoKeyFromUrl(null)).toBeNull();
  });

  it('reads a bodiless 404/405/501 as a missing route', () => {
    expect(isUnavailable(new ApiError(404, 'x', null))).toBe(true);
    expect(isUnavailable(new ApiError(501, 'x', null))).toBe(true);
    expect(isUnavailable(new ApiError(404, 'x', { code: 10004 }))).toBe(false);
    expect(isUnavailable(new ApiError(500, 'x', null))).toBe(false);
  });
});

describe('Live TV admin: off-air slate', () => {
  const slateIs = (body: Record<string, unknown>) =>
    server.use(http.get('*/api/v1/admin/tv/slate', () => HttpResponse.json(body)));

  it('shows the configured slate with its asset id and length', async () => {
    asStaff({ perms: [...CURATOR] });
    useTvAdmin();
    slateIs({ configured: true, asset_id: '777', playable: true, duration_ms: 12_000 });
    openAdmin('cinema-live-tv');
    const row = await screen.findByTestId('tv-slate');
    expect(row).toHaveTextContent('Off-air slate');
    expect(row).toHaveTextContent('Asset 777 · one pass of the loop lasts 12 s');
  });

  it('says how to make one when none is configured', async () => {
    asStaff({ perms: [...CURATOR] });
    useTvAdmin();
    slateIs({ configured: false, playable: false });
    openAdmin('cinema-live-tv');
    const row = await screen.findByTestId('tv-slate');
    expect(row).toHaveTextContent('None.');
    expect(within(row).getByText('sr-cli tv slate')).toBeInTheDocument();
  });

  it('says when the slate is set but does not play yet, and stays quiet when the route is missing', async () => {
    asStaff({ perms: [...CURATOR] });
    useTvAdmin();
    slateIs({ configured: true, asset_id: '778', playable: false });
    openAdmin('cinema-live-tv');
    expect(await screen.findByTestId('tv-slate')).toHaveTextContent(
      'Asset 778 is set but has no renditions to play yet',
    );
  });

  it('leaves the row out when the server cannot tell', async () => {
    asStaff({ perms: [...CURATOR] });
    useTvAdmin();
    server.use(http.get('*/api/v1/admin/tv/slate', () => new HttpResponse(null, { status: 500 })));
    openAdmin('cinema-live-tv');
    await screen.findByRole('heading', { name: 'Live TV' });
    await screen.findByRole('list', { name: 'Channels' });
    expect(screen.queryByTestId('tv-slate')).not.toBeInTheDocument();
  });
});
