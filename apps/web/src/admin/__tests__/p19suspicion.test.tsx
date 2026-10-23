// MOD-SUS-001/003/004: the suspicion panel on the Person page and the Signals tab of Admin > Moderation.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import {
  account,
  asStaff,
  detail,
  http,
  HttpResponse,
  openAdmin,
  readJson,
  server,
} from './helpers';

const SLOW = { timeout: 10_000 };

const signal = (over: Record<string, unknown> = {}) => ({
  id: '800',
  person_id: '500',
  account: { id: '100', username: 'alice' },
  related_person_id: '501',
  related_account: { id: '101', username: 'mallory' },
  kind: 'shared_login_ip',
  detail: { shared_ips: 2, window_days: 180 },
  state: 'open',
  created_at_ms: 1_700_000_000_000,
  reviewed_by: null,
  reviewed_at_ms: null,
  review_reason: null,
  ...over,
});

const panel = (over: Record<string, unknown> = {}) => ({
  person_id: '500',
  level: 2,
  freeze_threshold: 2,
  escalate_threshold: 3,
  invites_paused: true,
  accounts: [{ id: '100', username: 'alice' }],
  events: [
    {
      id: '2',
      delta: -2,
      level_after: 0,
      kind: 'cleared',
      reason: 'cleared by staff: checked with the member',
      source_account: null,
      actor_id: '7',
      case_id: null,
      created_at_ms: 1_700_000_200_000,
    },
    {
      id: '1',
      delta: 2,
      level_after: 2,
      kind: 'raised',
      reason: 'serious ban of an invitee',
      source_account: { id: '9', username: 'mallory' },
      actor_id: '7',
      case_id: '9100',
      created_at_ms: 1_700_000_000_000,
    },
  ],
  banned_descendants: [
    {
      account: { id: '9', username: 'mallory' },
      person_id: '509',
      depth: 1,
      ban_category: 'serious',
      reason_category: 'fraud',
      banned_at_ms: 1_700_000_000_000,
      lifted: false,
      case_id: '9099',
    },
  ],
  invite_paths: [
    {
      account_id: '100',
      nodes: [
        { id: '1', username: 'root' },
        { id: '100', username: 'alice' },
      ],
    },
  ],
  cases: [
    {
      id: '9100',
      state: 'open',
      auto_kind: 'suspicion_freeze',
      assignee_id: '12',
      linked_case_id: '9099',
      opened_at_ms: 1_700_000_000_000,
    },
  ],
  signals: [signal()],
  ...over,
});

async function openPerson(perms: Parameters<typeof asStaff>[0]['perms'], over = {}) {
  asStaff({ perms });
  server.use(
    http.get('*/api/v1/admin/accounts', () =>
      HttpResponse.json({ items: [account()], next_after: null }),
    ),
    http.get('*/api/v1/admin/accounts/100', () => HttpResponse.json(detail())),
    http.get('*/api/v1/admin/persons/500', () =>
      HttpResponse.json({
        id: '500',
        accounts: [account()],
        ban: null,
        notes: '',
        suspicion_level: 2,
      }),
    ),
    http.get('*/api/v1/moderation/persons/500/suspicion', () => HttpResponse.json(panel(over))),
  );
  openAdmin('accounts');
  const user = userEvent.setup();
  await user.click(await screen.findByRole('button', { name: 'View alice' }, SLOW));
  await screen.findByText('Active sessions', {}, SLOW);
  await user.click(screen.getByRole('button', { name: 'Open person record' }));
  await screen.findByRole('list', { name: 'Account chain' }, SLOW);
  return user;
}

describe('suspicion panel on the Person page (MOD-SUS-001, -004)', () => {
  it('shows the history with the clear and its reason, banned descendants, the path and the case', async () => {
    // The server withholds the signals (null) when the viewer lacks the bit; the page then shows no section.
    await openPerson(['STAFF_BASIC', 'MANAGE_ACCOUNTS', 'MANAGE_INVITES'], { signals: null });
    const heading = await screen.findByRole('heading', { name: 'Inviter suspicion' }, SLOW);
    expect(heading).toBeInTheDocument();
    expect(screen.getByText('Invite creation paused')).toBeInTheDocument();
    // The clear is shown with its reason; the event it follows is still there.
    expect(screen.getByText('Cleared, level 0')).toBeInTheDocument();
    expect(screen.getByText(/checked with the member/)).toBeInTheDocument();
    expect(screen.getByText(/\+2, level 2/)).toBeInTheDocument();
    // Banned descendants carry the ban reason and severity.
    const desc = screen.getByRole('list', { name: 'Banned accounts below this person' });
    expect(within(desc).getByText('@mallory')).toBeInTheDocument();
    expect(within(desc).getByText('Serious')).toBeInTheDocument();
    expect(within(desc).getByText(/Reason: fraud/)).toBeInTheDocument();
    // The invite path to the root.
    expect(screen.getByText('@root → @alice')).toBeInTheDocument();
    // The automatic case.
    expect(screen.getByText('Case 9100: invite creation paused')).toBeInTheDocument();
    // Evasion signals are IP-derived: the server sent none, so the section is absent.
    expect(screen.queryByRole('heading', { name: 'Evasion signals' })).not.toBeInTheDocument();
  });

  it('shows signals to staff who may see sign-in data, reads addresses on request and reviews with a reason', async () => {
    let review: unknown;
    server.use(
      http.get('*/api/v1/moderation/evasion-signals/800', () =>
        HttpResponse.json({
          signal: signal(),
          window_days: 180,
          shared_addresses: [
            {
              ip: '203.0.113.7',
              person_last_seen_ms: 1_700_000_000_000,
              related_last_seen_ms: 1_699_000_000_000,
              persons: 2,
            },
          ],
        }),
      ),
      http.post('*/api/v1/moderation/evasion-signals/800/review', async ({ request }) => {
        review = await readJson(request);
        return HttpResponse.json(signal({ state: 'dismissed', review_reason: 'roommates' }));
      }),
    );
    const user = await openPerson(['STAFF_BASIC', 'MANAGE_INVITES', 'MANAGE_ACCOUNTS']);
    await screen.findByRole('heading', { name: 'Evasion signals' }, SLOW);
    // The signal itself carries no address.
    expect(screen.queryByText('203.0.113.7')).not.toBeInTheDocument();
    expect(screen.getByText('2 shared addresses', { exact: false })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Show addresses' }));
    expect(await screen.findByText('203.0.113.7', { exact: false })).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Dismiss' }));
    const dialog = await screen.findByRole('dialog', { name: 'Dismiss this signal' });
    const confirm = within(dialog).getByRole('button', { name: 'Dismiss' });
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByLabelText(/Reason/), 'roommates');
    await user.click(confirm);
    await waitFor(() => expect(review).toEqual({ decision: 'dismissed', reason: 'roommates' }));
  });
});

describe('Signals tab (MOD-SUS-003)', () => {
  it('lists the open signals for staff with MANAGE_ACCOUNTS', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES', 'MANAGE_ACCOUNTS'] });
    const states: string[] = [];
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/holds', () => HttpResponse.json([])),
      http.get('*/api/v1/moderation/appeals', () => HttpResponse.json([])),
      http.get('*/api/v1/moderation/evasion-signals', ({ request }) => {
        states.push(new URL(request.url).searchParams.get('state') ?? '');
        return HttpResponse.json([
          signal(),
          signal({
            id: '801',
            kind: 'shared_invite_lineage',
            detail: { relation: 'invited_by_banned' },
          }),
        ]);
      }),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Signals' }, SLOW));
    const list = await screen.findByRole('list', { name: 'Evasion signals' }, SLOW);
    expect(within(list).getAllByRole('listitem').length).toBeGreaterThanOrEqual(2);
    expect(
      within(list).getByText('Invited by a banned person', { exact: false }),
    ).toBeInTheDocument();
    expect(states[0]).toBe('open');
  });

  it('is absent without MANAGE_ACCOUNTS', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES'] });
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
    );
    openAdmin('moderation');
    await screen.findByRole('tab', { name: /Cases/ }, SLOW);
    expect(screen.queryByRole('tab', { name: 'Signals' })).not.toBeInTheDocument();
  });
});

describe('follow-ups P-13 to P-16', () => {
  it('P-16: a MANAGE_ACCOUNTS-only viewer reaches Moderation and sees only the signals', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_ACCOUNTS'] });
    server.use(
      http.get('*/api/v1/moderation/evasion-signals', () => HttpResponse.json([signal()])),
    );
    openAdmin('moderation');
    await screen.findByRole('list', { name: 'Evasion signals' }, SLOW);
    expect(screen.queryByRole('tab')).not.toBeInTheDocument();
  });

  it('P-14: opens a case prefilled from the signal, with no address in the note, then links it', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES', 'MANAGE_ACCOUNTS'] });
    const calls: string[] = [];
    let note: unknown;
    let link: unknown;
    let opened: Record<string, unknown> = {};
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/holds', () => HttpResponse.json([])),
      http.get('*/api/v1/moderation/appeals', () => HttpResponse.json([])),
      http.get('*/api/v1/moderation/evasion-signals', () => HttpResponse.json([signal()])),
      http.post('*/api/v1/moderation/cases', async ({ request }) => {
        calls.push('open');
        opened = (await readJson(request)) as Record<string, unknown>;
        return HttpResponse.json({ id: '9300' }, { status: 201 });
      }),
      http.post('*/api/v1/moderation/cases/9300/notes', async ({ request }) => {
        calls.push('note');
        note = await readJson(request);
        return HttpResponse.json({ id: '1' }, { status: 201 });
      }),
      http.post('*/api/v1/moderation/evasion-signals/800/link-case', async ({ request }) => {
        calls.push('link');
        link = await readJson(request);
        return HttpResponse.json(signal({ case_id: '9300' }));
      }),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Signals' }, SLOW));
    await user.click(await screen.findByRole('button', { name: 'Open case' }, SLOW));
    const dialog = await screen.findByRole('dialog', { name: 'Open a case' });
    expect(within(dialog).getByLabelText('Account id of the person')).toHaveValue('100');
    const first = within(dialog).getByLabelText('First note') as HTMLTextAreaElement;
    expect(first.value).toBe('Evasion signal 800: shared sign-in address');
    expect(first.value).not.toMatch(/\d+\.\d+\.\d+\.\d+/);
    await user.click(within(dialog).getByRole('button', { name: 'Open case' }));
    await waitFor(() => expect(calls).toEqual(['open', 'note', 'link']));
    expect(opened).toMatchObject({ category: 'ban_evasion', subject_account_id: '100' });
    expect(note).toEqual({ body: 'Evasion signal 800: shared sign-in address' });
    expect(link).toEqual({ case_id: '9300' });
  });

  it('P-14: a signal that already has a case shows it instead of the button', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_CASES', 'MANAGE_ACCOUNTS'] });
    server.use(
      http.get('*/api/v1/moderation/cases', () =>
        HttpResponse.json({ items: [], next_cursor: null }),
      ),
      http.get('*/api/v1/moderation/holds', () => HttpResponse.json([])),
      http.get('*/api/v1/moderation/appeals', () => HttpResponse.json([])),
      http.get('*/api/v1/moderation/evasion-signals', () =>
        HttpResponse.json([signal({ case_id: '9300' })]),
      ),
    );
    openAdmin('moderation');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('tab', { name: 'Signals' }, SLOW));
    expect(await screen.findByText('Case 9300', {}, SLOW)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Open case' })).not.toBeInTheDocument();
  });

  it('P-15: the Review hub shows open signals to MANAGE_ACCOUNTS holders only', async () => {
    let asked = 0;
    server.use(
      http.get('*/api/v1/admin/developers/packages', () => HttpResponse.json([])),
      http.get('*/api/v1/moderation/evasion-signals/count', () => {
        asked += 1;
        return HttpResponse.json({ open: 3 });
      }),
    );
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_PACKAGES', 'MANAGE_ACCOUNTS'] });
    openAdmin('review');
    const link = await screen.findByRole('link', { name: /3 open evasion signals/ }, SLOW);
    expect(link).toHaveAttribute('href', '/admin/moderation?tab=signals');
    expect(asked).toBe(1);
  });

  it('P-15: without MANAGE_ACCOUNTS the hub never asks', async () => {
    let asked = 0;
    server.use(
      http.get('*/api/v1/admin/developers/packages', () => HttpResponse.json([])),
      http.get('*/api/v1/moderation/evasion-signals/count', () => {
        asked += 1;
        return HttpResponse.json({ open: 3 });
      }),
    );
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_PACKAGES'] });
    openAdmin('review');
    await screen.findByRole('tab', { name: 'Packages' }, SLOW);
    expect(asked).toBe(0);
    expect(screen.queryByText(/open evasion signal/)).not.toBeInTheDocument();
  });

  it('P-13: the panel lists a refused re-registration under the banned person', async () => {
    await openPerson(['STAFF_BASIC', 'MANAGE_ACCOUNTS', 'MANAGE_INVITES'], {
      signals: null,
      events: [
        {
          id: '3',
          delta: 0,
          level_after: 0,
          kind: 'reregistration_refused',
          reason: 'registration refused: matched by discord; invite from person 9',
          source_account: null,
          actor_id: null,
          case_id: null,
          created_at_ms: 1_700_000_300_000,
        },
      ],
    });
    expect(
      await screen.findByText('A registration by this banned person was refused'),
    ).toBeInTheDocument();
    expect(screen.getByText(/matched by discord/)).toBeInTheDocument();
  });
});

describe('Person links (D-349 to D-351, P-20, P-21)', () => {
  it('marks a folded signal and keeps a demoted automatic case listed with its survivor', async () => {
    await openPerson(['STAFF_BASIC', 'MANAGE_ACCOUNTS', 'MANAGE_INVITES'], {
      cases: [
        {
          id: '9200',
          state: 'open',
          auto_kind: 'suspicion_escalation',
          assignee_id: null,
          linked_case_id: null,
          opened_at_ms: 1_700_000_100_000,
          folded_into_case_id: null,
        },
        {
          id: '9100',
          state: 'open',
          auto_kind: null,
          assignee_id: null,
          linked_case_id: '9200',
          opened_at_ms: 1_700_000_000_000,
          folded_into_case_id: '9200',
        },
      ],
      signals: [
        signal({
          related_person_id: '500',
          related_account: { id: '100', username: 'alice' },
          state: 'confirmed',
          reviewed_at_ms: 1_700_000_500_000,
          review_reason: 'folded by a Person link: the two Persons were merged',
          folded_from_person_id: '777',
        }),
      ],
    });
    const cases = await screen.findByRole('list', { name: 'Cases opened by the system' }, SLOW);
    expect(within(cases).getByText('Case 9200: escalated for senior review')).toBeInTheDocument();
    expect(within(cases).getByText('Case 9100, from a merged Person:')).toBeInTheDocument();
    expect(within(cases).getByRole('link', { name: 'folded into case 9200' })).toHaveAttribute(
      'href',
      '/admin/moderation?case=9200',
    );
    expect(await screen.findByText('Folded (Person 777)')).toBeInTheDocument();
  });
});
