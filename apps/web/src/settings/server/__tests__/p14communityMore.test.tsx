import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const community = {
  guild_id: 'g1',
  community_enabled: true,
  rules_channel_id: null,
  public_updates_channel_id: null,
  safety_alerts_channel_id: null,
  description: null,
  preferred_locale: 'en-US',
  onboarding_enabled: false,
  welcome_screen_enabled: false,
  guide_enabled: true,
  invites_paused: false,
  raid_alert_joins_per_minute: 0,
  age_restricted: false,
  screening_enabled: false,
  screening_description: null,
  screening_rules: [] as string[],
};

const openTab = async (name: string) => {
  server.use(http.get('*/api/v1/guilds/g1/sr/community', () => HttpResponse.json(community)));
  await openSettings('community');
  await userEvent.click(await screen.findByRole('tab', { name }));
};

describe('Community: Server Guide editor (LNG-SSET-066)', () => {
  it('saves the welcome message, to-dos and resource pages', async () => {
    seed({ perms: [MANAGE.guild] });
    const put = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/sr/guide', () =>
        HttpResponse.json({
          guild_id: 'g1',
          enabled: true,
          welcome_message: null,
          todos: [],
          resources: [],
        }),
      ),
      http.put('*/api/v1/guilds/g1/sr/guide', async ({ request }) => {
        await put.record(request);
        return HttpResponse.json({
          guild_id: 'g1',
          enabled: true,
          welcome_message: 'Hi',
          todos: [],
          resources: [],
        });
      }),
    );
    await openTab('Server Guide');
    await userEvent.type(await screen.findByLabelText('Welcome message'), 'Hi');
    await userEvent.click(screen.getByRole('button', { name: 'Add a to-do' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(await screen.findByText('Give every to-do and every resource a title.')).toBeVisible();
    expect(put.calls).toHaveLength(0);
    await userEvent.type(screen.getByLabelText('To-do 1 title'), 'Say hello');
    await userEvent.selectOptions(screen.getByLabelText('To-do 1 channel'), 'c1');
    await userEvent.click(screen.getByRole('button', { name: 'Add a resource page' }));
    await userEvent.type(screen.getByLabelText('Resource 1 title'), 'Rules');
    await userEvent.type(screen.getByLabelText('Resource 1 text (Markdown)'), 'Be kind');
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(put.calls).toHaveLength(1));
    expect(put.calls[0]!.body).toEqual({
      welcome_message: 'Hi',
      todos: [{ title: 'Say hello', description: null, channel_id: 'c1' }],
      resources: [{ title: 'Rules', content: 'Be kind' }],
    });
  });
});

describe('Community: rules screening (LNG-SSET-041)', () => {
  it('needs a rule before screening can be switched on, then saves the rules', async () => {
    seed({ perms: [MANAGE.guild] });
    const patch = capture();
    server.use(
      http.patch('*/api/v1/guilds/g1/sr/community', async ({ request }) => {
        await patch.record(request);
        return HttpResponse.json({ ...community, screening_enabled: true });
      }),
    );
    await openTab('Rules Screening');
    await userEvent.click(
      await screen.findByRole('switch', { name: /Require new members to agree/ }),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    expect(
      await screen.findByText('Add at least one rule, and fill in every rule you add.'),
    ).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Add a rule' }));
    await userEvent.type(screen.getByLabelText('Rule 1'), ' Be kind ');
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(patch.calls).toHaveLength(1));
    expect(patch.calls[0]!.body).toEqual({
      screening_enabled: true,
      screening_description: null,
      screening_rules: ['Be kind'],
    });
  });
});

describe('Community: access and applications (LNG-SSET-039, LNG-SSET-042)', () => {
  const form = {
    guild_id: 'g1',
    enabled: true,
    description: null,
    questions: [{ id: '11', label: 'Why join?', kind: 'short', required: true }],
  };
  const app = (id: string, over: object = {}) => ({
    id,
    guild_id: 'g1',
    user: { id: '2', username: 'bob' },
    answers: [{ question_id: '11', value: 'I love books' }],
    status: 'pending',
    created_at: '2026-02-01T00:00:00Z',
    reviewed_by: null,
    reviewed_at: null,
    rejection_reason: null,
    ...over,
  });

  it('shows how people get in and approves an application', async () => {
    seed({ perms: [MANAGE.guild] });
    const decision = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/sr/applications/form', () => HttpResponse.json(form)),
      http.get('*/api/v1/guilds/g1/sr/applications', () => HttpResponse.json([app('77')])),
      http.get('*/api/v1/guilds/g1/directory', () =>
        HttpResponse.json({ message: 'Unknown', code: 10004 }, { status: 404 }),
      ),
      http.post('*/api/v1/guilds/g1/sr/applications/77/decision', async ({ request }) => {
        await decision.record(request);
        return HttpResponse.json(app('77', { status: 'approved' }));
      }),
    );
    await openTab('Access');
    expect(
      await screen.findByText('People can apply to join; you review the applications below.'),
    ).toBeVisible();
    expect(await screen.findByText('The server is not listed in the Directory.')).toBeVisible();
    const row = await screen.findByRole('listitem', { name: 'bob' });
    expect(within(row).getByText('Why join?')).toBeVisible();
    expect(within(row).getByText('I love books')).toBeVisible();
    await userEvent.click(within(row).getByRole('button', { name: 'Approve bob' }));
    await waitFor(() => expect(decision.calls).toHaveLength(1));
    expect(decision.calls[0]!.body).toEqual({ approve: true });
    await waitFor(() =>
      expect(within(row).queryByRole('button', { name: 'Approve bob' })).toBeNull(),
    );
    expect(within(row).getByText('Approved')).toBeVisible();
  });

  it('rejects with a reason and saves the application form', async () => {
    seed({ perms: [MANAGE.guild] });
    const decision = capture();
    const put = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/sr/applications/form', () =>
        HttpResponse.json({ ...form, enabled: false, questions: [] }),
      ),
      http.get('*/api/v1/guilds/g1/sr/applications', () => HttpResponse.json([app('78')])),
      http.get('*/api/v1/guilds/g1/directory', () =>
        HttpResponse.json({ message: 'Unknown', code: 10004 }, { status: 404 }),
      ),
      http.post('*/api/v1/guilds/g1/sr/applications/78/decision', async ({ request }) => {
        await decision.record(request);
        return HttpResponse.json(app('78', { status: 'rejected', rejection_reason: 'Not a fit' }));
      }),
      http.put('*/api/v1/guilds/g1/sr/applications/form', async ({ request }) => {
        await put.record(request);
        return HttpResponse.json(form);
      }),
    );
    await openTab('Access');
    await userEvent.type(
      await screen.findByLabelText('Reason for rejecting bob (optional)'),
      'Not a fit',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Reject bob' }));
    await waitFor(() => expect(decision.calls).toHaveLength(1));
    expect(decision.calls[0]!.body).toEqual({ approve: false, reason: 'Not a fit' });
    expect(await screen.findByText('Rejected')).toBeVisible();

    await userEvent.click(screen.getByRole('switch', { name: /Let people apply to join/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Add a question' }));
    await userEvent.type(screen.getByLabelText('Question 1 text'), 'Why join?');
    await userEvent.click(screen.getByRole('switch', { name: 'Question 1 is required' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(put.calls).toHaveLength(1));
    expect(put.calls[0]!.body).toEqual({
      enabled: true,
      description: null,
      questions: [{ label: 'Why join?', kind: 'short', required: true }],
    });
  });
});

describe('Community: insights (LNG-SSET-068)', () => {
  it('totals the days and names the top channels', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(
      http.get('*/api/v1/guilds/g1/sr/insights', () =>
        HttpResponse.json({
          days: [
            { date: '2026-03-01', members: 10, joins: 1, messages: 5, voice_minutes: 0 },
            { date: '2026-03-02', members: 13, joins: 3, messages: 7, voice_minutes: 0 },
          ],
          top_channels: [{ channel_id: 'c1', messages: 12 }],
        }),
      ),
    );
    await openTab('Insights');
    const totals = await screen.findByRole('list', { name: 'Totals for the period' });
    expect(within(totals).getByText(/13 members/)).toBeVisible();
    expect(within(totals).getByText('4 joins')).toBeVisible();
    expect(within(totals).getByText('12 messages')).toBeVisible();
    expect(screen.getByText('#general: 12 messages')).toBeVisible();
    expect(screen.getByRole('table')).toBeVisible();
  });

  it('says so when the daily job has not run yet', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(
      http.get('*/api/v1/guilds/g1/sr/insights', () =>
        HttpResponse.json({ days: [], top_channels: [] }),
      ),
    );
    await openTab('Insights');
    expect(await screen.findByText(/No data yet/)).toBeVisible();
  });
});

describe('Community: Directory listing request (LNG-SSET-069)', () => {
  it('requests a listing, shows the staff note and withdraws', async () => {
    seed({ perms: [MANAGE.guild] });
    const put = capture();
    const del = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/directory', () =>
        HttpResponse.json({ message: 'Unknown', code: 10004 }, { status: 404 }),
      ),
      http.get('*/api/v1/directory/categories', () => HttpResponse.json(['books', 'gaming'])),
      http.put('*/api/v1/guilds/g1/directory', async ({ request }) => {
        await put.record(request);
        return HttpResponse.json({
          guild_id: 'g1',
          status: 'rejected',
          category: 'gaming',
          description: 'Games',
          featured: false,
          requested_at: '2026-03-01T00:00:00Z',
          decided_at: '2026-03-02T00:00:00Z',
          decision_note: 'Add more channels',
        });
      }),
      http.delete('*/api/v1/guilds/g1/directory', async ({ request }) => {
        await del.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openTab('Directory');
    await userEvent.selectOptions(await screen.findByLabelText('Category'), 'gaming');
    await userEvent.type(screen.getByLabelText('Directory description'), 'Games');
    await userEvent.click(screen.getByRole('button', { name: 'Request a listing' }));
    await waitFor(() => expect(put.calls).toHaveLength(1));
    expect(put.calls[0]!.body).toEqual({ category: 'gaming', description: 'Games' });
    expect(await screen.findByText('Add more channels')).toBeVisible();
    expect(screen.getByText('Rejected')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Withdraw' }));
    await waitFor(() => expect(del.calls).toHaveLength(1));
    expect(await screen.findByRole('button', { name: 'Request a listing' })).toBeVisible();
  });
});
