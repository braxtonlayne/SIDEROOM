import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { HttpResponse, asStaff, http, openAdmin, readJson, server } from './helpers';

const CASE = {
  id: '9001',
  state: 'open',
  category: 'harassment',
  severity: 'standard',
  routing: 'platform',
  escalated: false,
  summary: 'message report: harassment',
  target_kind: 'message',
  target_id: '5',
  guild_id: null,
  assignee_id: null,
  report_count: 2,
  active_holds: 1,
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
  reports: [
    {
      id: '1',
      category: 'harassment',
      note: 'He keeps doing it',
      state: 'received',
      moderators_may_see_name: false,
      reporter: { account_id: '20', username: 'reporter' },
      created_at_ms: 1_700_000_000_000,
    },
    {
      id: '2',
      category: 'harassment',
      note: '',
      state: 'received',
      moderators_may_see_name: false,
      reporter: null,
      created_at_ms: 1_700_000_100_000,
    },
  ],
  evidence: [
    {
      id: '3',
      report_id: '1',
      kind: 'message_snapshot',
      payload: {
        messages: [
          {
            id: '4',
            target: false,
            author_id: '71',
            author_username: 'pat',
            content: 'hello there',
            visibility: 'visible',
            created_at_ms: 1_700_000_000_000,
            attachments: [],
          },
          {
            id: '5',
            target: true,
            author_id: '71',
            author_username: 'pat',
            content: 'the reported words',
            visibility: 'visible',
            created_at_ms: 1_700_000_001_000,
            attachments: [
              { id: '6', filename: 'pic.png', content_type: 'image/png', size_bytes: 10 },
            ],
          },
        ],
      },
      created_at_ms: 1_700_000_002_000,
    },
  ],
  notes: [{ id: '8', author_id: '1', body: 'looks real', created_at_ms: 1_700_000_003_000 }],
  events: [
    { id: '10', actor_id: null, kind: 'case_opened', detail: {}, created_at_ms: 1_700_000_000_000 },
    {
      id: '11',
      actor_id: '1',
      kind: 'state_changed',
      detail: { from: 'open', to: 'investigating' },
      created_at_ms: 1_700_000_004_000,
    },
  ],
  holds: [
    {
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
      attachments: 1,
      submissions: 0,
      review_due: true,
    },
  ],
  appeal: null,
  access: 'manage',
};

describe('Admin > Moderation', () => {
  it('is hidden from staff with no case or review bit', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'PLATFORM_BAN'] });
    server.use(http.get('*/api/v1/admin/stats', () => HttpResponse.json({})));
    openAdmin('dashboard');
    await screen.findByRole('heading', { name: 'Dashboard' });
    expect(screen.queryByRole('button', { name: 'Moderation' })).toBeNull();
  });

  it('lists the queue and opens a case with the reporter, the snapshot, the hold and the timeline', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    let query = '';
    server.use(
      http.get('*/api/v1/moderation/cases', ({ request }) => {
        query = new URL(request.url).search;
        return HttpResponse.json({ items: [CASE], next_cursor: null });
      }),
      http.get('*/api/v1/moderation/cases/9001', () => HttpResponse.json(DETAIL)),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    const row = await screen.findByRole('button', { name: 'Open case 9001' });
    expect(query).toContain('open_only=true');
    expect(query).toContain('oldest_first=true');
    expect(screen.getByText(/2 reports/)).toBeInTheDocument();
    await user.click(row);

    expect(await screen.findByRole('heading', { name: 'Case 9001' })).toBeInTheDocument();
    // The first reporter is shown, the second is hidden from this viewer.
    expect(screen.getByText(/Reported by reporter/)).toBeInTheDocument();
    expect(screen.getByText(/Reporter hidden/)).toBeInTheDocument();
    // The snapshot marks the reported message.
    const snapshot = screen.getByRole('list', { name: 'Messages around the report' });
    expect(within(snapshot).getByText('the reported words')).toBeInTheDocument();
    expect(within(snapshot).getByText('Reported message')).toBeInTheDocument();
    expect(within(snapshot).getByText('pic.png')).toBeInTheDocument();
    // The hold is flagged for review, and the timeline reads in words.
    expect(screen.getByText('Due for review')).toBeInTheDocument();
    expect(screen.getByText('Moved from Open to Investigating')).toBeInTheDocument();
    expect(screen.getByText('looks real')).toBeInTheDocument();
  });

  it('claims, moves and closes a case with the outcome and the line for the reporters', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    const calls: { path: string; body: unknown }[] = [];
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [CASE], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/cases/9001', () =>
        HttpResponse.json({ ...DETAIL, case: { ...CASE, state: 'dismissed' } }),
      ),
      http.put('*/api/v1/moderation/cases/9001/assignee', () => {
        calls.push({ path: 'claim', body: null });
        return HttpResponse.json(CASE);
      }),
      http.post('*/api/v1/moderation/cases/9001/state', async ({ request }) => {
        calls.push({ path: 'state', body: await readJson(request) });
        return HttpResponse.json(CASE);
      }),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Open case 9001' }));
    await screen.findByRole('heading', { name: 'Case 9001' });
    await user.click(screen.getByRole('button', { name: 'Claim' }));
    await user.click(screen.getByRole('button', { name: 'Close case' }));
    const dialog = await screen.findByRole('dialog', { name: 'Close this case' });
    const confirm = within(dialog).getByRole('button', { name: 'Close case' });
    expect(confirm).toBeDisabled();
    await user.type(
      within(dialog).getByLabelText('What was decided, in a few words'),
      'Not a breach',
    );
    await user.type(
      within(dialog).getByLabelText(/A line for the people who reported/),
      'Thanks, nothing broke the rules.',
    );
    await user.click(confirm);
    expect(await screen.findByText('The case is closed.')).toBeInTheDocument();
    expect(calls).toEqual([
      { path: 'claim', body: null },
      {
        path: 'state',
        body: {
          to: 'closed',
          outcome: 'Not a breach',
          reporter_line: 'Thanks, nothing broke the rules.',
        },
      },
    ]);
  });

  it('places a hold with a reason and releases one with a reason', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    const calls: { path: string; body: unknown }[] = [];
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [CASE], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/cases/9001', () => HttpResponse.json(DETAIL)),
      http.post('*/api/v1/moderation/cases/9001/holds', async ({ request }) => {
        calls.push({ path: 'place', body: await readJson(request) });
        return HttpResponse.json(
          { hold_id: '13', messages: 3, attachments: 0, submissions: 0 },
          { status: 201 },
        );
      }),
      http.post('*/api/v1/moderation/holds/12/release', async ({ request }) => {
        calls.push({ path: 'release', body: await readJson(request) });
        return HttpResponse.json({
          messages: 1,
          attachments: 1,
          submissions: 0,
          purgeable_messages: 0,
        });
      }),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Open case 9001' }));
    await screen.findByRole('heading', { name: 'Case 9001' });

    await user.click(screen.getByRole('button', { name: 'Place hold' }));
    let dialog = await screen.findByRole('dialog', { name: 'Place a hold' });
    await user.type(within(dialog).getByLabelText('Id'), '5');
    await user.type(within(dialog).getByLabelText(/Reason/), 'evidence');
    await user.click(within(dialog).getByRole('button', { name: 'Place hold' }));
    expect(await screen.findByText('Held.')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Release hold 12' }));
    dialog = await screen.findByRole('dialog', { name: 'Release this hold' });
    const go = within(dialog).getByRole('button', { name: 'Release' });
    expect(go).toBeDisabled();
    await user.type(within(dialog).getByLabelText(/Reason/), 'case closed');
    await user.click(go);
    expect(await screen.findByText('The hold is released.')).toBeInTheDocument();
    expect(calls).toEqual([
      { path: 'place', body: { kind: 'message', id: '5', reason: 'evidence' } },
      { path: 'release', body: { reason: 'case closed' } },
    ]);
  });

  it('shows holds due for review and appeals waiting for a decision', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    let reviewed = false;
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/holds', () => HttpResponse.json(DETAIL.holds)),
      http.post('*/api/v1/moderation/holds/12/reviewed', () => {
        reviewed = true;
        return new HttpResponse(null, { status: 204 });
      }),
      http.get('*/api/v1/moderation/appeals', () =>
        HttpResponse.json([
          {
            id: '30',
            ban_id: '31',
            case_id: '9001',
            appellant_account_id: '71',
            message: 'It was not me',
            state: 'open',
            banned_by: '1',
            person_id: '70',
            decided_by: null,
            decision_reason: '',
            created_at_ms: 1_700_000_000_000,
            decided_at_ms: null,
          },
        ]),
      ),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    expect(await screen.findByText('No cases')).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Holds' }));
    expect(await screen.findByText('Due for review')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Keep hold 12 and mark it reviewed' }));
    expect(await screen.findByText('Marked as reviewed. It stays in force.')).toBeInTheDocument();
    expect(reviewed).toBe(true);
    await user.click(screen.getByRole('tab', { name: 'Appeals' }));
    expect(await screen.findByText('It was not me')).toBeInTheDocument();
    expect(screen.getByText('Account 71')).toBeInTheDocument();
  });
});

describe('New case (N-22)', () => {
  it('needs an account and a first note, sends the case, adds the note and opens it', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    const calls: { path: string; body: unknown }[] = [];
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
      http.post('*/api/v1/moderation/cases', async ({ request }) => {
        calls.push({ path: 'open', body: await readJson(request) });
        return HttpResponse.json({ ...CASE, id: '9100' }, { status: 201 });
      }),
      http.post('*/api/v1/moderation/cases/9100/notes', async ({ request }) => {
        calls.push({ path: 'note', body: await readJson(request) });
        return HttpResponse.json(
          { id: '1', author_id: '1', body: 'x', created_at_ms: 1 },
          { status: 201 },
        );
      }),
      http.get('*/api/v1/moderation/cases/9100', () =>
        HttpResponse.json({ ...DETAIL, case: { ...CASE, id: '9100' } }),
      ),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'New case' }));
    const dialog = await screen.findByRole('dialog', { name: 'Open a case' });
    const go = within(dialog).getByRole('button', { name: 'Open case' });
    expect(go).toBeDisabled();
    await user.type(within(dialog).getByLabelText('Account id of the person'), '71');
    expect(go).toBeDisabled();
    await user.type(within(dialog).getByLabelText('First note'), 'Saw chargebacks');
    await user.type(within(dialog).getByLabelText('Server id (optional)'), '8');
    await user.click(go);
    expect(await screen.findByRole('heading', { name: 'Case 9100' })).toBeInTheDocument();
    expect(calls).toEqual([
      {
        path: 'open',
        body: {
          category: 'other',
          target_kind: 'user',
          target_id: '71',
          subject_account_id: '71',
          severity: 'standard',
          guild_id: '8',
        },
      },
      { path: 'note', body: { body: 'Saw chargebacks' } },
    ]);
  });
});

describe('Reviewers (N-23)', () => {
  it('see the section with the queue only: no holds, appeals or New case', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_READING'] });
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [{ ...CASE, routing: 'reviewers' }], next_cursor: null }),
      ),
    );
    openAdmin('moderation');
    expect(await screen.findByRole('button', { name: 'Open case 9001' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Holds' })).toBeNull();
    expect(screen.queryByRole('tab', { name: 'Appeals' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'New case' })).toBeNull();
  });
});
