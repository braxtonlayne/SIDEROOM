// Phase 19 (mod-age): the age tools on the case page (MOD-AGE-001, -003), the hold decision after a deletion
// request (MOD-DSR-005), and hiding activity / correcting a Person link from the data request queue
// (MOD-DSR-006, -007).
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { HttpResponse, asStaff, http, openAdmin, readJson, server } from './helpers';

const DAY = 86_400_000;
const NOW = Date.now();

const CASE = {
  id: '9001',
  state: 'open',
  category: 'nsfw_policy',
  severity: 'standard',
  routing: 'platform',
  escalated: false,
  summary: 'message report: nsfw_policy',
  target_kind: 'message',
  target_id: '5',
  target_channel_id: '88',
  guild_id: '77',
  assignee_id: null,
  report_count: 1,
  active_holds: 0,
  opened_at_ms: 1_700_000_000_000,
  updated_at_ms: 1_700_000_000_000,
  closed_at_ms: null,
};

const DETAIL = {
  case: CASE,
  subject_person_id: '70',
  subject_account_id: '71',
  linked_case_id: null,
  outcome: '',
  reports: [],
  evidence: [],
  notes: [],
  events: [
    {
      id: '10',
      actor_id: '1',
      kind: 'age_restriction_set',
      detail: { label: 'general', restricted: true, reason: 'misplaced' },
      created_at_ms: 1_700_000_004_000,
    },
    {
      id: '11',
      actor_id: '1',
      kind: 'erasure_hold_flagged',
      detail: {},
      created_at_ms: 1_700_000_005_000,
    },
  ],
  holds: [],
  appeal: null,
  access: 'manage',
};

describe('Case page age tools (MOD-AGE-001, -003)', () => {
  it(
    'marks the channel of a misplaced-adult-content case with a reason, and reads the timeline in words',
    { timeout: 30_000 },
    async () => {
      asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
      const calls: unknown[] = [];
      server.use(
        http.get('*/api/v1/moderation/cases', () =>
          HttpResponse.json({ items: [CASE], next_cursor: null }),
        ),
        http.get('*/api/v1/moderation/cases/9001', () => HttpResponse.json(DETAIL)),
        http.post('*/api/v1/moderation/age/restrictions', async ({ request }) => {
          calls.push(await readJson(request));
          return HttpResponse.json({
            kind: 'channel',
            id: '88',
            label: 'general',
            restricted: true,
            changed: true,
          });
        }),
      );
      openAdmin('moderation');
      const user = userEvent.setup();
      // The first test of the file loads the lazy admin chunks, so it waits longer than the default.
      await user.click(
        await screen.findByRole('button', { name: 'Open case 9001' }, { timeout: 15_000 }),
      );
      await screen.findByRole('heading', { name: 'Case 9001' });
      expect(screen.getByText('Marked general age-restricted: misplaced')).toBeInTheDocument();
      expect(screen.getByText(/A deletion request met a hold on this case/)).toBeInTheDocument();

      await user.click(screen.getByRole('button', { name: 'Age restriction' }));
      const dialog = await screen.findByRole('dialog', {
        name: 'Age restriction and underage handling',
      });
      await user.type(
        within(dialog).getByLabelText(/Reason/),
        'adult images outside an 18+ channel',
      );
      await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
      expect(await screen.findByText('Done.')).toBeInTheDocument();
      expect(calls).toEqual([
        {
          kind: 'channel',
          id: '88',
          restricted: true,
          reason: 'adult images outside an 18+ channel',
          case_id: '9001',
        },
      ]);
    },
  );

  it('resets and locks an underage account from the case', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    const calls: unknown[] = [];
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [CASE], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/cases/9001', () => HttpResponse.json(DETAIL)),
      http.post('*/api/v1/moderation/age/underage', async ({ request }) => {
        calls.push(await readJson(request));
        return HttpResponse.json({
          account_id: '71',
          band: 'under_18',
          suspended: false,
          member_notified: true,
          inviter_notified: false,
        });
      }),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Open case 9001' }));
    await screen.findByRole('heading', { name: 'Case 9001' });
    await user.click(screen.getByRole('button', { name: 'Age restriction' }));
    const dialog = await screen.findByRole('dialog', {
      name: 'Age restriction and underage handling',
    });
    await user.click(within(dialog).getByRole('combobox', { name: 'What to do' }));
    await user.click(
      await screen.findByRole('option', {
        name: 'Account is under 18: reset and lock the age confirmation',
      }),
    );
    await user.type(within(dialog).getByLabelText(/Reason/), 'says they are 16');
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    expect(await screen.findByText('Done.')).toBeInTheDocument();
    expect(calls).toEqual([
      { account_id: '71', band: 'under_18', note: 'says they are 16', case_id: '9001' },
    ]);
  });

  it('offers no age tools on a case the server moderators still work', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [CASE], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/cases/9001', () =>
        HttpResponse.json({ ...DETAIL, case: { ...CASE, routing: 'guild' } }),
      ),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Open case 9001' }));
    await screen.findByRole('heading', { name: 'Case 9001' });
    expect(screen.queryByRole('button', { name: 'Age restriction' })).toBeNull();
  });
});

describe('Holds after a deletion request (MOD-DSR-005)', () => {
  it('flags a hold a deletion request met and keeps it with a stated legal basis', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    const calls: unknown[] = [];
    const hold = {
      id: '12',
      case_id: '9001',
      scope_kind: 'message',
      scope_id: '5',
      reason: 'Reported by a member',
      created_by: null,
      created_at_ms: 1_700_000_000_000,
      released_at_ms: null,
      release_reason: null,
      messages: 1,
      attachments: 0,
      submissions: 0,
      review_due: false,
      erasure_flagged: true,
      erasure_request_id: '900',
    };
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/holds', () => HttpResponse.json([hold])),
      http.post('*/api/v1/moderation/holds/12/retain', async ({ request }) => {
        calls.push(await readJson(request));
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await screen.findByText('No cases');
    await user.click(screen.getByRole('tab', { name: 'Holds' }));
    expect(await screen.findByText('Deletion request')).toBeInTheDocument();
    expect(screen.getByText(/data request 900\) met this hold/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retain hold 12 with a legal basis' }));
    const dialog = await screen.findByRole('dialog', { name: 'Retain the held rows' });
    const confirm = within(dialog).getByRole('button', { name: 'Retain' });
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByLabelText('Legal basis'), 'pending legal claim');
    await user.click(confirm);
    expect(await screen.findByText('The hold was marked as retained.')).toBeInTheDocument();
    expect(calls).toEqual([{ basis: 'pending legal claim' }]);
  });
});

const request = (over: Record<string, unknown> = {}) => ({
  id: '900',
  kind: 'export',
  person_id: '500',
  account_id: '100',
  username: 'alice',
  state: 'open',
  note: '',
  received_at_ms: NOW - 5 * DAY,
  due_at_ms: NOW + 25 * DAY,
  extended_until_ms: null,
  extension_reason: null,
  effective_due_ms: NOW + 25 * DAY,
  overdue: false,
  verified_by: null,
  verified_at_ms: null,
  export_id: null,
  outcome: null,
  retained: [],
  completed_at_ms: null,
  extended_at_ms: null,
  extended_late: false,
  member_notified_at_ms: null,
  activity_hidden: false,
  activity_hidden_at_ms: null,
  activity_hidden_reason: null,
  created_by: '1',
  ...over,
});

describe('Data requests: restriction and rectification (MOD-DSR-006, -007)', () => {
  it('hides the requester activity with a reason, shows the badge, and shows it again', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    let current = request();
    const calls: unknown[] = [];
    server.use(
      http.get('*/api/v1/admin/data-requests', () =>
        HttpResponse.json({ items: [current], deadline_days: 30, retained_catalog: [] }),
      ),
      http.post('*/api/v1/admin/data-requests/900/restriction', async ({ request: req }) => {
        const body = (await readJson(req)) as { hide: boolean; reason: string };
        calls.push(body);
        current = request(
          body.hide
            ? {
                activity_hidden: true,
                activity_hidden_at_ms: NOW,
                activity_hidden_reason: body.reason,
              }
            : {},
        );
        return HttpResponse.json(current);
      }),
    );
    openAdmin('data-requests');
    const user = userEvent.setup();
    await user.click(
      await screen.findByRole('button', {
        name: 'Hide the shared activity of alice from discovery',
      }),
    );
    let dialog = await screen.findByRole('dialog', { name: 'Hide activity from discovery' });
    await user.type(within(dialog).getByLabelText(/Reason/), 'objects to profiling');
    await user.click(within(dialog).getByRole('button', { name: 'Hide activity' }));
    expect(await screen.findByText('Activity hidden')).toBeInTheDocument();
    expect(screen.getByText(/objects to profiling/)).toBeInTheDocument();

    await user.click(
      await screen.findByRole('button', { name: 'Show the shared activity of alice again' }),
    );
    dialog = await screen.findByRole('dialog', { name: 'Show activity again' });
    await user.type(within(dialog).getByLabelText(/Reason/), 'review finished');
    await user.click(within(dialog).getByRole('button', { name: 'Show activity' }));
    expect(await screen.findByText('The activity is visible again.')).toBeInTheDocument();
    expect(calls).toEqual([
      { hide: true, reason: 'objects to profiling' },
      { hide: false, reason: 'review finished' },
    ]);
  });

  it('corrects a Person link with a reason, or splits the account off when no Person is given', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS', 'MANAGE_ACCOUNTS'] });
    const calls: unknown[] = [];
    server.use(
      http.get('*/api/v1/admin/data-requests', () =>
        HttpResponse.json({ items: [request()], deadline_days: 30, retained_catalog: [] }),
      ),
      http.post('*/api/v1/admin/data-requests/900/rectify', async ({ request: req }) => {
        calls.push(await readJson(req));
        return HttpResponse.json({ person_id: '300' });
      }),
    );
    openAdmin('data-requests');
    const user = userEvent.setup();
    await user.click(
      await screen.findByRole('button', { name: 'Correct the Person link of alice' }),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Correct a Person link' });
    await user.type(within(dialog).getByLabelText('Person id'), '300');
    await user.type(within(dialog).getByLabelText('Why the link was wrong'), 'linked by mistake');
    await user.click(within(dialog).getByRole('button', { name: 'Correct' }));
    expect(await screen.findByText('The Person link was corrected.')).toBeInTheDocument();
    expect(calls).toEqual([{ reason: 'linked by mistake', person_id: '300' }]);
  });
});
