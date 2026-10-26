import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { TRIGGER } from '../api';
import { AUDIT_FILTERS, describeEntry, diffLines } from '../auditText';
import { emptyRule, formToBody, ruleToForm, validateRule } from '../sections/Moderation';
import {
  PERM,
  bitsToValues,
  canOpenSettingsFromBits,
  overwriteToValues,
  permissionGroups,
  valuesToBits,
  valuesToOverwrite,
} from '../permissions';
import { capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const guildObj = {
  id: 'g1',
  name: 'Book Club',
  icon: null,
  description: null,
  owner_id: '9',
  system_channel_id: null,
  verification_level: 1,
  default_message_notifications: 0,
  explicit_content_filter: 0,
  preferred_locale: 'en-US',
  features: [],
};

const rule = (over: object = {}) => ({
  id: 'r1',
  guild_id: 'g1',
  name: 'No spoilers',
  creator_id: '1',
  event_type: 1,
  trigger_type: 1,
  trigger_metadata: { keyword_filter: ['spoiler'], regex_patterns: [], allow_list: [] },
  actions: [{ type: 1, metadata: {} }],
  enabled: true,
  exempt_roles: [],
  exempt_channels: [],
  ...over,
});

function modHandlers(rules: object[] = [rule()]) {
  server.use(
    http.get('*/api/v1/guilds/g1', () => HttpResponse.json(guildObj)),
    http.get('*/api/v1/guilds/g1/auto-moderation/rules', () => HttpResponse.json(rules)),
  );
}

describe('AutoMod section', () => {
  it('lists rules and toggles one', async () => {
    seed({ perms: [MANAGE.guild] });
    const cap = capture();
    modHandlers();
    server.use(
      http.patch('*/api/v1/guilds/g1/auto-moderation/rules/r1', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(rule({ enabled: false }));
      }),
    );
    await openSettings('moderation');
    expect(await screen.findByText('No spoilers')).toBeInTheDocument();
    expect(screen.getByText(/Block custom words/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('switch', { name: 'Enable No spoilers' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ enabled: false });
  });

  it('creates a keyword rule with an alert and exemptions; validates first', async () => {
    seed({ perms: [MANAGE.guild] });
    const cap = capture();
    modHandlers([]);
    server.use(
      http.post('*/api/v1/guilds/g1/auto-moderation/rules', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(rule());
      }),
    );
    await openSettings('moderation');
    await userEvent.click(await screen.findByRole('button', { name: 'Create rule' }));
    const dialog = screen.getByRole('dialog', { name: 'Create AutoMod rule' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create rule' }));
    expect(
      await within(dialog).findByText('Give the rule a name (up to 100 characters).'),
    ).toBeInTheDocument();
    await userEvent.type(within(dialog).getByLabelText('Rule name'), 'Words');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create rule' }));
    expect(
      await within(dialog).findByText('Add at least one keyword or regular expression.'),
    ).toBeInTheDocument();
    await userEvent.type(within(dialog).getByLabelText('Keywords'), 'bad, worse');
    await userEvent.click(within(dialog).getByRole('checkbox', { name: 'Timeout member' }));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create rule' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toMatchObject({
      name: 'Words',
      event_type: 1,
      trigger_type: 1,
      trigger_metadata: { keyword_filter: ['bad', 'worse'], regex_patterns: [], allow_list: [] },
      actions: [
        { type: 1, metadata: {} },
        { type: 3, metadata: { duration_seconds: 600 } },
      ],
      enabled: true,
    });
  });

  it('shows a server error from rule creation', async () => {
    seed({ perms: [MANAGE.guild] });
    modHandlers([]);
    server.use(
      http.post('*/api/v1/guilds/g1/auto-moderation/rules', () =>
        HttpResponse.json(
          {
            message: 'Invalid Form Body',
            errors: { regex_patterns: { _errors: [{ message: 'Bad regex.' }] } },
          },
          { status: 400 },
        ),
      ),
    );
    await openSettings('moderation');
    await userEvent.click(await screen.findByRole('button', { name: 'Create rule' }));
    const dialog = screen.getByRole('dialog', { name: 'Create AutoMod rule' });
    await userEvent.type(within(dialog).getByLabelText('Rule name'), 'X');
    await userEvent.type(within(dialog).getByLabelText('Keywords'), 'a');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create rule' }));
    expect(await within(dialog).findByText(/regex_patterns: Bad regex\./)).toBeInTheDocument();
  });

  it('deletes a rule and saves verification level', async () => {
    seed({ perms: [MANAGE.guild] });
    const del = capture();
    const patch = capture();
    modHandlers();
    server.use(
      http.delete('*/api/v1/guilds/g1/auto-moderation/rules/r1', async ({ request }) => {
        await del.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
      http.patch('*/api/v1/guilds/g1', async ({ request }) => {
        await patch.record(request);
        return HttpResponse.json({ ...guildObj, verification_level: 3 });
      }),
    );
    await openSettings('moderation');
    await userEvent.click(await screen.findByRole('radio', { name: /^High Also/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(patch.calls).toHaveLength(1));
    expect(patch.calls[0]!.body).toEqual({ verification_level: 3 });
    await userEvent.click(screen.getByRole('button', { name: 'Delete rule No spoilers' }));
    await userEvent.click(screen.getByRole('button', { name: 'Delete rule' }));
    await waitFor(() => expect(del.calls).toHaveLength(1));
    expect(await screen.findByText('No AutoMod rules')).toBeInTheDocument();
  });
});

describe('AutoMod form helpers', () => {
  it('validates each trigger', () => {
    const f = { ...emptyRule(), name: 'n' };
    expect(validateRule({ ...f, keywords: '' })).toMatch(/keyword/);
    expect(validateRule({ ...f, keywords: 'a', regex: '(' })).toMatch(
      /not a valid regular expression/,
    );
    expect(validateRule({ ...f, keywords: 'a' })).toBeNull();
    expect(validateRule({ ...f, keywords: 'a', block: false })).toMatch(/at least one action/);
    expect(validateRule({ ...f, keywords: 'a', alert: true })).toMatch(/channel for the alert/);
    expect(validateRule({ ...f, trigger: TRIGGER.MENTION_SPAM, mentionLimit: 80 })).toMatch(
      /between 1 and 50/,
    );
    expect(validateRule({ ...f, trigger: TRIGGER.LINKS })).toMatch(/Block all links/);
    expect(validateRule({ ...f, trigger: TRIGGER.LINKS, blockAllLinks: true })).toBeNull();
    expect(validateRule({ ...f, trigger: TRIGGER.PRESET })).toMatch(/word list/);
    expect(validateRule({ ...f, trigger: TRIGGER.SPAM })).toBeNull();
    expect(validateRule({ ...f, keywords: 'a', blockMessage: 'x'.repeat(151) })).toMatch(/150/);
  });
  it('round-trips a rule through the form', () => {
    const r = rule({
      trigger_type: 100,
      trigger_metadata: {
        link_block_all: false,
        link_block_domains: ['evil.test'],
        link_allow_domains: [],
      },
      actions: [
        { type: 2, metadata: { channel_id: 'c1' } },
        { type: 3, metadata: { duration_seconds: 3600 } },
      ],
      exempt_roles: ['mod'],
    });
    const body = formToBody(ruleToForm(r as never), false);
    expect(body).toMatchObject({
      trigger_metadata: { link_block_domains: ['evil.test'], link_block_all: false },
      actions: r.actions,
      exempt_roles: ['mod'],
    });
    expect(body.trigger_type).toBeUndefined();
    expect(formToBody({ ...emptyRule(), name: ' a ', keywords: 'x' }, true)).toMatchObject({
      name: 'a',
      trigger_type: 1,
      event_type: 1,
    });
  });
});

describe('Audit log', () => {
  const entries = [
    {
      id: '1200000000000000000',
      user_id: '2',
      target_id: '3',
      action_type: 22,
      changes: [],
      reason: 'raid',
    },
    {
      id: '1100000000000000000',
      user_id: '2',
      target_id: 'mod',
      action_type: 31,
      changes: [
        {
          key: 'permissions',
          old_value: String(1n << 2n),
          new_value: String((1n << 2n) | (1n << 1n)),
        },
        { key: 'name', old_value: 'Old', new_value: 'Mod' },
      ],
    },
  ];

  it('renders readable text, expands diffs, filters, and pages', async () => {
    seed({ perms: [MANAGE.audit] });
    const urls: string[] = [];
    server.use(
      http.get('*/api/v1/guilds/g1/audit-logs', ({ request }) => {
        urls.push(request.url);
        const u = new URL(request.url);
        const list = u.searchParams.get('action_type') === '22' ? entries.slice(0, 1) : entries;
        return HttpResponse.json({
          audit_log_entries: list,
          users: [{ id: '2', username: 'bob' }],
        });
      }),
    );
    await openSettings('audit-log');
    expect(await screen.findByText('bob banned carol')).toBeInTheDocument();
    expect(screen.getByText('bob updated role Mod')).toBeInTheDocument();
    expect(screen.getByText(/Reason: raid/)).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole('button', { name: 'Show details for entry 1100000000000000000' }),
    );
    const changes = screen.getByRole('list', { name: 'Changes' });
    expect(within(changes).getByText('Ban Members')).toBeInTheDocument();
    expect(within(changes).getByText('Kick Members, Ban Members')).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText('Filter by action'), '22');
    await waitFor(() => expect(screen.queryByText('bob updated role Mod')).toBeNull());
    expect(urls.at(-1)).toContain('action_type=22');
    await userEvent.selectOptions(screen.getByLabelText('Filter by user'), '2');
    await waitFor(() => expect(urls.at(-1)).toContain('user_id=2'));
  });

  it('shows an error and an empty state', async () => {
    seed({ perms: [MANAGE.audit] });
    server.use(
      http.get('*/api/v1/guilds/g1/audit-logs', () =>
        HttpResponse.json({ message: 'Missing Access' }, { status: 403 }),
      ),
    );
    await openSettings('audit-log');
    expect(await screen.findByText('You do not have permission to do that.')).toBeInTheDocument();
  });
});

describe('auditText and permissions units', () => {
  const names = {
    user: (id: string) => `u${id}`,
    role: (id: string) => `r${id}`,
    channel: (id: string) => id,
  };
  it('describes entries', () => {
    const e = { id: '1', user_id: '2', target_id: '5', action_type: 10, changes: [] };
    expect(describeEntry(e, 'Ann', names)).toBe('Ann created channel #5');
    expect(describeEntry({ ...e, action_type: 40, target_id: null }, 'Ann', names)).toBe(
      'Ann created an invite',
    );
    expect(describeEntry({ ...e, action_type: 999 }, 'Ann', names)).toBe(
      'Ann performed action 999',
    );
    expect(AUDIT_FILTERS.find((f) => f.value === 22)?.label).toBe('Banned');
  });
  it('formats diffs', () => {
    const d = diffLines([
      { key: 'color', old_value: 0, new_value: 255 },
      { key: 'hoist', new_value: true },
      { key: 'deny', old_value: '0', new_value: String(1n << 10n) },
    ]);
    expect(d[0]).toMatchObject({ before: 'default', after: '#0000ff' });
    expect(d[1]).toMatchObject({ before: null, after: 'on' });
    expect(d[2]).toMatchObject({ before: 'no permissions', after: 'View Channels' });
  });
  it('covers every Discord bit and the SR bits', () => {
    const all = permissionGroups().flatMap((g) => g.items.map((i) => i.id));
    expect(new Set(all).size).toBe(all.length);
    for (const n of [
      'START_WATCH_PARTY',
      'CONTROL_WATCH_PARTY',
      'USE_LIBRARY_EMBEDS',
      'MANAGE_EXTENSIONS',
      'USE_EXPRESSIONS_EXTERNALLY',
    ])
      expect(all).toContain(n);
    expect(PERM.START_WATCH_PARTY).toBe(1n << 56n);
    expect(PERM.MANAGE_EXTENSIONS).toBe(1n << 59n);
    expect(PERM.USE_EXPRESSIONS_EXTERNALLY).toBe(1n << 60n);
    expect(PERM.PIN_MESSAGES).toBe(1n << 51n);
    expect(permissionGroups().at(-1)?.label).toBe('SIDE ROOM');
  });
  it('converts bits and overwrites', () => {
    const bits = PERM.SEND_MESSAGES! | PERM.START_WATCH_PARTY!;
    expect(valuesToBits(bitsToValues(bits))).toBe(bits);
    const v = overwriteToValues(PERM.SEND_MESSAGES!, PERM.VIEW_CHANNEL!);
    expect(v).toEqual({ SEND_MESSAGES: 'allow', VIEW_CHANNEL: 'deny' });
    expect(valuesToOverwrite(v)).toEqual({ allow: PERM.SEND_MESSAGES, deny: PERM.VIEW_CHANNEL });
    expect(canOpenSettingsFromBits(PERM.VIEW_AUDIT_LOG!)).toBe(true);
    expect(canOpenSettingsFromBits(PERM.SEND_MESSAGES!)).toBe(false);
  });
});
