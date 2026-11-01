// MOD-GRD-001: Admin > Moderation > Subtree cases (the case view, the node decisions, the two-person requests and
// the Operator's branch action) and the member's own "Your account review" and transparency cards.
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { AccountReviewCard, TransparencyCard } from '../../settings/user/sections/AccountReview';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const SLOW = { timeout: 10_000 };

const summary = {
  id: '900',
  root_account_id: '30',
  root_username: 'banned_one',
  category: 'fraud',
  state: 'open',
  opened_at_ms: 1_700_000_000_000,
  nodes: 4,
  auto_frozen: 1,
  waiting_review: 2,
};

const node = (over: Record<string, unknown>) => ({
  account_id: '1',
  username: 'node',
  role: 'descendant',
  depth: 1,
  parent_account_id: '30',
  registered_at_ms: 1_690_000_000_000,
  messages: 0,
  voice_seconds: 0,
  unactivated: false,
  status: 'review',
  restricted: false,
  note: '',
  ...over,
});

const detail = (requests: unknown[] = []) => ({
  case: summary,
  opened_by: '5',
  nodes_truncated: false,
  nodes: [
    node({ account_id: '30', username: 'banned_one', role: 'root', depth: 0, status: 'banned' }),
    node({
      account_id: '31',
      username: 'idle_one',
      unactivated: true,
      status: 'auto_frozen',
      restricted: true,
    }),
    node({ account_id: '32', username: 'busy_one', messages: 120, voice_seconds: 40 }),
    node({ account_id: '20', username: 'the_inviter', role: 'inviter', depth: -1 }),
  ],
  requests,
});

function serve(requests: unknown[] = []) {
  server.use(
    http.get('*/api/v1/admin/moderation/subtree-cases', () => HttpResponse.json([summary])),
    http.get('*/api/v1/admin/moderation/subtree-cases/900', () =>
      HttpResponse.json(detail(requests)),
    ),
    http.get('*/api/v1/admin/moderation/requests', () => HttpResponse.json(requests)),
    http.get('*/api/v1/admin/moderation/restriction-appeals', () => HttpResponse.json([])),
    // The queue behind the other tabs.
    http.get('*/api/v1/moderation/cases', () => HttpResponse.json({ items: [], next_after: null })),
    http.get('*/api/v1/moderation/dashboard', () => HttpResponse.json({})),
  );
}

async function openCase() {
  asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
  serve();
  openAdmin('moderation');
  await userEvent.click(await screen.findByRole('tab', { name: 'Subtree cases' }, SLOW));
  await userEvent.click((await screen.findAllByRole('button', { name: 'Open' }, SLOW))[0]!);
}

describe('Admin > Moderation > Subtree cases (MOD-GRD-001)', () => {
  it('lists the case with its category and counts, then every node with stats and lineage', async () => {
    await openCase();
    const table = await screen.findByRole('table', undefined, SLOW);
    const rows = within(table).getAllByRole('row');
    expect(rows).toHaveLength(5);
    expect(within(table).getByText('@idle_one')).toBeInTheDocument();
    expect(within(table).getByText('Frozen automatically')).toBeInTheDocument();
    expect(within(table).getAllByText('Waiting for review').length).toBe(2);
    expect(within(table).getByText('Invited the banned account')).toBeInTheDocument();
    expect(table).toHaveTextContent('120 messages · 40 voice seconds');
    expect(table).toHaveTextContent('unactivated');
    // The banned root has no buttons; the inviter is only listed with the decisions (no automatic penalty).
    expect(within(rows[1]!).queryByRole('button')).toBeNull();
    expect(
      screen.getByText(/Only accounts that are unactivated .* are frozen by the case/),
    ).toBeInTheDocument();
  });

  it('clears an automatically frozen node with a written reason', async () => {
    await openCase();
    let sent: unknown;
    server.use(
      http.post(
        '*/api/v1/admin/moderation/subtree-cases/900/nodes/31/clear',
        async ({ request }) => {
          sent = await readJson(request);
          return HttpResponse.json({ cleared: true });
        },
      ),
    );
    const row = (await screen.findByText('@idle_one', undefined, SLOW)).closest('tr')!;
    await userEvent.click(within(row).getByRole('button', { name: 'Clear' }));
    const dialog = await screen.findByRole('dialog', { name: 'Clear this account' });
    await userEvent.type(within(dialog).getByRole('textbox'), 'verified by the owner');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm' }));
    expect(await screen.findByText('Cleared.', undefined, SLOW)).toBeInTheDocument();
    expect(sent).toEqual({ reason: 'verified by the owner' });
  });

  it('files a sterilization request that says a second staff member must approve it', async () => {
    await openCase();
    let sent: unknown;
    server.use(
      http.post('*/api/v1/admin/moderation/subtree-cases/900/requests', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({ id: '1' });
      }),
    );
    const row = (await screen.findByText('@the_inviter', undefined, SLOW)).closest('tr')!;
    await userEvent.click(within(row).getByRole('button', { name: 'Stop inviting' }));
    const dialog = await screen.findByRole('dialog', {
      name: 'Stop this account inviting for 12 months',
    });
    expect(dialog).toHaveTextContent('until a second staff member approves it');
    expect(dialog).toHaveTextContent('second factor');
    await userEvent.type(
      within(dialog).getByRole('textbox'),
      'Invited the account that ran the ring and ignored two reports.',
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm' }));
    expect(
      await screen.findByText(/A second staff member has to approve it/, undefined, SLOW),
    ).toBeInTheDocument();
    expect(sent).toMatchObject({ kind: 'sterilize', account_id: '20' });
  });

  it('shows the branch action to the Operator only, and runs an approved request', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'], owner: true });
    const approved = {
      id: '77',
      kind: 'branch_action',
      case_id: '900',
      target_account_id: null,
      action: 'freeze',
      reason: 'One script made the whole branch.',
      proposed_by: '1',
      approved_by: '5',
      state: 'approved',
      created_at_ms: 1_700_000_000_000,
      nodes_actioned: null,
    };
    serve([approved]);
    let ran: unknown;
    server.use(
      http.post('*/api/v1/admin/moderation/branch-action', async ({ request }) => {
        ran = await readJson(request);
        return HttpResponse.json({
          request_id: '77',
          case_id: '900',
          action: 'freeze',
          nodes_actioned: 3,
          already_banned_or_gone: 0,
          notice: { body: '3 accounts actioned, 3 frozen pending review.' },
        });
      }),
    );
    openAdmin('moderation');
    await userEvent.click(await screen.findByRole('tab', { name: 'Subtree cases' }, SLOW));
    await userEvent.click((await screen.findAllByRole('button', { name: 'Open' }, SLOW))[0]!);
    expect(
      await screen.findByRole('button', { name: 'Branch action (Operator)' }, SLOW),
    ).toBeInTheDocument();
    await userEvent.click(await screen.findByRole('button', { name: 'Run the branch action' }));
    expect(
      await screen.findByText(/Branch action done: 3 accounts/, undefined, SLOW),
    ).toBeInTheDocument();
    expect(ran).toEqual({ request_id: '77' });
  });

  it('does not show the branch action to a staff member who is not the Owner', async () => {
    await openCase();
    await screen.findByText('@idle_one', undefined, SLOW);
    expect(screen.queryByRole('button', { name: 'Branch action (Operator)' })).toBeNull();
  });
});

describe('Your account review and transparency (MOD-GRD-001)', () => {
  it('shows a restriction in plain words and lets the member appeal it once', async () => {
    let appeal: unknown;
    server.use(
      http.get('*/api/v1/sr/moderation/restrictions', () =>
        HttpResponse.json([
          {
            id: '5',
            kind: 'case_freeze',
            effect:
              'Your account is frozen while it is reviewed: you cannot post, invite or join voice. You can still sign in and appeal.',
            reason: 'Your account is under review.',
            ends_at_ms: 1_800_000_000_000,
            state: 'active',
            can_appeal: true,
            appeal_state: null,
          },
        ]),
      ),
      http.post('*/api/v1/sr/moderation/restrictions/5/appeal', async ({ request }) => {
        appeal = await readJson(request);
        return HttpResponse.json({});
      }),
    );
    render(<AccountReviewCard />);
    const card = await screen.findByRole('region', { name: 'Your account review' }, SLOW);
    expect(card).toHaveTextContent('Your account is frozen while it is reviewed');
    expect(card).toHaveTextContent('Your account is under review.');
    await userEvent.click(within(card).getByRole('button', { name: 'Appeal' }));
    await userEvent.type(within(card).getByRole('textbox'), 'I was away for a month');
    await userEvent.click(within(card).getByRole('button', { name: 'Send appeal' }));
    expect(await screen.findByText('Your appeal was sent.', undefined, SLOW)).toBeInTheDocument();
    expect(appeal).toEqual({ message: 'I was away for a month' });
  });

  it('shows nothing when no restriction is in force', async () => {
    server.use(http.get('*/api/v1/sr/moderation/restrictions', () => HttpResponse.json([])));
    const { container } = render(<AccountReviewCard />);
    await new Promise((r) => setTimeout(r, 50));
    expect(container).toBeEmptyDOMElement();
  });

  it('lists the monthly counts and nothing else', async () => {
    server.use(
      http.get('*/api/v1/moderation/transparency', () =>
        HttpResponse.json({
          months: [
            {
              month: '2026-11',
              serious_bans: 2,
              branch_actions: 1,
              accounts_actioned: 5,
              accounts_frozen: 7,
              sterilizations: 1,
              appeals_filed: 3,
              appeals_overturned: 1,
              by_category: [{ category: 'Fraud', notices: 3 }],
            },
          ],
        }),
      ),
    );
    render(<TransparencyCard />);
    const card = await screen.findByRole('region', { name: 'Moderation transparency' }, SLOW);
    expect(card).toHaveTextContent('2026-11');
    expect(card).toHaveTextContent(
      '2 serious bans, 1 branch action, 5 accounts actioned, 7 frozen',
    );
    expect(card).toHaveTextContent('Fraud: 3');
  });
});
