import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { HttpResponse, asStaff, http, openAdmin, readJson, server } from './helpers';

const HEALTH = {
  window_days: 30,
  reports_per_day: [
    { day: '2026-10-20', reports: 1 },
    { day: '2026-10-21', reports: 0 },
    { day: '2026-10-22', reports: 4 },
  ],
  reports: 5,
  cases_opened: 3,
  median_first_action_seconds: 5_400,
  cases_with_first_action: 2,
  cases_waiting: 1,
  closed: 4,
  reopened: 1,
  reopen_rate: 0.25,
  appeal_outcomes: [
    { key: 'overturned', count: 2 },
    { key: 'upheld', count: 3 },
  ],
  appeals_open: 1,
};

const DASH = {
  generated_at_ms: 1_700_000_000_000,
  window_days: 30,
  inbox: { groups: 2, reports: 3, oldest_seconds: 7_300 },
  health: HEALTH,
  retention: {
    rows: [
      { key: 'messages_held', count: 12, capped: false },
      { key: 'attachments_expiry_held', count: 10_000, capped: true },
      { key: 'submissions_held', count: 0, capped: false },
    ],
    holds_active: 7,
    holds_review_due: 2,
  },
  age: {
    window_days: 30,
    restricted_channels: 4,
    restricted_series: 5,
    restricted_titles: 6,
    age_gate_resets: 8,
    child_safety_open_cases: 1,
    child_safety_reports: 2,
    child_safety_active_bans: 3,
  },
  bans: null,
  takedowns: null,
  data_requests: null,
};

const GROUPS = {
  items: [
    {
      case_id: '9001',
      summary: 'Scam links in a DM',
      target_kind: 'message',
      severity: 'standard',
      categories: ['spam'],
      reports: 2,
      first_report_at_ms: 1_699_000_000_000,
      last_report_at_ms: 1_699_000_100_000,
    },
    {
      case_id: '9002',
      summary: '',
      target_kind: 'user',
      severity: 'serious',
      categories: ['child_safety'],
      reports: 1,
      first_report_at_ms: 1_699_000_200_000,
      last_report_at_ms: 1_699_000_200_000,
    },
  ],
  next_cursor: null,
};

describe('Admin > Moderation > Dashboard', () => {
  it('shows the sections a case moderator may read and nothing of the others', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/dashboard', () => HttpResponse.json(DASH)),
      http.get('*/api/v1/moderation/dashboard/inbox', () => HttpResponse.json(GROUPS)),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Dashboard' }, { timeout: 15000 }));

    // Inbox: groups, duplicate marking and no reporter anywhere.
    expect(await screen.findByRole('heading', { name: 'Reports inbox' })).toBeInTheDocument();
    const inbox = await screen.findByRole('list', { name: 'Reports waiting in the inbox' });
    expect(within(inbox).getAllByRole('listitem')).toHaveLength(2);
    expect(within(inbox).getByText('2 duplicate reports')).toBeInTheDocument();
    expect(within(inbox).getByText('Scam links in a DM')).toBeInTheDocument();

    // Health: median, reopen rate, appeal outcomes and the per-day bars with their numbers.
    expect(screen.getByRole('heading', { name: 'Health' })).toBeInTheDocument();
    expect(screen.getByText('Median time to first action')).toBeInTheDocument();
    expect(screen.getByText('25%')).toBeInTheDocument();
    expect(screen.getByText('3 upheld, 2 overturned')).toBeInTheDocument();
    const perDay = screen.getByRole('list', { name: 'Reports per day' });
    expect(within(perDay).getAllByRole('listitem')).toHaveLength(3);

    // Retention check: a capped count says "at least".
    const retention = screen.getByRole('list', { name: 'Rows skipped by purge jobs' });
    expect(within(retention).getByText('at least 10,000')).toBeInTheDocument();
    expect(within(retention).getByText('Deleted messages due for purge, held')).toBeInTheDocument();

    // Age and child safety.
    expect(screen.getByRole('heading', { name: 'Age and child safety' })).toBeInTheDocument();
    expect(screen.getByText('Active child-safety bans')).toBeInTheDocument();

    // Sections the API left out are not drawn.
    expect(screen.queryByRole('heading', { name: 'Platform bans' })).toBeNull();
    expect(screen.queryByRole('heading', { name: 'Takedown register' })).toBeNull();
    expect(screen.queryByRole('heading', { name: 'Data requests' })).toBeNull();
  });

  it('dismisses the picked inbox cases with one reason', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    let sent: unknown = null;
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/dashboard', () => HttpResponse.json(DASH)),
      http.get('*/api/v1/moderation/dashboard/inbox', () => HttpResponse.json(GROUPS)),
      http.post('*/api/v1/moderation/dashboard/inbox/dismiss', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({
          dismissed: ['9001'],
          skipped: [{ case_id: '9002', reason: 'The case is no longer waiting in the inbox.' }],
        });
      }),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Dashboard' }, { timeout: 15000 }));
    await screen.findByRole('list', { name: 'Reports waiting in the inbox' });
    const dismiss = screen.getByRole('button', { name: 'Dismiss selected' });
    expect(dismiss).toBeDisabled();
    await user.click(screen.getByRole('checkbox', { name: 'Select case 9001' }));
    await user.click(screen.getByRole('checkbox', { name: 'Select case 9002' }));
    await user.click(screen.getByRole('button', { name: 'Dismiss 2 selected' }));
    const dialog = await screen.findByRole('dialog', { name: 'Dismiss 2 cases' });
    const confirm = within(dialog).getByRole('button', { name: 'Dismiss' });
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByLabelText(/reason/i), 'Not a violation');
    await user.click(confirm);
    expect(
      await screen.findByText(/1 case dismissed; 1 skipped because it left the inbox/),
    ).toBeInTheDocument();
    expect(sent).toEqual({ case_ids: ['9001', '9002'], reason: 'Not a violation' });
  });

  it('a takedown clerk gets the register and the data requests', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    server.use(
      http.get('*/api/v1/moderation/takedowns', () => HttpResponse.json([])),
      http.get('*/api/v1/moderation/dashboard', () =>
        HttpResponse.json({
          generated_at_ms: 1_700_000_000_000,
          window_days: 30,
          takedowns: {
            window_days: 30,
            undecided: 2,
            triage_overdue: 1,
            counter_noticed: 3,
            restore_due: 1,
            removed_in_window: 4,
            received_in_window: 6,
          },
          data_requests: {
            window_days: 30,
            open_exports: 1,
            open_erasures: 2,
            overdue: 1,
            due_soon: 2,
            completed_in_window: 5,
          },
        }),
      ),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Dashboard' }, { timeout: 15000 }));
    expect(await screen.findByRole('heading', { name: 'Takedown register' })).toBeInTheDocument();
    expect(screen.getByText('Past the triage target')).toBeInTheDocument();
    expect(screen.getByText('These need a decision first.')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Data requests' })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: 'Open the data requests' })).toHaveAttribute(
      'href',
      '/admin/data-requests',
    );
    expect(screen.queryByRole('heading', { name: 'Reports inbox' })).toBeNull();
    // The register button switches to the Takedowns tab.
    await user.click(screen.getByRole('button', { name: 'Open the register' }));
    expect(await screen.findByRole('button', { name: 'Enter a notice' })).toBeInTheDocument();
  });

  it('shows the ban figures to staff who also hold PLATFORM_BAN', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES', 'PLATFORM_BAN'] });
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/dashboard', () =>
        HttpResponse.json({
          generated_at_ms: 1_700_000_000_000,
          window_days: 30,
          bans: {
            active: 4,
            by_reason: [
              { key: 'fraud', count: 2 },
              { key: 'none', count: 2 },
            ],
            by_severity: [{ key: 'serious', count: 2 }],
            by_age: [
              { key: 'under_7d', count: 1 },
              { key: '7_to_30d', count: 0 },
              { key: '30_to_90d', count: 1 },
              { key: 'over_90d', count: 2 },
            ],
            lifted_in_window: 1,
          },
        }),
      ),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Dashboard' }, { timeout: 15000 }));
    expect(await screen.findByRole('heading', { name: 'Platform bans' })).toBeInTheDocument();
    const reasons = screen.getByRole('list', { name: 'Active bans by reason' });
    expect(within(reasons).getByText('Fraud or impersonation')).toBeInTheDocument();
    expect(within(reasons).getByText('No reason recorded')).toBeInTheDocument();
    expect(screen.getByRole('list', { name: 'Active bans by age' })).toBeInTheDocument();
    expect(screen.getByText('Over 90 days')).toBeInTheDocument();
  });
});

describe('Admin > Bans filters', () => {
  it('sends the severity, reason and age filters', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'PLATFORM_BAN'] });
    const urls: string[] = [];
    server.use(
      http.get('*/api/v1/admin/bans', ({ request }) => {
        urls.push(new URL(request.url).search);
        return HttpResponse.json({
          items: [
            {
              id: '1',
              person_id: '500',
              banned_by: '9',
              category: 'serious',
              note: 'scam',
              created_at_ms: 1_700_000_000_000,
              lifted_at_ms: null,
              reason_category: 'fraud',
            },
          ],
          next_after: null,
        });
      }),
    );
    openAdmin('bans');
    const user = userEvent.setup();
    expect(await screen.findByText('Fraud or impersonation')).toBeInTheDocument();
    expect(urls[0]).toBe('?active_only=true&limit=50');

    await user.click(screen.getByRole('combobox', { name: 'Severity' }));
    await user.click(await screen.findByRole('option', { name: 'Serious' }));
    await user.click(screen.getByRole('combobox', { name: 'Reason' }));
    await user.click(await screen.findByRole('option', { name: 'No reason recorded' }));
    await user.click(screen.getByRole('combobox', { name: 'Age of the ban' }));
    await user.click(await screen.findByRole('option', { name: '7 to 30 days' }));
    await screen.findByText('Fraud or impersonation');
    const last = new URLSearchParams(urls[urls.length - 1]);
    expect(last.get('category')).toBe('serious');
    expect(last.get('reason_category')).toBe('none');
    expect(last.get('older_than_days')).toBe('7');
    expect(last.get('newer_than_days')).toBe('30');
    expect(last.get('active_only')).toBe('true');
  });
});

describe('Admin > Bans figures (D-340)', () => {
  const BANS = {
    generated_at_ms: 1_700_000_000_000,
    window_days: 30,
    bans: {
      active: 4,
      by_reason: [
        { key: 'fraud', count: 2 },
        { key: 'none', count: 2 },
      ],
      by_severity: [{ key: 'serious', count: 2 }],
      by_age: [
        { key: 'under_7d', count: 1 },
        { key: '7_to_30d', count: 0 },
        { key: '30_to_90d', count: 1 },
        { key: 'over_90d', count: 2 },
      ],
      lifted_in_window: 1,
    },
  };

  it('shows the figures to PLATFORM_BAN alone and a click sets the filter', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'PLATFORM_BAN'] });
    const urls: string[] = [];
    server.use(
      http.get('*/api/v1/moderation/dashboard', () => HttpResponse.json(BANS)),
      http.get('*/api/v1/admin/bans', ({ request }) => {
        urls.push(new URL(request.url).search);
        return HttpResponse.json({ items: [], next_after: null });
      }),
    );
    openAdmin('bans');
    const user = userEvent.setup();
    const reasons = await screen.findByRole(
      'group',
      { name: 'In force by reason' },
      { timeout: 15000 },
    );
    expect(within(reasons).getByText('No reason recorded: 2')).toBeInTheDocument();
    expect(screen.getByText('4 bans in force; 1 lifted in this window.')).toBeInTheDocument();
    await user.click(
      within(reasons).getByRole('button', { name: 'Show Fraud or impersonation bans (2)' }),
    );
    await user.click(screen.getByRole('button', { name: 'Show Over 90 days bans (2)' }));
    await user.click(screen.getByRole('button', { name: 'Show Serious bans (2)' }));
    await vi.waitFor(() => {
      const q = new URLSearchParams(urls[urls.length - 1]);
      expect(q.get('reason_category')).toBe('fraud');
      expect(q.get('older_than_days')).toBe('90');
      expect(q.get('category')).toBe('serious');
    });
  });

  it('a caller without the bit never reaches the page and makes no dashboard call', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    let calls = 0;
    server.use(
      http.get('*/api/v1/moderation/dashboard', () => {
        calls += 1;
        return HttpResponse.json(BANS);
      }),
      http.get('*/api/v1/admin/stats', () => HttpResponse.json({})),
    );
    openAdmin('bans');
    await new Promise((r) => setTimeout(r, 1500));
    expect(screen.queryByRole('group', { name: 'In force by reason' })).toBeNull();
    expect(calls).toBe(0);
  });
});
