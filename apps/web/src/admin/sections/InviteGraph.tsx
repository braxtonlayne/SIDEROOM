// Invite graph (ADM-GRAPH-003/004/005): a collapsible tree and a flat list of everyone a root account invited,
// a path-to-root breadcrumb and a side panel for the selected node. Past about 500 accounts the canvas 2D renderer
// (`graph/GraphCanvas.tsx`, layout from sr-graph-wasm) is the default picture; the tree and list views remain the
// accessible way to read the same accounts.
import { Badge, Button, Notice, Select, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import * as api from '../api';
import { useT } from '../../i18n';
import { CANVAS_THRESHOLD } from './graph/graphModel';
import { GraphCanvas } from './graph/GraphCanvas';
import {
  Async,
  ReasonDialog,
  SectionHeader,
  StateBadge,
  fmtMs,
  useAdminViewer,
  useResource,
} from '../ui';

export interface TreeRow {
  node: api.InviteNode;
  children: TreeRow[];
}

/** Nest the flat descendants list under the root. Nodes whose parent is missing (cap cut) hang off the root. */
export function buildTree(root: api.InviteNode, nodes: api.InviteNode[]): TreeRow {
  const rows = new Map<string, TreeRow>();
  rows.set(root.account_id, { node: root, children: [] });
  for (const n of nodes) rows.set(n.account_id, { node: n, children: [] });
  const top = rows.get(root.account_id)!;
  for (const n of nodes) {
    const parent = n.parent_id ? rows.get(n.parent_id) : undefined;
    (parent ?? top).children.push(rows.get(n.account_id)!);
  }
  return top;
}

const DEPTHS = [1, 2, 3, 5, 10, 20];

export function InviteGraphSection() {
  const t = useT();
  const viewer = useAdminViewer();
  const [rootId, setRootId] = useState(viewer.userId);
  const [draft, setDraft] = useState('');
  const [depth, setDepth] = useState('3');
  const [lookupError, setLookupError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [picked, setPicked] = useState<'tree' | 'list' | 'graph' | null>(null);

  const graph = useResource(`graph:${rootId}:${depth}`, async () => {
    const [tree, desc] = await Promise.all([
      api.getInviteTree(rootId),
      api.getInviteDescendants(rootId, Number(depth)),
    ]);
    return { tree, desc };
  });

  // The canvas is the default view of a big graph; a person's own pick wins.
  const big = (graph.data?.desc.nodes.length ?? 0) > CANVAS_THRESHOLD;
  const mode = picked ?? (big ? 'graph' : 'tree');
  const setMode = setPicked;

  const reroot = (id: string) => {
    setRootId(id);
    setSelected(null);
  };

  const go = async (e: FormEvent) => {
    e.preventDefault();
    const q = draft.trim();
    if (!q) return;
    setLookupError(null);
    if (/^\d+$/.test(q)) {
      reroot(q);
      return;
    }
    try {
      const page = await api.searchAccounts({ q, limit: 1 });
      const hit = page.items[0];
      if (hit) reroot(hit.id);
      else setLookupError(t('admin.graph.noMatch'));
    } catch {
      setLookupError(t('admin.graph.noMatch'));
    }
  };

  return (
    <>
      <SectionHeader title={t('admin.graph.title')} description={t('admin.graph.description')} />
      <form
        className="ss-toolbar"
        role="search"
        aria-label={t('admin.graph.rootForm')}
        onSubmit={(e) => void go(e)}
      >
        <TextInput
          label={t('admin.graph.rootLabel')}
          hint={t('admin.graph.rootHint')}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <Select
          aria-label={t('admin.graph.depthLabel')}
          options={DEPTHS.map((d) => ({ value: String(d), label: t('admin.graph.depth', { d }) }))}
          value={depth}
          onValueChange={setDepth}
        />
        <Button type="submit">{t('admin.graph.show')}</Button>
        <div className="adm-actions" role="group" aria-label={t('admin.graph.viewMode')}>
          {big && (
            <Button
              variant={mode === 'graph' ? 'primary' : 'secondary'}
              aria-pressed={mode === 'graph'}
              onClick={() => setMode('graph')}
            >
              {t('admin.graph.modeGraph')}
            </Button>
          )}
          <Button
            variant={mode === 'tree' ? 'primary' : 'secondary'}
            aria-pressed={mode === 'tree'}
            onClick={() => setMode('tree')}
          >
            {t('admin.graph.modeTree')}
          </Button>
          <Button
            variant={mode === 'list' ? 'primary' : 'secondary'}
            aria-pressed={mode === 'list'}
            onClick={() => setMode('list')}
          >
            {t('admin.graph.modeList')}
          </Button>
        </div>
      </form>
      {lookupError && (
        <Notice variant="warning" role="alert" className="ss-status">
          {lookupError}
        </Notice>
      )}
      <Async res={graph} label={t('admin.graph.loading')}>
        {({ tree, desc }) => (
          <div className="adm-graph">
            <div className="adm-graph-main">
              <nav aria-label={t('admin.graph.pathToRoot')} className="adm-crumbs">
                <ol>
                  {tree.ancestors.map((a) => (
                    <li key={a.account_id}>
                      <button
                        type="button"
                        className="adm-link"
                        onClick={() => reroot(a.account_id)}
                      >
                        @{a.username}
                      </button>
                    </li>
                  ))}
                  <li aria-current="page">@{tree.account.username}</li>
                </ol>
              </nav>
              <p className="ss-sub">
                {t('admin.graph.summary', {
                  descendants: tree.stats.descendants,
                  banned: tree.stats.banned_descendants,
                  shown: desc.nodes.length,
                })}
              </p>
              {desc.truncated && (
                <Notice variant="warning" className="ss-status">
                  {t('admin.graph.truncated')}
                </Notice>
              )}
              {desc.nodes.length === 0 ? (
                <p className="ss-na">{t('admin.graph.leaf')}</p>
              ) : mode === 'graph' && big ? (
                <GraphCanvas
                  root={tree.account}
                  nodes={desc.nodes}
                  selected={selected}
                  onSelect={setSelected}
                />
              ) : mode === 'tree' ? (
                <TreeView
                  root={buildTree(tree.account, desc.nodes)}
                  selected={selected}
                  onSelect={setSelected}
                />
              ) : (
                <ListView nodes={desc.nodes} selected={selected} onSelect={setSelected} />
              )}
            </div>
            <NodePanel
              accountId={selected ?? tree.account.account_id}
              onReroot={reroot}
              onChanged={() => void graph.reload()}
            />
          </div>
        )}
      </Async>
    </>
  );
}

function NodeBadges({ n }: { n: api.InviteNode }) {
  const t = useT();
  return (
    <>
      <StateBadge state={n.state} />
      {n.state === 'platform_banned' && (
        <Badge variant="danger">{t('admin.graph.bannedMarker')}</Badge>
      )}
      {n.suspicion_level > 0 && (
        <Badge variant="warning">{t('admin.graph.suspicion', { level: n.suspicion_level })}</Badge>
      )}
    </>
  );
}

function TreeView({
  root,
  selected,
  onSelect,
}: {
  root: TreeRow;
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const t = useT();
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const toggle = (id: string) =>
    setCollapsed((cur) => {
      const next = new Set(cur);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  const render = (row: TreeRow, level: number) => {
    const id = row.node.account_id;
    const open = !collapsed.has(id);
    const has = row.children.length > 0;
    return (
      <li
        key={id}
        role="treeitem"
        aria-level={level}
        aria-expanded={has ? open : undefined}
        aria-selected={selected === id}
      >
        <div className="adm-tree-row">
          {has ? (
            <Button
              size="sm"
              variant="ghost"
              aria-label={t(open ? 'admin.graph.collapse' : 'admin.graph.expand', {
                username: row.node.username,
              })}
              onClick={() => toggle(id)}
            >
              {open ? '▾' : '▸'}
            </Button>
          ) : (
            <span className="adm-tree-spacer" aria-hidden="true" />
          )}
          <button
            type="button"
            className="adm-link"
            aria-label={t('admin.graph.selectNode', { username: row.node.username })}
            onClick={() => onSelect(id)}
          >
            @{row.node.username}
          </button>
          <NodeBadges n={row.node} />
          {has && (
            <span className="ss-meta">
              {t('admin.graph.childCount', { count: row.children.length })}
            </span>
          )}
        </div>
        {has && open && (
          <ul role="group" className="adm-tree-children">
            {row.children.map((c) => render(c, level + 1))}
          </ul>
        )}
      </li>
    );
  };
  return (
    <ul role="tree" aria-label={t('admin.graph.treeLabel')} className="adm-tree">
      {root.children.map((c) => render(c, 1))}
    </ul>
  );
}

function ListView({
  nodes,
  selected,
  onSelect,
}: {
  nodes: api.InviteNode[];
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const t = useT();
  return (
    <div className="adm-table-wrap">
      <table className="adm-table" aria-label={t('admin.graph.listLabel')}>
        <thead>
          <tr>
            <th scope="col">{t('admin.graph.colAccount')}</th>
            <th scope="col">{t('admin.graph.colDepth')}</th>
            <th scope="col">{t('admin.graph.colInvitedBy')}</th>
            <th scope="col">{t('admin.graph.colState')}</th>
            <th scope="col">{t('admin.graph.colSuspicion')}</th>
          </tr>
        </thead>
        <tbody>
          {nodes.map((n) => (
            <tr key={n.account_id} aria-selected={selected === n.account_id}>
              <td>
                <button
                  type="button"
                  className="adm-link"
                  aria-label={t('admin.graph.selectNode', { username: n.username })}
                  onClick={() => onSelect(n.account_id)}
                >
                  @{n.username}
                </button>
              </td>
              <td>{n.depth}</td>
              <td className="adm-mono">{n.parent_id}</td>
              <td>
                <StateBadge state={n.state} />
              </td>
              <td>{n.suspicion_level}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function NodePanel({
  accountId,
  onReroot,
  onChanged,
}: {
  accountId: string;
  onReroot: (id: string) => void;
  onChanged: () => void;
}) {
  const t = useT();
  const viewer = useAdminViewer();
  const detail = useResource(`gnode:${accountId}`, () => api.getAccount(accountId));
  const open = useResource(`gnode-open:${accountId}`, () =>
    api.listInvites({ issuer_id: accountId, state: 'open', limit: 20 }),
  );
  const credits = useResource(`gnode-credits:${accountId}`, () => api.getCredits(accountId));
  const [pending, setPending] = useState<'credits' | 'clear' | 'freeze' | 'unfreeze' | null>(null);
  const [delta, setDelta] = useState('1');
  const [notice, setNotice] = useState<string | null>(null);
  const done = async (text: string) => {
    setPending(null);
    setNotice(text);
    await Promise.all([detail.reload(), credits.reload()]);
    onChanged();
  };
  const deltaNum = Number(delta);

  return (
    <aside className="adm-graph-panel ss-card" aria-label={t('admin.graph.panelLabel')}>
      <Async res={detail} label={t('admin.graph.loadingNode')}>
        {(d) => (
          <>
            <h2>@{d.account.username}</h2>
            {notice && (
              <Notice variant="success" role="status">
                {notice}
              </Notice>
            )}
            <dl className="adm-kv">
              <dt>{t('admin.graph.colState')}</dt>
              <dd>
                <StateBadge state={d.account.state} />
              </dd>
              <dt>{t('admin.accounts.dtJoined')}</dt>
              <dd>{fmtMs(d.account.created_at_ms)}</dd>
              <dt>{t('admin.accounts.dtInvitedBy')}</dt>
              <dd>
                {d.invited_by ? `@${d.invited_by.username}` : t('admin.accounts.rootAccount')}
              </dd>
              <dt>{t('admin.accounts.dtInvites')}</dt>
              <dd>
                {t('admin.accounts.invitesSummary', {
                  issued: d.invites_issued,
                  redeemed: d.invites_redeemed,
                })}
              </dd>
              <dt>{t('admin.accounts.dtSuspicion')}</dt>
              <dd>{d.suspicion_level}</dd>
              <dt>{t('admin.accounts.dtInviteCredits')}</dt>
              <dd>{credits.data ? credits.data.balance : d.credit_balance}</dd>
              {d.invites_frozen && (
                <>
                  <dt>{t('admin.accountExtras.freezeTitle')}</dt>
                  <dd>{t('admin.accountExtras.frozen')}</dd>
                </>
              )}
            </dl>
            <div className="adm-actions">
              <Button variant="secondary" onClick={() => onReroot(accountId)}>
                {t('admin.graph.reroot')}
              </Button>
              {viewer.can('MANAGE_INVITES') && (
                <Button variant="secondary" onClick={() => setPending('credits')}>
                  {t('admin.graph.adjustCredits')}
                </Button>
              )}
              {viewer.can('MANAGE_INVITES') && (
                <Button
                  variant={d.invites_frozen ? 'secondary' : 'danger'}
                  onClick={() => setPending(d.invites_frozen ? 'unfreeze' : 'freeze')}
                >
                  {d.invites_frozen
                    ? t('admin.accountExtras.unfreeze')
                    : t('admin.accountExtras.freeze')}
                </Button>
              )}
              {viewer.can('MANAGE_INVITES') && d.suspicion_level > 0 && (
                <Button variant="secondary" onClick={() => setPending('clear')}>
                  {t('admin.invites.clearSuspicion')}
                </Button>
              )}
            </div>
            <h3>{t('admin.graph.openCodes')}</h3>
            <Async res={open} label={t('admin.graph.loadingCodes')}>
              {(page) =>
                page.items.length === 0 ? (
                  <p className="ss-na">{t('admin.graph.noOpenCodes')}</p>
                ) : (
                  <ul className="ss-list" aria-label={t('admin.graph.openCodes')}>
                    {page.items.map((i) => (
                      <li key={i.id} className="ss-row">
                        <span className="adm-mono">{i.code}</span>
                      </li>
                    ))}
                  </ul>
                )
              }
            </Async>
            <h3>{t('admin.accounts.creditsTitle')}</h3>
            <Async res={credits} label={t('admin.graph.loadingCredits')}>
              {(c) =>
                c.entries.length === 0 ? (
                  <p className="ss-na">{t('admin.accounts.noCreditHistory')}</p>
                ) : (
                  <ul className="ss-list" aria-label={t('admin.accounts.creditLedger')}>
                    {c.entries.slice(0, 8).map((e) => (
                      <li key={e.id} className="ss-row">
                        <div className="ss-grow">
                          <div className="ss-name">{e.delta > 0 ? `+${e.delta}` : e.delta}</div>
                          <div className="ss-meta">
                            {e.reason || t('admin.accounts.noReason')} · {fmtMs(e.created_at_ms)}
                          </div>
                        </div>
                      </li>
                    ))}
                  </ul>
                )
              }
            </Async>
            <p className="ss-sub">{t('admin.graph.freezeGap')}</p>
            <ReasonDialog
              open={pending === 'credits'}
              title={t('admin.accounts.creditsDialogTitle', { username: d.account.username })}
              description={t('admin.accounts.creditsDialogDescription')}
              confirmLabel={t('admin.accounts.apply')}
              valid={Number.isInteger(deltaNum) && deltaNum !== 0}
              onCancel={() => setPending(null)}
              onConfirm={async (reason) => {
                const r = await api.adjustCredits(accountId, deltaNum, reason);
                await done(t('admin.accounts.balanceNow', { balance: r.balance }));
              }}
            >
              <TextInput
                label={t('admin.accounts.change')}
                type="number"
                value={delta}
                onChange={(e) => setDelta(e.target.value)}
              />
            </ReasonDialog>
            <ReasonDialog
              open={pending === 'freeze'}
              danger
              title={t('admin.accountExtras.freezeDialog', { username: d.account.username })}
              description={t('admin.accountExtras.freezeDescription')}
              confirmLabel={t('admin.accountExtras.freeze')}
              onCancel={() => setPending(null)}
              onConfirm={async (reason) => {
                await api.freezeInvites(accountId, reason);
                await done(t('admin.accountExtras.froze', { username: d.account.username }));
              }}
            />
            <ReasonDialog
              open={pending === 'unfreeze'}
              title={t('admin.accountExtras.unfreezeDialog', { username: d.account.username })}
              description={t('admin.accountExtras.unfreezeDescription')}
              confirmLabel={t('admin.accountExtras.unfreeze')}
              onCancel={() => setPending(null)}
              onConfirm={async (reason) => {
                await api.unfreezeInvites(accountId, reason);
                await done(t('admin.accountExtras.unfroze', { username: d.account.username }));
              }}
            />
            <ReasonDialog
              open={pending === 'clear'}
              title={t('admin.invites.clearSuspicion')}
              description={t('admin.invites.clearDescription')}
              confirmLabel={t('admin.invites.clearSuspicion')}
              onCancel={() => setPending(null)}
              onConfirm={async (reason) => {
                await api.clearSuspicion(d.account.person_id, reason);
                await done(t('admin.invites.cleared'));
              }}
            />
          </>
        )}
      </Async>
    </aside>
  );
}
