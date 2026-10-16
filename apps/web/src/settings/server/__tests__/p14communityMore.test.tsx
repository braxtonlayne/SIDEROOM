import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { render } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../../router';
import {
  baseHandlers,
  capture,
  http,
  HttpResponse,
  MANAGE,
  openSettings,
  seed,
  server,
} from './helpers';

/** Like openSettings, but the test's own handlers (set up after the base ones) win. */
async function openWith(section: string, setup: () => void) {
  baseHandlers();
  setup();
  const router = createAppRouter(
    createMemoryHistory({ initialEntries: [`/settings/server/g1/${section}`] }),
  );
  render(<RouterProvider router={router} />);
  await waitFor(() => expect(screen.getByRole('dialog')).toBeInTheDocument());
}

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

describe('Member details: audit history (LNG-SSET-030)', () => {
  it('lists the audit entries about the member when the viewer may read the log', async () => {
    seed({ perms: [MANAGE.kick, MANAGE.audit] });
    const urls: string[] = [];
    server.use(
      http.get('*/api/v1/guilds/g1/audit-logs', ({ request }) => {
        urls.push(new URL(request.url).search);
        return HttpResponse.json({
          audit_log_entries: [
            { id: '5', user_id: '3', target_id: '2', action_type: 22, changes: [], reason: 'spam' },
          ],
          users: [
            { id: '2', username: 'bob' },
            { id: '3', username: 'carol' },
          ],
        });
      }),
    );
    await openSettings('members');
    await userEvent.click(await screen.findByRole('button', { name: 'Details for bob' }));
    const dialog = await screen.findByRole('dialog', { name: 'bob' });
    expect(await within(dialog).findByText(/carol .*bob/)).toBeVisible();
    expect(within(dialog).getByText('(spam)')).toBeVisible();
    expect(urls[0]).toContain('target_id=2');
  });

  it('hides the history from a viewer who cannot read the audit log', async () => {
    seed({ perms: [MANAGE.kick] });
    await openSettings('members');
    await userEvent.click(await screen.findByRole('button', { name: 'Details for bob' }));
    const dialog = await screen.findByRole('dialog', { name: 'bob' });
    expect(within(dialog).queryByText('Audit history')).toBeNull();
  });
});

describe('Features search (LNG-SSET-049)', () => {
  it('narrows the modules by name or description', async () => {
    seed({ perms: [MANAGE.guild] });
    const mod = (id: string, name: string, description: string) => ({
      id,
      name,
      description,
      section: 'Lounge',
      phase: 11,
      enabled: false,
      default_enabled: false,
      platform_disabled: false,
      settings: {},
      schema: { fields: [] },
    });
    server.use(
      http.get('*/api/v1/guilds/g1/sr/features', () =>
        HttpResponse.json([
          mod('lounge.starboard', 'Starboard', 'Repost popular messages.'),
          mod('lounge.sticky', 'Sticky messages', 'Keep a message at the bottom.'),
        ]),
      ),
    );
    await openSettings('features');
    await userEvent.type(await screen.findByLabelText('Search features'), 'bottom');
    expect(screen.queryByText('Starboard')).toBeNull();
    expect(screen.getByText('Sticky messages')).toBeVisible();
    await userEvent.clear(screen.getByLabelText('Search features'));
    await userEvent.type(screen.getByLabelText('Search features'), 'zzz');
    expect(await screen.findByText('No feature matches your search.')).toBeVisible();
  });
});

describe('Integrations overview and followed channels (LNG-SSET-044, LNG-SSET-046)', () => {
  const hook = (id: string, type: number, name: string) => ({
    id,
    type,
    guild_id: 'g1',
    channel_id: 'c1',
    name,
    avatar: null,
  });

  const integration = (id: string, name: string) => ({
    id,
    name,
    type: 'discord',
    enabled: true,
    account: { id, name },
    application: {
      id,
      name,
      icon: null,
      description: '',
      bot: { id: `${id}0`, username: name.toLowerCase(), bot: true },
    },
    user: { id: '3', username: 'carol' },
    scopes: ['bot', 'applications.commands'],
  });

  it('counts webhooks and followed channels, lists installed applications and removes one', async () => {
    seed({ perms: [MANAGE.webhooks, MANAGE.guild] });
    const remove = capture();
    const setup = () =>
      server.use(
        http.get('*/api/v1/guilds/g1/webhooks', () =>
          HttpResponse.json([hook('w1', 1, 'CI'), hook('w2', 2, 'News'), hook('w3', 2, 'Blog')]),
        ),
        http.get('*/api/v1/guilds/g1/integrations', () =>
          HttpResponse.json([integration('50', 'Helper')]),
        ),
        http.delete('*/api/v1/guilds/g1/integrations/50', async ({ request }) => {
          await remove.record(request);
          return new HttpResponse(null, { status: 204 });
        }),
      );
    await openWith('integrations', setup);
    const counts = await screen.findByRole('list', { name: 'Integration counts' });
    expect(within(counts).getByText('1 webhook')).toBeVisible();
    expect(within(counts).getByText('2 followed channels')).toBeVisible();
    expect(await screen.findByText('Helper')).toBeVisible();
    expect(screen.getByText('Added by carol')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Remove Helper' }));
    await userEvent.click(
      within(await screen.findByRole('dialog', { name: 'Remove Helper?' })).getByRole('button', {
        name: 'Remove',
      }),
    );
    await waitFor(() => expect(remove.calls).toHaveLength(1));
    await waitFor(() => expect(screen.queryByText('Helper')).toBeNull());
  });

  it('does not list applications for someone who cannot manage the server', async () => {
    seed({ perms: [MANAGE.webhooks] });
    const asked = vi.fn();
    await openWith('integrations', () =>
      server.use(
        http.get('*/api/v1/guilds/g1/webhooks', () => HttpResponse.json([])),
        http.get('*/api/v1/guilds/g1/integrations', () => {
          asked();
          return HttpResponse.json([]);
        }),
      ),
    );
    await screen.findByRole('list', { name: 'Integration counts' });
    expect(screen.queryByRole('heading', { name: 'Bots and apps' })).toBeNull();
    expect(asked).not.toHaveBeenCalled();
  });

  it('edits the command permissions of an installed application (J-10)', async () => {
    seed({ perms: [MANAGE.webhooks, MANAGE.guild] });
    const put = capture();
    let saved = [{ id: '9', type: 2, permission: false }];
    await openWith('integrations', () =>
      server.use(
        http.get('*/api/v1/guilds/g1/webhooks', () => HttpResponse.json([])),
        http.get('*/api/v1/guilds/g1/integrations', () =>
          HttpResponse.json([integration('50', 'Helper')]),
        ),
        http.get('*/api/v1/guilds/g1/members', () =>
          HttpResponse.json([
            { user: { id: '2', username: 'bob' }, roles: [], joined_at: '2026-01-01T00:00:00Z' },
          ]),
        ),
        http.get('*/api/v1/applications/50/guilds/g1/commands', ({ request }) => {
          expect(new URL(request.url).searchParams.get('include_global')).toBe('true');
          return HttpResponse.json([
            { id: '700', name: 'ping', guild_id: null },
            { id: '701', name: 'ban-hammer', guild_id: 'g1' },
          ]);
        }),
        http.get('*/api/v1/applications/50/guilds/g1/commands/permissions', () =>
          HttpResponse.json([
            {
              id: '700',
              application_id: '50',
              guild_id: 'g1',
              permissions: [{ id: 'g1', type: 1, permission: false }],
            },
          ]),
        ),
        http.put(
          '*/api/v1/applications/50/guilds/g1/commands/701/permissions',
          async ({ request }) => {
            await put.record(request);
            const body = (await request.json()) as { permissions: typeof saved };
            saved = body.permissions;
            return HttpResponse.json({
              id: '701',
              application_id: '50',
              guild_id: 'g1',
              permissions: saved,
            });
          },
        ),
      ),
    );
    await userEvent.click(
      await screen.findByRole('button', { name: 'Command permissions of Helper' }),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Command permissions for Helper' });
    // /ping already denies Everyone.
    await userEvent.selectOptions(await within(dialog).findByLabelText('Command'), '700');
    expect(within(dialog).getByLabelText('Rule for Everyone')).toHaveValue('deny');
    // /ban-hammer: allow a role and one channel, deny Everyone and one member.
    await userEvent.selectOptions(within(dialog).getByLabelText('Command'), '701');
    expect(within(dialog).getByLabelText('Rule for Everyone')).toHaveValue('default');
    await userEvent.selectOptions(within(dialog).getByLabelText('Rule for Everyone'), 'deny');
    await userEvent.selectOptions(within(dialog).getByLabelText('Rule for All channels'), 'allow');
    await userEvent.selectOptions(
      within(dialog).getByLabelText('Add a role, member or channel'),
      '1:mod',
    );
    await userEvent.selectOptions(
      within(dialog).getByLabelText('Add a role, member or channel'),
      '2:2',
    );
    await userEvent.selectOptions(within(dialog).getByLabelText('Rule for bob'), 'deny');
    expect(within(dialog).getByLabelText('Rule for @Mod')).toHaveValue('allow');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(put.calls).toHaveLength(1));
    // 18446744073709551615 stands for nothing here: "All channels" is the server id minus one.
    expect(put.calls[0]!.body).toEqual({
      permissions: expect.arrayContaining([
        { id: 'g1', type: 1, permission: false },
        { id: 'mod', type: 1, permission: true },
        { id: '2', type: 2, permission: false },
      ]),
    });
  });

  it('unfollows a channel from its own tab and keeps followers out of Webhooks', async () => {
    seed({ perms: [MANAGE.webhooks] });
    const del = capture();
    const setup = () =>
      server.use(
        http.get('*/api/v1/guilds/g1/webhooks', () =>
          HttpResponse.json([hook('w1', 1, 'CI'), hook('w2', 2, 'News')]),
        ),
        http.get('*/api/v1/guilds/g1/members', () => HttpResponse.json([])),
        http.delete('*/api/v1/webhooks/w2', async ({ request }) => {
          await del.record(request);
          return new HttpResponse(null, { status: 204 });
        }),
      );
    await openWith('integrations', setup);
    await userEvent.click(await screen.findByRole('tab', { name: 'Webhooks' }));
    expect(await screen.findByText('CI')).toBeVisible();
    expect(screen.queryByText('News')).toBeNull();
    await userEvent.click(screen.getByRole('tab', { name: 'Followed Channels' }));
    expect(await screen.findByText('News')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Unfollow News' }));
    await userEvent.click(
      within(await screen.findByRole('dialog', { name: 'Unfollow News?' })).getByRole('button', {
        name: 'Unfollow',
      }),
    );
    await waitFor(() => expect(del.calls).toHaveLength(1));
    expect(await screen.findByText('No followed channels')).toBeVisible();
  });
});

describe('Soundboard settings (LNG-SSET-025)', () => {
  const sound = (id: string, name: string) => ({
    sound_id: id,
    name,
    volume: 0.5,
    emoji_name: '🔔',
    guild_id: 'g1',
    available: true,
    user: { id: '2', username: 'bob' },
  });

  afterEach(() => vi.unstubAllGlobals());

  const fakeAudio = (duration: number) => {
    class FakeAudio {
      duration = duration;
      preload = '';
      onloadedmetadata: (() => void) | null = null;
      onerror: (() => void) | null = null;
      set src(_: string) {
        queueMicrotask(() => this.onloadedmetadata?.());
      }
    }
    vi.stubGlobal('Audio', FakeAudio);
    vi.stubGlobal(
      'URL',
      Object.assign(URL, { createObjectURL: () => 'blob:x', revokeObjectURL: () => undefined }),
    );
  };

  it('lists sounds, renames one and deletes another', async () => {
    seed({ perms: [MANAGE.expressions] });
    const patch = capture();
    const del = capture();
    await openWith('expressions', () =>
      server.use(
        http.get('*/api/v1/guilds/g1/soundboard-sounds', () =>
          HttpResponse.json({ items: [sound('7', 'Airhorn'), sound('8', 'Ding')] }),
        ),
        http.patch('*/api/v1/guilds/g1/soundboard-sounds/7', async ({ request }) => {
          await patch.record(request);
          return HttpResponse.json(sound('7', 'Horn'));
        }),
        http.delete('*/api/v1/guilds/g1/soundboard-sounds/8', async ({ request }) => {
          await del.record(request);
          return new HttpResponse(null, { status: 204 });
        }),
      ),
    );
    await userEvent.click(await screen.findByRole('tab', { name: 'Soundboard' }));
    const input = await screen.findByLabelText('Name of Airhorn');
    expect(screen.getAllByText('Uploaded by bob')).toHaveLength(2);
    await userEvent.clear(input);
    await userEvent.type(input, 'Horn');
    await userEvent.click(screen.getByRole('button', { name: 'Save Airhorn' }));
    await waitFor(() => expect(patch.calls).toHaveLength(1));
    expect(patch.calls[0]!.body).toEqual({ name: 'Horn', volume: 0.5 });
    await userEvent.click(screen.getByRole('button', { name: 'Delete Ding' }));
    await userEvent.click(
      within(await screen.findByRole('dialog', { name: 'Delete Ding?' })).getByRole('button', {
        name: 'Delete',
      }),
    );
    await waitFor(() => expect(del.calls).toHaveLength(1));
  });

  it('uploads a short sound and refuses one over five seconds', async () => {
    seed({ perms: [MANAGE.expressions] });
    const post = capture();
    await openWith('expressions', () =>
      server.use(
        http.get('*/api/v1/guilds/g1/soundboard-sounds', () => HttpResponse.json({ items: [] })),
        http.post('*/api/v1/guilds/g1/soundboard-sounds', async ({ request }) => {
          await post.record(request);
          return HttpResponse.json(sound('9', 'Clap'), { status: 201 });
        }),
      ),
    );
    await userEvent.click(await screen.findByRole('tab', { name: 'Soundboard' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Upload a sound' }));
    const dialog = await screen.findByRole('dialog', { name: 'Upload a sound' });

    fakeAudio(8);
    const file = new File(['abc'], 'clap.wav', { type: 'audio/wav' });
    await userEvent.upload(within(dialog).getByLabelText('Sound file'), file);
    expect(await within(dialog).findByText(/8\.0 seconds long/)).toBeVisible();
    expect(within(dialog).getByRole('button', { name: 'Upload' })).toBeDisabled();

    fakeAudio(2);
    await userEvent.upload(
      within(dialog).getByLabelText('Sound file'),
      new File(['abc'], 'clap.wav', { type: 'audio/wav' }),
    );
    await waitFor(() => expect(within(dialog).getByLabelText('Name')).toHaveValue('clap'));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Upload' }));
    await waitFor(() => expect(post.calls).toHaveLength(1));
    const sent = post.calls[0]!.body as Record<string, unknown>;
    expect(sent.name).toBe('clap');
    expect(sent.volume).toBe(1);
    expect(String(sent.sound)).toMatch(/^data:/);
  });
});

describe('Channel settings: announcement, voice limits, followers (LNG-CHN-017)', () => {
  const ch = (id: string, name: string, type: number, extra: object = {}) => ({
    id,
    type,
    guild_id: 'g1',
    name,
    topic: null,
    position: 0,
    parent_id: null,
    nsfw: false,
    rate_limit_per_user: 0,
    permission_overwrites: [],
    ...extra,
  });

  it('turns a text channel into an announcement channel', async () => {
    seed({ perms: [MANAGE.channels] });
    const cap = capture();
    await openWith('channels', () =>
      server.use(
        http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json([ch('c1', 'news', 0)])),
        http.patch('*/api/v1/channels/c1', async ({ request }) => {
          await cap.record(request);
          return HttpResponse.json(ch('c1', 'news', 5));
        }),
      ),
    );
    await userEvent.click(await screen.findByRole('button', { name: '# news' }));
    await userEvent.click(await screen.findByRole('switch', { name: /Announcement channel/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ type: 5 });
  });

  it('changes the user limit and bitrate of a voice channel', async () => {
    seed({ perms: [MANAGE.channels] });
    const cap = capture();
    await openWith('channels', () =>
      server.use(
        http.get('*/api/v1/guilds/g1/channels', () =>
          HttpResponse.json([ch('v1', 'Lounge', 2, { user_limit: 0, bitrate: 64000 })]),
        ),
        http.patch('*/api/v1/channels/v1', async ({ request }) => {
          await cap.record(request);
          return HttpResponse.json(ch('v1', 'Lounge', 2));
        }),
      ),
    );
    await userEvent.click(
      await screen.findAllByRole('button', { name: /Lounge/ }).then((l) => l[0]!),
    );
    fireEvent.change(await screen.findByLabelText(/User limit/), { target: { value: '5' } });
    fireEvent.change(screen.getByLabelText(/Bitrate/), { target: { value: '96' } });
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ user_limit: 5, bitrate: 96000 });
  });

  it('lists followers of a channel and unfollows one', async () => {
    seed({ perms: [MANAGE.channels, MANAGE.webhooks] });
    const del = capture();
    await openWith('channels', () =>
      server.use(
        http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json([ch('c1', 'news', 5)])),
        http.get('*/api/v1/guilds/g1/webhooks', () =>
          HttpResponse.json([
            { id: 'w2', type: 2, guild_id: 'g1', channel_id: 'c1', name: 'Far Away', avatar: null },
          ]),
        ),
        http.delete('*/api/v1/webhooks/w2', async ({ request }) => {
          await del.record(request);
          return new HttpResponse(null, { status: 204 });
        }),
      ),
    );
    await userEvent.click(await screen.findByRole('button', { name: '# news' }));
    await userEvent.click(await screen.findByRole('tab', { name: 'Integrations' }));
    expect(await screen.findByText('Far Away')).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Unfollow Far Away' }));
    const dialogs = await screen.findAllByRole('dialog');
    await userEvent.click(within(dialogs.at(-1)!).getAllByRole('button').at(-1)!);
    await waitFor(() => expect(del.calls).toHaveLength(1));
  });
});
