// Imported-guide channels in Admin > Cinema > Live TV (TV-ADM-003 weekly grid, TV-ADM-004 guide import).
import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { findOverlap, minutesOf, sortSlots } from '../cinema/tv/GuidePanel';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const CURATOR = ['STAFF_BASIC', 'REVIEW_MEDIA', 'PUBLISH_MEDIA', 'MANAGE_TV'] as const;

const channel = () => ({
  id: '601',
  number: 120,
  call_sign: 'RETRO',
  name: 'Retro Block',
  era_label: 'Fall 2003',
  logo_url: null,
  kind: 'imported_guide',
  timezone: 'UTC',
  age_restricted: false,
  enabled: true,
  filler_policy: 'slate',
  theme: null,
  seed: 1,
  viewers: 0,
  rules: null,
});

const slot = (over: Record<string, unknown> = {}) => ({
  id: '1',
  weekday: 0,
  start: '20:00',
  duration_min: 30,
  show_name: 'Cowboy Bebop',
  title_id: '7',
  ...over,
});

const report = (confirmed = false) => ({
  id: '900',
  channel_id: '601',
  format: 'csv',
  confirmed,
  created_at_ms: 1,
  report: {
    matched: 1,
    ambiguous: 1,
    missing: 1,
    invalid: 1,
    rows: [
      {
        line: 2,
        weekday: 0,
        start: '20:00',
        duration_min: 30,
        show: 'Cowboy Bebop',
        status: 'matched',
        title_id: '7',
        candidates: [{ title_id: '7', title: 'Cowboy Bebop', year: 1998, score: 1 }],
        message: null,
      },
      {
        line: 3,
        weekday: 0,
        start: '20:30',
        duration_min: 30,
        show: 'The Big Movie',
        status: 'ambiguous',
        title_id: null,
        candidates: [
          { title_id: '3', title: 'The Big Movie', year: 1999, score: 1 },
          { title_id: '4', title: 'The Big Movie', year: 2015, score: 1 },
        ],
        message: 'Pick the right title',
      },
      {
        line: 4,
        weekday: 1,
        start: '21:00',
        duration_min: 30,
        show: 'Nobody Knows',
        status: 'missing',
        title_id: null,
        candidates: [],
        message: 'Not in the library: this slot will be skipped',
      },
      {
        line: 5,
        weekday: 0,
        start: '',
        duration_min: 0,
        show: '',
        status: 'invalid',
        title_id: null,
        candidates: [],
        message: 'Unreadable start time "x"',
      },
    ],
  },
});

function useGuideApi(initial = [slot(), slot({ id: '2', start: '20:30', show_name: 'Unknown', title_id: null })]) {
  const writes: { method: string; url: string; body: unknown }[] = [];
  let slots = initial;
  server.use(
    http.get('*/api/v1/tv/timezones', () => HttpResponse.json({ items: ['UTC'] })),
    http.get('*/api/v1/admin/tv/channels', () => HttpResponse.json({ items: [channel()] })),
    http.get('*/api/v1/admin/tv/channels/:id/slots', () => HttpResponse.json({ items: slots })),
    http.put('*/api/v1/admin/tv/channels/:id/slots', async ({ request }) => {
      const body = (await readJson(request)) as { items: typeof initial };
      writes.push({ method: 'PUT', url: 'slots', body });
      slots = body.items.map((s, i) => ({ ...s, id: String(i + 1) }));
      return HttpResponse.json({ items: slots });
    }),
    http.post('*/api/v1/admin/tv/channels/:id/imports', async ({ request }) => {
      writes.push({ method: 'POST', url: 'imports', body: await readJson(request) });
      return HttpResponse.json(report(), { status: 201 });
    }),
    http.post('*/api/v1/admin/tv/channels/:id/imports/:iid/confirm', async ({ request }) => {
      writes.push({ method: 'POST', url: 'confirm', body: await readJson(request) });
      slots = [slot()];
      return HttpResponse.json({ slots: 3, skipped: 2, warnings: [] });
    }),
    http.get('*/api/v1/admin/cinema/titles', () =>
      HttpResponse.json({
        items: [{ id: '9', title: 'Alien', year: 1979 }],
        total: 1,
        next_cursor: null,
      }),
    ),
  );
  return writes;
}

async function openGuide() {
  const user = userEvent.setup();
  openAdmin('cinema-live-tv');
  await user.click(await screen.findByRole('button', { name: 'Open Retro Block' }));
  await user.click(await screen.findByRole('tab', { name: 'Guide' }));
  return user;
}

describe('weekly grid helpers', () => {
  it('sorts by day and time and finds overlaps across midnight and the turn of the week', () => {
    const a = slot({ id: 'a', weekday: 1, start: '09:00' });
    const b = slot({ id: 'b', weekday: 0, start: '23:00' });
    expect(sortSlots([a, b]).map((s) => s.id)).toEqual(['b', 'a']);
    expect(minutesOf('08:30')).toBe(510);
    expect(findOverlap([a, b])).toBeNull();
    // Monday 23:30 for 60 minutes runs into Tuesday 00:00..00:30.
    const late = slot({ id: 'c', weekday: 0, start: '23:30', duration_min: 60 });
    const dawn = slot({ id: 'd', weekday: 1, start: '00:15', duration_min: 30 });
    expect(findOverlap([late, dawn])?.map((s) => s.id)).toEqual(['c', 'd']);
    // Sunday night wraps into Monday morning.
    const sun = slot({ id: 'e', weekday: 6, start: '23:00', duration_min: 120 });
    const mon = slot({ id: 'f', weekday: 0, start: '00:30', duration_min: 30 });
    expect(findOverlap([sun, mon])).not.toBeNull();
  });
});

describe('Live TV admin: imported guide', () => {
  it('shows the weekly grid, marks unmatched slots as skipped and saves edits whole', async () => {
    asStaff({ perms: [...CURATOR] });
    const writes = useGuideApi();
    const user = await openGuide();
    const monday = await screen.findByRole('list', { name: /Monday/ });
    expect(within(monday).getByText('Cowboy Bebop')).toBeInTheDocument();
    expect(within(monday).getAllByText('Skipped')).toHaveLength(1);
    // Edit the second slot: rename it and move it to Tuesday through the form (no dragging needed).
    await user.click(within(monday).getByRole('button', { name: /Unknown/ }));
    const form = await screen.findByRole('form', { name: 'Slot' });
    const show = within(form).getByLabelText('Show as written in the guide');
    await user.clear(show);
    await user.type(show, 'Samurai Champloo');
    await user.click(screen.getByRole('button', { name: 'Save weekly grid' }));
    await waitFor(() => expect(writes).toHaveLength(1));
    const sent = (writes[0]!.body as { items: { show_name: string }[] }).items;
    expect(sent.map((s) => s.show_name)).toEqual(['Cowboy Bebop', 'Samurai Champloo']);
    expect(await screen.findByText(/Weekly grid saved/)).toBeInTheDocument();
  });

  it('moves a slot to another day by dropping it, and blocks saving an overlap', async () => {
    asStaff({ perms: [...CURATOR] });
    useGuideApi([slot(), slot({ id: '2', weekday: 1, start: '20:15', show_name: 'Clash' })]);
    const user = await openGuide();
    const tuesday = (await screen.findByRole('list', { name: /Tuesday/ })).closest('.adm-week-day')!;
    const monday = screen.getByRole('list', { name: /Monday/ });
    // Dropping Tuesday's slot on Monday makes it overlap Monday 20:00: the save is blocked and says why.
    const clash = within(tuesday as HTMLElement).getByRole('button', { name: /Clash/ });
    const data = new Map<string, string>();
    const dataTransfer = {
      setData: (k: string, v: string) => data.set(k, v),
      getData: (k: string) => data.get(k) ?? '',
    };
    fireEvent.dragStart(clash, { dataTransfer });
    fireEvent.drop(monday.closest('.adm-week-day')!, { dataTransfer });
    expect(await screen.findByRole('alert')).toHaveTextContent(/overlap/);
    expect(screen.getByRole('button', { name: 'Save weekly grid' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Discard changes' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('links a slot to a library title found by search', async () => {
    asStaff({ perms: [...CURATOR] });
    const writes = useGuideApi([slot({ title_id: null, show_name: 'Alien' })]);
    const user = await openGuide();
    await user.click(await screen.findByRole('button', { name: /Alien/ }));
    const form = await screen.findByRole('form', { name: 'Slot' });
    await user.click(within(form).getByRole('button', { name: 'Search' }));
    await user.click(await within(form).findByRole('button', { name: 'Use Alien' }));
    expect(within(form).getByText('Linked to library title 9.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Save weekly grid' }));
    await waitFor(() => expect(writes).toHaveLength(1));
    expect((writes[0]!.body as { items: { title_id: string }[] }).items[0]!.title_id).toBe('9');
  });

  it('imports a file, shows the mapping report and confirms with the picked title', async () => {
    asStaff({ perms: [...CURATOR] });
    const writes = useGuideApi();
    const user = await openGuide();
    expect(await screen.findByRole('link', { name: 'Download the CSV template' })).toHaveAttribute(
      'href',
      '/api/v1/admin/tv/guide-template?format=csv',
    );
    const file = new File(
      ['weekday,start_time,duration,show\nMon,20:00,30,Cowboy Bebop\n'],
      'guide.csv',
      { type: 'text/csv' },
    );
    await user.upload(screen.getByLabelText('Guide file'), file);
    expect(
      await screen.findByText('1 matched, 1 to pick, 1 not in the library (skipped), 1 unreadable.'),
    ).toBeInTheDocument();
    expect(writes[0]).toMatchObject({
      url: 'imports',
      body: { format: 'csv', content: expect.stringContaining('Cowboy Bebop') },
    });
    const table = screen.getByRole('table', { name: 'Mapping report' });
    expect(within(table).getAllByRole('row')).toHaveLength(5);
    expect(within(table).getByText('Unreadable start time "x"')).toBeInTheDocument();
    await user.selectOptions(
      within(table).getByRole('combobox', { name: 'Title for line 3' }),
      '4',
    );
    await user.click(screen.getByRole('button', { name: 'Confirm and replace the weekly grid' }));
    await waitFor(() => expect(writes.at(-1)).toMatchObject({ url: 'confirm' }));
    expect(writes.at(-1)!.body).toEqual({ choices: [{ line: 3, title_id: '4' }] });
    expect(await screen.findByText(/Guide imported/)).toBeInTheDocument();
    // The grid reloaded: the confirmed import left one slot.
    await waitFor(() =>
      expect(within(screen.getByRole('list', { name: /Monday/ })).getAllByRole('button')).toHaveLength(1),
    );
  });
});
