import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { ApiProblem } from '../ui';
import { HttpResponse, asStaff, http, openAdmin, readJson, server } from './helpers';

const SUMMARY = {
  id: '5001',
  kind: 'copyright',
  state: 'received',
  notifier_name: 'Rights Holder Ltd',
  received_at_ms: 1_700_000_000_000,
  triage_due_at_ms: 1_700_086_400_000,
  triage_overdue: true,
  case_id: '9100',
  items: 0,
  removed_at_ms: null,
  restore_after_ms: null,
  court_filing_noted: false,
};

const DETAIL = {
  ...SUMMARY,
  notifier_contact: 'legal@rights.example',
  legal_basis: 'Owner of Stolen Saga',
  locations: ['https://side.example/library/series/1'],
  original_text: 'Please remove it.',
  entered_by: '1',
  rejection_reason: '',
  counter_notice_text: '',
  counter_noticed_at_ms: null,
  restored_at_ms: null,
  restore_reason: '',
  items: [] as unknown[],
  events: [
    { id: '1', actor_id: '1', kind: 'received', detail: {}, created_at_ms: 1_700_000_000_000 },
  ],
  strikes: [] as unknown[],
};

const ITEM = {
  id: '70',
  kind: 'library_series',
  target_id: '700',
  label: 'Series Stolen Saga',
  subject_account_id: '71',
  state: 'identified',
  hold_id: null,
  disabled_at_ms: null,
  restored_at_ms: null,
};

describe('Admin > Moderation > Takedowns', () => {
  it('needs MANAGE_TAKEDOWNS: a case moderator sees the case tabs only', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
    );
    openAdmin('moderation');
    await screen.findByRole('tab', { name: 'Cases' });
    expect(screen.queryByRole('tab', { name: 'Takedowns' })).toBeNull();
  });

  it('opens for staff with MANAGE_TAKEDOWNS alone, without the case tabs', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    server.use(http.get('*/api/v1/moderation/takedowns', () => HttpResponse.json([SUMMARY])));
    openAdmin('moderation');
    expect(await screen.findByRole('tab', { name: 'Takedowns' })).toBeInTheDocument();
    expect(screen.queryByRole('tab', { name: 'Cases' })).toBeNull();
    expect(await screen.findByText('Rights Holder Ltd', { exact: false })).toBeInTheDocument();
    expect(screen.getByText('Triage overdue')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Export CSV' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Export JSON' })).toBeInTheDocument();
  });

  it('enters a notice from the form and lands on its page', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    let sent: unknown = null;
    server.use(
      http.get('*/api/v1/moderation/takedowns', () => HttpResponse.json([])),
      http.post('*/api/v1/moderation/takedowns', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(DETAIL, { status: 201 });
      }),
      http.get('*/api/v1/moderation/takedowns/5001', () => HttpResponse.json(DETAIL)),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Enter a notice' }));
    const dialog = await screen.findByRole('dialog', { name: 'Enter a takedown notice' });
    const enter = within(dialog).getByRole('button', { name: 'Enter notice' });
    expect(enter).toBeDisabled();
    await user.type(within(dialog).getByLabelText('Who sent it'), 'Rights Holder Ltd');
    await user.type(within(dialog).getByLabelText('How to reach them'), 'legal@rights.example');
    await user.type(
      within(dialog).getByLabelText(/Work claimed or legal basis/),
      'Owner of Stolen Saga',
    );
    await user.type(
      within(dialog).getByLabelText(/Addresses or ids they named/),
      'https://side.example/library/series/1',
    );
    await user.type(within(dialog).getByLabelText(/The notice as received/), 'Please remove it.');
    await user.click(enter);
    expect(
      await screen.findByRole('heading', { name: 'Takedown notice TK-5001' }),
    ).toBeInTheDocument();
    expect(sent).toEqual({
      kind: 'copyright',
      notifier_name: 'Rights Holder Ltd',
      notifier_contact: 'legal@rights.example',
      legal_basis: 'Owner of Stolen Saga',
      locations: ['https://side.example/library/series/1'],
      original_text: 'Please remove it.',
    });
  });

  it('identifies content, upholds the notice and reports who was told', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    let detail: Record<string, unknown> = { ...DETAIL, items: [] as unknown[] };
    const calls: { path: string; body: unknown }[] = [];
    server.use(
      http.get('*/api/v1/moderation/takedowns', () => HttpResponse.json([SUMMARY])),
      http.get('*/api/v1/moderation/takedowns/5001', () => HttpResponse.json(detail)),
      http.post('*/api/v1/moderation/takedowns/5001/items', async ({ request }) => {
        calls.push({ path: 'items', body: await readJson(request) });
        detail = { ...detail, state: 'triage', items: [ITEM] };
        return HttpResponse.json(detail);
      }),
      http.post('*/api/v1/moderation/takedowns/5001/uphold', async ({ request }) => {
        calls.push({ path: 'uphold', body: await readJson(request) });
        detail = {
          ...detail,
          state: 'removed',
          items: [{ ...ITEM, state: 'disabled' }],
          strikes: [
            {
              person_id: '72',
              strikes: 3,
              contested: 1,
              counted: [
                {
                  id: '5001',
                  kind: 'copyright',
                  state: 'counter_noticed',
                  removed_at_ms: 1_700_000_100_000,
                  contested: true,
                },
                {
                  id: '4000',
                  kind: 'illegal_content',
                  state: 'removed',
                  removed_at_ms: 1_699_000_000_000,
                  contested: false,
                },
              ],
              window_months: 12,
              threshold: 3,
              review_case_id: '9200',
            },
          ],
        };
        return HttpResponse.json({ items_disabled: 1, notified: 1, review_case_id: '9200' });
      }),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Open takedown notice 5001' }));
    await screen.findByRole('heading', { name: 'Takedown notice TK-5001' });
    // Nothing to uphold before content is named.
    expect(screen.queryByRole('button', { name: 'Uphold and remove' })).toBeNull();
    const form = screen.getByRole('form', { name: 'Add content to the notice' });
    await user.type(within(form).getByLabelText('Id of the content'), '700');
    await user.click(within(form).getByRole('button', { name: 'Add' }));
    expect(await screen.findByText('Series Stolen Saga')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Uphold and remove' }));
    const dialog = await screen.findByRole('dialog', { name: 'Uphold this notice?' });
    await user.type(within(dialog).getByLabelText(/Why|reason/i), 'The claim is valid');
    await user.click(within(dialog).getByRole('button', { name: 'Uphold and remove' }));
    expect(
      await screen.findByText(/1 item removed; 1 person told\. A review case 9200 was opened\./),
    ).toBeInTheDocument();
    expect(calls).toEqual([
      { path: 'items', body: { kind: 'library_series', id: '700' } },
      { path: 'uphold', body: { reason: 'The claim is valid' } },
    ]);
    // The strike line and the next actions.
    expect(
      await screen.findByText('Person 72: 3 of 3 upheld takedowns in 12 months'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Record counter-notice' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Restore now' })).toBeInTheDocument();
    // D-333: the contested count and the notices counted, with the counter-notice marker.
    expect(screen.getByText('1 contested by a counter-notice')).toBeInTheDocument();
    const counted = screen.getByRole('list', { name: 'Takedowns counted' });
    expect(within(counted).getAllByRole('listitem')).toHaveLength(2);
    expect(within(counted).getByText('Counter-notice pending')).toBeInTheDocument();
    expect(
      screen.getByText(/Leave a review open while a counter-notice is pending/),
    ).toBeInTheDocument();
    // A viewer without MANAGE_CASES gets no link into the case.
    expect(screen.queryByRole('button', { name: 'Open review case' })).toBeNull();
  });

  it('records a counter-notice, shows the scheduled restore and lets staff note a court filing', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS', 'MANAGE_CASES'] });
    let detail: Record<string, unknown> = {
      ...DETAIL,
      state: 'removed',
      items: [{ ...ITEM, state: 'disabled' }],
      removed_at_ms: 1_700_000_100_000,
    };
    const calls: { path: string; body: unknown }[] = [];
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/takedowns', () =>
        HttpResponse.json([{ ...SUMMARY, state: 'removed', items: 1 }]),
      ),
      http.get('*/api/v1/moderation/takedowns/5001', () => HttpResponse.json(detail)),
      http.post('*/api/v1/moderation/takedowns/5001/counter-notice', async ({ request }) => {
        calls.push({ path: 'counter', body: await readJson(request) });
        detail = {
          ...detail,
          state: 'counter_noticed',
          counter_noticed_at_ms: 1_700_000_200_000,
          restore_after_ms: 1_701_000_000_000,
          counter_notice_text: 'It is mine',
        };
        return HttpResponse.json(detail);
      }),
      http.post('*/api/v1/moderation/takedowns/5001/court-filing', async ({ request }) => {
        calls.push({ path: 'court', body: await readJson(request) });
        detail = { ...detail, court_filing_noted: true };
        return HttpResponse.json(detail);
      }),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Takedowns' }));
    await user.click(await screen.findByRole('button', { name: 'Open takedown notice 5001' }));
    await screen.findByRole('heading', { name: 'Takedown notice TK-5001' });
    await user.click(screen.getByRole('button', { name: 'Record counter-notice' }));
    const dialog = await screen.findByRole('dialog', { name: 'Record a counter-notice' });
    await user.type(within(dialog).getByLabelText(/The counter-notice as received/), 'It is mine');
    await user.click(within(dialog).getByRole('button', { name: 'Record counter-notice' }));
    expect(
      await screen.findByText(
        /The content returns after .* \(ten business days\) unless you note a court filing\./,
      ),
    ).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Note court filing' }));
    const court = await screen.findByRole('dialog', { name: 'Note a court filing?' });
    await user.type(within(court).getByLabelText(/Why|reason/i), 'Filed on the 3rd');
    await user.click(within(court).getByRole('button', { name: 'Note court filing' }));
    expect(
      await screen.findByText(
        /The content stays down and the housekeeping job will not restore it\./,
      ),
    ).toBeInTheDocument();
    expect(calls).toEqual([
      { path: 'counter', body: { text: 'It is mine' } },
      { path: 'court', body: { noted: true, reason: 'Filed on the 3rd' } },
    ]);
  });

  it('rejects an incomplete notice and hands staff the templated reply', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    server.use(
      http.get('*/api/v1/moderation/takedowns', () => HttpResponse.json([SUMMARY])),
      http.get('*/api/v1/moderation/takedowns/5001', () => HttpResponse.json(DETAIL)),
      http.post('*/api/v1/moderation/takedowns/5001/reject', () =>
        HttpResponse.json({
          reply: 'Thank you for your copyright notice (TK-5001). Which chapter?',
        }),
      ),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Open takedown notice 5001' }));
    await screen.findByRole('heading', { name: 'Takedown notice TK-5001' });
    await user.click(screen.getByRole('button', { name: 'Reject as incomplete' }));
    const dialog = await screen.findByRole('dialog', { name: 'Reject as incomplete?' });
    await user.type(within(dialog).getByLabelText('What is missing'), 'Which chapter?');
    await user.click(within(dialog).getByRole('button', { name: 'Reject as incomplete' }));
    const reply = await screen.findByLabelText('Reply text');
    expect((reply as HTMLTextAreaElement).value).toContain('Which chapter?');
  });
});

describe('takedown cases (D-334) and held deletes (D-335)', () => {
  const CASE_BASE = {
    id: '9100',
    state: 'open',
    category: 'illegal_content',
    severity: 'serious',
    routing: 'platform',
    escalated: false,
    summary: 'Copyright takedown notice from Rights Holder Ltd',
    target_kind: 'library',
    target_id: '700',
    guild_id: null,
    assignee_id: null,
    report_count: 0,
    active_holds: 1,
    opened_at_ms: 1_700_000_000_000,
    updated_at_ms: 1_700_000_000_000,
    closed_at_ms: null,
  };

  it('badges a takedown case with its notice, opens the notice, and filters by source', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES', 'MANAGE_TAKEDOWNS'] });
    const queries: string[] = [];
    server.use(
      http.get('*/api/v1/moderation/cases', ({ request }) => {
        queries.push(new URL(request.url).search);
        return HttpResponse.json({
          items: [{ ...CASE_BASE, takedown: { notice_id: '5001', kind: 'copyright' } }],
          next_cursor: null,
        });
      }),
      http.get('*/api/v1/moderation/takedowns/5001', () => HttpResponse.json(DETAIL)),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    const badge = await screen.findByRole('button', { name: 'Open takedown notice 5001' });
    expect(badge).toHaveTextContent('Copyright notice TK-5001');
    await user.click(screen.getByRole('combobox', { name: 'Source' }));
    await user.click(await screen.findByRole('option', { name: 'Takedown notices' }));
    await screen.findByRole('button', { name: 'Open takedown notice 5001' });
    expect(queries.some((q) => q.includes('source=takedown'))).toBe(true);
    await user.click(screen.getByRole('button', { name: 'Open takedown notice 5001' }));
    expect(
      await screen.findByRole('heading', { name: 'Takedown notice TK-5001' }),
    ).toBeInTheDocument();
  });

  it('shows a plain badge for staff without MANAGE_TAKEDOWNS', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({
          items: [{ ...CASE_BASE, takedown: { notice_id: '5001', kind: 'illegal_content' } }],
          next_cursor: null,
        }),
      ),
    );
    openAdmin('moderation');
    expect(await screen.findByText('Illegal-content notice TK-5001')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Open takedown notice 5001' })).toBeNull();
  });

  it('links a refused delete to the takedown register', async () => {
    render(
      <ApiProblem error="This item is under a moderation hold (notice TK-77). Release the hold first." />,
    );
    const link = await screen.findByRole('link', { name: 'Open the takedown register' });
    expect(link).toHaveAttribute('href', '/admin/moderation');
  });
});
