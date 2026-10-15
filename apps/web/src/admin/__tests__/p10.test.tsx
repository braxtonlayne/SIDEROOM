// Phase-10 admin front end: shared table, account drawer extras, person record, invite graph.
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { buildCsv, csvField, DataTable, type Column } from '../DataTable';
import { buildTree } from '../sections/InviteGraph';
import { overridesOf, schemaFields, type SchemaField } from '../SchemaForm';
import type { Paged } from '../ui';
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

describe('csv', () => {
  it('quotes separators and defuses spreadsheet formulas', () => {
    expect(csvField('plain')).toBe('plain');
    expect(csvField('a,b')).toBe('"a,b"');
    expect(csvField('say "hi"')).toBe('"say ""hi"""');
    expect(csvField('=SUM(A1)')).toBe("'=SUM(A1)");
    expect(csvField('-2+3')).toBe("'-2+3");
    expect(csvField(-5)).toBe('-5');
    expect(csvField(null)).toBe('');
  });

  it('exports only columns with a csv value', () => {
    const cols: Column<{ n: string; k: number }>[] = [
      { id: 'n', header: 'Name', cell: (r) => r.n, csv: (r) => r.n },
      { id: 'ui', header: 'Open', cell: () => 'x' },
      { id: 'k', header: 'Count', cell: (r) => r.k, csv: (r) => r.k },
    ];
    expect(
      buildCsv(
        [
          { n: 'a', k: 1 },
          { n: 'b,c', k: 2 },
        ],
        cols,
      ),
    ).toBe('Name,Count\r\na,1\r\n"b,c",2\r\n');
  });
});

describe('DataTable', () => {
  interface Row {
    id: string;
    name: string;
    email: string;
  }
  const columns: Column<Row>[] = [
    { id: 'name', header: 'Name', locked: true, cell: (r) => r.name, csv: (r) => r.name },
    { id: 'email', header: 'Email', cell: (r) => r.email, csv: (r) => r.email },
    { id: 'id', header: 'Id', defaultHidden: true, cell: (r) => r.id, csv: (r) => r.id },
  ];
  const paged = (over: Partial<Paged<Row>> = {}): Paged<Row> => ({
    items: [{ id: '1', name: 'Ann', email: 'ann@example.test' }],
    loading: false,
    error: null,
    more: false,
    loadMore: () => undefined,
    reload: () => undefined,
    ...over,
  });
  const view = (p: Paged<Row>) =>
    render(
      <DataTable
        tableId="test-table"
        label="People"
        columns={columns}
        paged={p}
        rowKey={(r) => r.id}
        emptyTitle="Nobody"
      />,
    );

  beforeEach(() => localStorage.clear());

  it('hides default-hidden columns and lets the staff member show them, remembering the choice', async () => {
    const user = userEvent.setup();
    const first = view(paged());
    const table = screen.getByRole('table', { name: 'People' });
    expect(within(table).queryByRole('columnheader', { name: 'Id' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Columns' }));
    expect(screen.getByRole('checkbox', { name: 'Name' })).toBeDisabled();
    await user.click(screen.getByRole('checkbox', { name: 'Id' }));
    expect(within(table).getByRole('columnheader', { name: 'Id' })).toBeInTheDocument();
    first.unmount();
    view(paged());
    expect(screen.getByRole('columnheader', { name: 'Id' })).toBeInTheDocument();
  });

  it('shows the empty state, an error, the loaded count and Load more', async () => {
    const loadMore = vi.fn();
    const user = userEvent.setup();
    const { rerender } = view(paged({ items: [] }));
    expect(screen.getByText('Nobody')).toBeInTheDocument();
    rerender(
      <DataTable
        tableId="test-table"
        label="People"
        columns={columns}
        paged={paged({ more: true, loadMore })}
        rowKey={(r) => r.id}
        emptyTitle="Nobody"
      />,
    );
    expect(screen.getByRole('status')).toHaveTextContent('1 row loaded');
    await user.click(screen.getByRole('button', { name: 'Load more' }));
    expect(loadMore).toHaveBeenCalled();
  });

  it('exports the visible columns as a CSV download', async () => {
    const created: Blob[] = [];
    const orig = URL.createObjectURL;
    URL.createObjectURL = (b: Blob | MediaSource) => {
      created.push(b as Blob);
      return 'blob:test';
    };
    URL.revokeObjectURL = () => undefined;
    const user = userEvent.setup();
    view(paged());
    await user.click(screen.getByRole('button', { name: 'Export CSV' }));
    URL.createObjectURL = orig;
    expect(created).toHaveLength(1);
    const text = await new Promise<string>((res) => {
      const r = new FileReader();
      r.onload = () => res(String(r.result));
      r.readAsText(created[0]!);
    });
    expect(text).toBe('Name,Email\r\nAnn,ann@example.test\r\n');
  });
});

describe('account drawer extras', () => {
  it('shows entitlements and the audit trail to staff who may read them, and opens the person record', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_ACCOUNTS', 'MANAGE_DONATIONS', 'VIEW_ACCESS_LOG'] });
    let grantQuery = '';
    let logQuery = '';
    server.use(
      http.get('*/api/v1/admin/accounts', () =>
        HttpResponse.json({ items: [account()], next_after: null }),
      ),
      http.get('*/api/v1/admin/accounts/100', () => HttpResponse.json(detail())),
      http.get('*/api/v1/admin/donations/entitlements', ({ request }) => {
        grantQuery = new URL(request.url).search;
        return HttpResponse.json({
          items: [
            {
              id: '9',
              kind: 'patron',
              user_id: '100',
              guild_id: null,
              source: 'admin',
              source_id: null,
              days: null,
              ends_at_ms: null,
              permanent: true,
              usd_cents: null,
              starts_at_ms: 1,
              revoked_at_ms: null,
              reason: null,
            },
          ],
          next_before: null,
        });
      }),
      http.get('*/api/v1/admin/access-log', ({ request }) => {
        const u = new URL(request.url);
        logQuery = u.search;
        return HttpResponse.json({
          items: [
            {
              seq: 5,
              at_us: 1_700_000_000_000_000,
              actor_id: '1',
              kind: 'admin_action',
              action: 'accounts.suspend',
              target_kind: 'account',
              target_id: '100',
              reason: 'spam wave',
              detail: '',
              ip: null,
              prev_hash: 'a',
              hash: 'b',
            },
          ],
          next_before: null,
        });
      }),
      http.get('*/api/v1/admin/persons/500', () =>
        HttpResponse.json({
          id: '500',
          accounts: [
            account({ id: '101', username: 'newer', created_at_ms: 2_000 }),
            account({ id: '100', username: 'older', created_at_ms: 1_000 }),
          ],
          ban: null,
          notes: 'watch this one',
          suspicion_level: 2,
        }),
      ),
    );
    openAdmin('accounts');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'View alice' }));
    expect(await screen.findByText('Permanent')).toBeInTheDocument();
    expect(await screen.findByText('accounts.suspend')).toBeInTheDocument();
    expect(grantQuery).toContain('user_id=100');
    expect(logQuery).toContain('target_id=100');

    await user.click(screen.getByRole('button', { name: 'Open person record' }));
    const chain = await screen.findByRole('list', { name: 'Account chain' });
    const names = within(chain)
      .getAllByRole('listitem')
      .map((li) => li.textContent);
    expect(names[0]).toContain('@older');
    expect(names[1]).toContain('@newer');
    expect(screen.getByText('watch this one')).toBeInTheDocument();
  });

  it('hides those sections from staff without the permissions', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_ACCOUNTS'] });
    server.use(
      http.get('*/api/v1/admin/accounts', () =>
        HttpResponse.json({ items: [account()], next_after: null }),
      ),
      http.get('*/api/v1/admin/accounts/100', () => HttpResponse.json(detail())),
    );
    openAdmin('accounts');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'View alice' }));
    await screen.findByText('Open person record');
    expect(screen.queryByText('Entitlements')).toBeNull();
    expect(screen.queryByText('Audit trail')).toBeNull();
  });
});

describe('invite graph', () => {
  const node = (id: string, parent: string | null, over: Record<string, unknown> = {}) => ({
    account_id: id,
    username: `user${id}`,
    state: 'active',
    person_id: `p${id}`,
    suspicion_level: 0,
    parent_id: parent,
    depth: parent === '1' ? 1 : 2,
    ...over,
  });

  it('nests descendants under their inviters', () => {
    const root = node('1', null, { depth: 0 }) as never;
    const tree = buildTree(root, [node('2', '1'), node('3', '2'), node('4', '1')] as never);
    expect(tree.children.map((c) => c.node.account_id)).toEqual(['2', '4']);
    expect(tree.children[0]!.children.map((c) => c.node.account_id)).toEqual(['3']);
  });

  const seed = () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_INVITES', 'MANAGE_ACCOUNTS'] });
    server.use(
      http.get('*/api/v1/admin/accounts/1/invite-tree', () =>
        HttpResponse.json({
          account: node('1', null, { depth: 0 }),
          ancestors: [],
          stats: {
            issued: 3,
            open: 1,
            redeemed: 2,
            revoked: 0,
            descendants: 3,
            banned_descendants: 1,
          },
        }),
      ),
      http.get('*/api/v1/admin/accounts/1/invite-descendants', () =>
        HttpResponse.json({
          nodes: [
            node('2', '1'),
            node('3', '2', { state: 'platform_banned', suspicion_level: 3 }),
            node('4', '1'),
          ],
          truncated: false,
        }),
      ),
      http.get('*/api/v1/admin/accounts/3/invite-tree', () =>
        HttpResponse.json({
          account: node('3', '2'),
          ancestors: [node('1', null, { depth: -2 }), node('2', '1', { depth: -1 })],
          stats: {
            issued: 0,
            open: 0,
            redeemed: 0,
            revoked: 0,
            descendants: 0,
            banned_descendants: 0,
          },
        }),
      ),
      http.get('*/api/v1/admin/accounts/3/invite-descendants', () =>
        HttpResponse.json({ nodes: [], truncated: false }),
      ),
      http.get('*/api/v1/admin/accounts/:id', ({ params }) =>
        HttpResponse.json(
          detail(
            { suspicion_level: params.id === '3' ? 3 : 0 },
            {
              id: params.id,
              username: `user${String(params.id)}`,
              person_id: `p${String(params.id)}`,
            },
          ),
        ),
      ),
      http.get('*/api/v1/admin/invites', ({ request }) => {
        const issuer = new URL(request.url).searchParams.get('issuer_id');
        return HttpResponse.json({
          items:
            issuer === '3'
              ? [
                  {
                    id: '70',
                    code: 'OPENCODE',
                    issuer_account_id: '3',
                    state: 'open',
                    emergency: false,
                    note: '',
                    created_at_ms: 1,
                  },
                ]
              : [],
          next_after: null,
        });
      }),
      http.get('*/api/v1/admin/accounts/:id/credits', () =>
        HttpResponse.json({ balance: 4, entries: [] }),
      ),
    );
  };

  it('shows the tree with banned and suspicion markers, collapses a subtree, and switches to the list', async () => {
    seed();
    openAdmin('invite-graph');
    const user = userEvent.setup();
    const tree = await screen.findByRole('tree', { name: 'Invite tree' });
    expect(within(tree).getAllByRole('treeitem')).toHaveLength(3);
    expect(within(tree).getByText('Banned')).toBeInTheDocument();
    expect(within(tree).getByText('Suspicion 3')).toBeInTheDocument();
    await user.click(within(tree).getByRole('button', { name: 'Collapse @user2' }));
    expect(within(tree).getAllByRole('treeitem')).toHaveLength(2);
    await user.click(screen.getByRole('button', { name: 'List' }));
    const table = screen.getByRole('table', { name: 'Invited accounts' });
    expect(within(table).getAllByRole('row')).toHaveLength(4);
  });

  it('selects a node, shows its open codes, re-roots on it with a breadcrumb, and adjusts credits with a reason', async () => {
    seed();
    let sent: unknown;
    server.use(
      http.post('*/api/v1/admin/accounts/3/credits', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({ balance: 6 });
      }),
    );
    openAdmin('invite-graph');
    const user = userEvent.setup();
    await screen.findByRole('tree', { name: 'Invite tree' });
    await user.click(screen.getByRole('button', { name: 'Select @user3' }));
    const panel = screen.getByRole('complementary', { name: 'Selected account' });
    expect(await within(panel).findByText('OPENCODE')).toBeInTheDocument();

    await user.click(within(panel).getByRole('button', { name: 'Grant or revoke credits' }));
    const dialog = await screen.findByRole('dialog', { name: /Invite credits for @user3/ });
    await user.clear(within(dialog).getByLabelText('Change'));
    await user.type(within(dialog).getByLabelText('Change'), '2');
    await user.type(within(dialog).getByLabelText(/Reason/), 'goodwill');
    await user.click(within(dialog).getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(sent).toEqual({ delta: 2, reason: 'goodwill' }));

    await user.click(within(panel).getByRole('button', { name: 'Make this the root' }));
    const crumbs = await screen.findByRole('navigation', { name: 'Path to the root' });
    expect(within(crumbs).getByRole('button', { name: '@user1' })).toBeInTheDocument();
    expect(within(crumbs).getByRole('button', { name: '@user2' })).toBeInTheDocument();
    expect(await screen.findByText('This account has not invited anyone yet.')).toBeInTheDocument();
  });
});

describe('review queues hub', () => {
  it('shows only the tabs the staff role may work', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_APPS', 'DELETE_CONTENT'] });
    server.use(
      http.get('*/api/v1/admin/developers/requests', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/developers/versions', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/cinema/reports', () =>
        HttpResponse.json({ items: [], next_before: null }),
      ),
    );
    openAdmin('review');
    await screen.findByRole('tab', { name: 'Apps' });
    expect(screen.getAllByRole('tab').map((t) => t.textContent)).toEqual(['Cinema', 'Apps']);
  });

  it('approves a developer application with a note', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'REVIEW_APPS'] });
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/developers/requests', () =>
        HttpResponse.json([
          {
            id: '5',
            account_id: '100',
            state: 'pending',
            pitch: 'A music bot',
            prior_work: '',
            created_at: '2026-01-02T00:00:00Z',
            account: { id: '100', username: 'botmaker' },
          },
        ]),
      ),
      http.get('*/api/v1/admin/developers/versions', () => HttpResponse.json([])),
      http.post('*/api/v1/admin/developers/requests/5/decision', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({});
      }),
    );
    openAdmin('review');
    const user = userEvent.setup();
    await user.click(
      await screen.findByRole('button', { name: 'Approve application from @botmaker' }),
    );
    const dialog = await screen.findByRole('dialog', {
      name: /Approve this developer application/,
    });
    await user.type(within(dialog).getByLabelText(/Note to the developer/), 'welcome');
    await user.click(within(dialog).getByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(sent).toEqual({ approve: true, note: 'welcome' }));
    expect(await screen.findByText('Decision recorded.')).toBeInTheDocument();
  });

  it('hides reported cinema content with the report closing as actioned by the server', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'DELETE_CONTENT'] });
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/cinema/reports', () =>
        HttpResponse.json({
          items: [
            {
              id: '8',
              reporter_id: '2',
              target_kind: 'review',
              target_id: '77',
              reason: 'spoilers',
              status: 'open',
              created_at_ms: 1_700_000_000_000,
              content: {
                title_id: '3',
                author_id: '4',
                excerpt: 'He was the killer all along',
                hidden: false,
                created_at_ms: 1,
              },
            },
          ],
          next_before: null,
        }),
      ),
      http.put('*/api/v1/admin/cinema/reviews/77/hidden', async ({ request }) => {
        sent = await readJson(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAdmin('review');
    const user = userEvent.setup();
    expect(await screen.findByText('He was the killer all along')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Hide the content of report 8' }));
    const dialog = await screen.findByRole('dialog', { name: /Hide reported content/ });
    await user.type(within(dialog).getByLabelText(/Reason/), 'spoiler');
    await user.click(within(dialog).getByRole('button', { name: 'Hide' }));
    await waitFor(() => expect(sent).toEqual({ hidden: true, reason: 'spoiler' }));
  });
});

describe('library catalog', () => {
  const series = (over: Record<string, unknown> = {}) => ({
    id: '900',
    kind: 'manga',
    title: 'Blue Lantern',
    alt_titles: [],
    authors: ['A. Writer'],
    artists: [],
    status: 'ongoing',
    year: 2020,
    synopsis: '',
    genres: [],
    tags: [],
    reading_direction: 'rtl',
    age_restricted: false,
    cover_url: null,
    published: false,
    published_at_ms: null,
    merged_into: null,
    chapter_count: 0,
    last_chapter_at_ms: null,
    follower_count: 0,
    created_at_ms: 1_700_000_000_000,
    updated_at_ms: 1_700_000_000_000,
    ...over,
  });

  it('lists series, edits one, and publishes a chapter', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'PUBLISH_READING'] });
    let saved: Record<string, unknown> | undefined;
    let published = false;
    server.use(
      http.get('*/api/v1/admin/library/series', () =>
        HttpResponse.json({ items: [series()], total: 1, limit: 50, offset: 0 }),
      ),
      http.get('*/api/v1/admin/library/series/900', () => HttpResponse.json(series())),
      http.put('*/api/v1/admin/library/series/900', async ({ request }) => {
        saved = (await readJson(request)) as Record<string, unknown>;
        return HttpResponse.json(series({ title: 'Blue Lantern 2' }));
      }),
      http.get('*/api/v1/admin/library/series/900/chapters', () =>
        HttpResponse.json({
          editions: [
            { id: '1', series_id: '900', language: 'en', label: 'Official', is_default: true },
          ],
          volumes: [],
          chapters: [
            {
              id: '11',
              series_id: '900',
              edition_id: '1',
              number: 1,
              title: 'Start',
              page_count: 20,
              format: 'images',
              published,
            },
          ],
        }),
      ),
      http.post('*/api/v1/admin/library/chapters/11/publish', () => {
        published = true;
        return HttpResponse.json({});
      }),
    );
    openAdmin('library-catalog');
    const user = userEvent.setup();
    await screen.findByRole('table', { name: 'Library series' });
    await user.click(screen.getByRole('button', { name: 'Edit Blue Lantern' }));
    const title = await screen.findByLabelText('Title');
    await user.clear(title);
    await user.type(title, 'Blue Lantern 2');
    await user.type(screen.getByLabelText('Genres'), 'drama, sea');
    await user.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(saved?.title).toBe('Blue Lantern 2'));
    expect(saved?.genres).toEqual(['drama', 'sea']);
    expect(saved?.authors).toEqual(['A. Writer']);

    await user.click(await screen.findByRole('button', { name: 'Publish chapter 1' }));
    await waitFor(() => expect(published).toBe(true));
    expect(await screen.findByRole('button', { name: 'Unpublish chapter 1' })).toBeInTheDocument();
  });
});

describe('G-26 library merge needs both bits', () => {
  const row = {
    id: '900',
    kind: 'manga',
    title: 'Blue Lantern',
    alt_titles: [],
    authors: [],
    artists: [],
    status: 'ongoing',
    year: 2020,
    synopsis: '',
    genres: [],
    tags: [],
    reading_direction: 'rtl',
    age_restricted: false,
    cover_url: null,
    published: false,
    published_at_ms: null,
    merged_into: null,
    chapter_count: 0,
    last_chapter_at_ms: null,
    follower_count: 0,
    created_at_ms: 1_700_000_000_000,
    updated_at_ms: 1_700_000_000_000,
  };
  const serve = () =>
    server.use(
      http.get('*/api/v1/admin/library/series', () =>
        HttpResponse.json({ items: [row], total: 1, limit: 50, offset: 0 }),
      ),
      http.get('*/api/v1/admin/library/series/900', () => HttpResponse.json(row)),
      http.get('*/api/v1/admin/library/series/900/chapters', () =>
        HttpResponse.json({ editions: [], volumes: [], chapters: [] }),
      ),
    );

  it('hides Merge when the viewer lacks DELETE_CONTENT', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'PUBLISH_READING'] });
    serve();
    openAdmin('library-catalog');
    await screen.findByRole('table', { name: 'Library series' });
    await userEvent.click(screen.getByRole('button', { name: 'Edit Blue Lantern' }));
    await screen.findByRole('button', { name: 'Publish' });
    expect(screen.queryByRole('button', { name: 'Merge into another series' })).toBeNull();
  });

  it('shows Merge with PUBLISH_READING and DELETE_CONTENT', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'PUBLISH_READING', 'DELETE_CONTENT'] });
    serve();
    openAdmin('library-catalog');
    await screen.findByRole('table', { name: 'Library series' });
    await userEvent.click(screen.getByRole('button', { name: 'Edit Blue Lantern' }));
    expect(await screen.findByRole('button', { name: 'Merge into another series' })).toBeInTheDocument();
  });
});

describe('dashboard tiles and setup checklist', () => {
  const STATS = {
    accounts_active: 7,
    accounts_banned: 0,
    accounts_deactivated: 1,
    accounts_deleted: 0,
    jobs_dead: 2,
    jobs_queued: 0,
    jobs_running: 0,
    sessions_active: 4,
    staff: 1,
  };

  it('shows review sizes, donations against cost, alerts and the setup steps for the Owner', async () => {
    asStaff({ perms: [], owner: true });
    server.use(
      http.get('*/api/v1/admin/stats', () => HttpResponse.json(STATS)),
      http.get('*/api/v1/admin/settings', () =>
        HttpResponse.json([
          {
            key: 'accounts.cap',
            group: 'limits',
            kind: 'integer',
            description: '',
            min: 1,
            max: 99,
            value: 20,
            default: 20,
            is_default: true,
          },
        ]),
      ),
      http.get('*/api/v1/admin/system', () =>
        HttpResponse.json({
          counts: STATS,
          gateway_sessions: 2,
          probes: [
            { name: 'database', ok: true },
            { name: 'object storage', ok: false, error: 'timeout' },
          ],
          version: '1',
        }),
      ),
      http.get('*/api/v1/admin/review/queue', () =>
        HttpResponse.json({ items: [], total: 12, limit: 1, offset: 0 }),
      ),
      http.get('*/api/v1/admin/developers/requests', () => HttpResponse.json([{ id: '1' }])),
      http.get('*/api/v1/admin/developers/versions', () =>
        HttpResponse.json([{ id: '2' }, { id: '3' }]),
      ),
      http.get('*/api/v1/admin/cinema/reports', () =>
        HttpResponse.json({ items: [{ id: '1' }], next_before: null }),
      ),
      http.get('*/api/v1/admin/donations/config', () =>
        HttpResponse.json({ hosting_cost_cents: 10_000 }),
      ),
      http.get('*/api/v1/admin/donations/report', () =>
        HttpResponse.json({ credited_cents: 2_500, by_purpose: [], by_day: [], by_state: {} }),
      ),
      http.get('*/api/v1/admin/donations/status', () =>
        HttpResponse.json({ wallet: null, needs_attention: 1, counts: {} }),
      ),
      http.get('*/api/v1/admin/access-log/verify', () =>
        HttpResponse.json({ ok: false, rows_checked: 5, first_bad_seq: 4 }),
      ),
    );
    openAdmin('dashboard');
    expect(await screen.findByText('Library submissions waiting')).toBeInTheDocument();
    const group = screen.getByRole('group', { name: 'Review queue sizes' });
    expect(within(group).getByText('12')).toBeInTheDocument();
    expect(within(group).getByText('3')).toBeInTheDocument();
    expect(
      await screen.findByRole('progressbar', { name: /Donations credited/ }),
    ).toBeInTheDocument();
    const alerts = await screen.findByRole('list', { name: 'Recent alerts' });
    expect(within(alerts).getByText(/object storage is failing: timeout/)).toBeInTheDocument();
    expect(within(alerts).getByText(/breaks at entry 4/)).toBeInTheDocument();
    expect(within(alerts).getByText(/2 background jobs have failed/)).toBeInTheDocument();

    const setup = await screen.findByRole('list', { name: 'Setup checklist' });
    expect(
      within(setup).getByText('The storage probe is failing. Check the System page.'),
    ).toBeInTheDocument();
    expect(
      within(setup).getByText('No wallet yet. Donations stay off until one is set.'),
    ).toBeInTheDocument();
    expect(within(setup).getAllByText('Cannot check yet')).toHaveLength(3);
  });

  it('shows none of it to basic staff', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    server.use(http.get('*/api/v1/admin/stats', () => HttpResponse.json(STATS)));
    openAdmin('dashboard');
    await screen.findByText('Active accounts');
    expect(screen.queryByText('Setup checklist')).toBeNull();
    expect(screen.queryByText('Donations against hosting cost')).toBeNull();
    expect(screen.queryByRole('group', { name: 'Review queue sizes' })).toBeNull();
  });
});

describe('feature settings form', () => {
  const fields: SchemaField[] = [
    { key: 'on', label: 'On', type: 'bool', default: true },
    { key: 'n', label: 'Limit', type: 'int', default: 5, min: 1, max: 10 },
    { key: 'note', label: 'Note', type: 'string' },
    {
      key: 'g',
      label: 'Group',
      type: 'group',
      fields: [{ key: 'x', label: 'X', type: 'bool', default: false }],
    },
  ];

  it('stores only values that differ from the schema defaults', () => {
    expect(overridesOf(fields, { on: true, n: 5, note: '', g: { x: false } })).toEqual({});
    expect(overridesOf(fields, { on: false, n: 7, note: 'hi', g: { x: true } })).toEqual({
      on: false,
      n: 7,
      note: 'hi',
      g: { x: true },
    });
    expect(schemaFields(null)).toEqual([]);
    expect(schemaFields({ fields })).toHaveLength(4);
  });

  it('edits a feature setting and saves the overrides with an optional reason', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_FEATURES'] });
    let sent: unknown;
    const feature = {
      id: 'moderation',
      name: 'Moderation',
      description: 'Auto moderation',
      section: 'lounge',
      phase: 1,
      scopes: ['platform'],
      enabled: true,
      default_enabled: true,
      schema: { fields },
      settings: { on: true, n: 5, note: '', g: { x: false } },
    };
    server.use(
      http.get('*/api/v1/admin/features', () => HttpResponse.json([feature])),
      http.patch('*/api/v1/admin/features/moderation', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(feature);
      }),
    );
    openAdmin('features');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Settings for Moderation' }));
    const limit = await screen.findByLabelText('Limit');
    await user.clear(limit);
    await user.type(limit, '8');
    await user.click(screen.getByRole('button', { name: 'Save settings' }));
    const dialog = await screen.findByRole('dialog', { name: /Save Moderation settings/ });
    await user.click(within(dialog).getByRole('button', { name: 'Save settings' }));
    await waitFor(() => expect(sent).toEqual({ settings: { n: 8 } }));
    expect(await screen.findByText('Moderation settings saved.')).toBeInTheDocument();
  });
});

describe('platform audit log', () => {
  it('lists only action rows and passes the filters to the server', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'VIEW_ACCESS_LOG'] });
    const queries: string[] = [];
    server.use(
      http.get('*/api/v1/admin/access-log', ({ request }) => {
        queries.push(new URL(request.url).search);
        return HttpResponse.json({
          items: [
            {
              seq: 9,
              at_us: 1_700_000_000_000_000,
              actor_id: '1',
              kind: 'admin_action',
              action: 'settings.patch',
              target_kind: 'setting',
              target_id: '77',
              reason: 'raise cap',
              detail: '',
              ip: null,
              prev_hash: 'a',
              hash: 'b',
            },
          ],
          next_before: null,
        });
      }),
    );
    openAdmin('audit-log');
    const user = userEvent.setup();
    const table = await screen.findByRole('table', { name: 'Platform audit log' });
    expect(within(table).getByText('settings.patch')).toBeInTheDocument();
    expect(queries[0]).toContain('kind=admin_action');
    await user.type(screen.getByLabelText('Action'), 'settings.patch');
    await user.click(screen.getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(queries.at(-1)).toContain('action=settings.patch'));
  });
});
