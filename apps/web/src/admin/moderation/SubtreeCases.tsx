// Admin > Moderation > Subtree cases (MOD-GRD-001, D-395): the case a serious ban opens on the invite tree, every
// node with its activity stats and lineage, the per-node decisions (clear, freeze, ban), the per-member acts
// (warn, timeout, invite freeze, revoke unused codes), the two-person requests (sterilization, the Operator's
// branch action) and the appeals of restrictions. The API decides what each viewer may do; the Operator's
// buttons are only shown to the Owner.
import { Badge, Button, EmptyState, Notice, Select, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT, type MessageKey } from '../../i18n';
import * as g from '../../moderation/graduated';
import { CATEGORY_KEYS } from '../../moderation/ReportDialog';
import { REPORT_CATEGORIES, type ReportCategory } from '../../moderation/api';
import { ApiProblem, ReasonDialog, fmtMs, useAdminViewer, useResource } from '../ui';
import {
  NodeRow,
  RequestList,
  RequestsPanel,
  RestrictionAppeals,
  type NodeAct,
} from './SubtreeCaseParts';

type Act = { kind: NodeAct; node: g.SubtreeNode } | { kind: 'branch' };

const catLabel = (t: ReturnType<typeof useT>, c: string) =>
  (REPORT_CATEGORIES as string[]).includes(c) ? t(CATEGORY_KEYS[c as ReportCategory]) : c;

export function SubtreeCasesTab() {
  const t = useT();
  const [openOnly, setOpenOnly] = useState(true);
  const [caseId, setCaseId] = useState<string | null>(null);
  const list = useResource(`grd-cases:${openOnly}`, () => g.listCases(openOnly));
  if (caseId)
    return (
      <>
        <Button variant="ghost" onClick={() => setCaseId(null)}>
          {t('admin.moderation.grd.back')}
        </Button>
        <CaseDetail id={caseId} />
      </>
    );
  return (
    <>
      <div className="ss-toolbar">
        <Switch
          label={t('admin.moderation.grd.onlyOpen')}
          checked={openOnly}
          onChange={(e) => setOpenOnly(e.target.checked)}
        />
      </div>
      <ApiProblem error={list.error} />
      {list.data && list.data.length === 0 && (
        <EmptyState
          title={t('admin.moderation.grd.emptyTitle')}
          description={t('admin.moderation.grd.emptyDescription')}
        />
      )}
      {list.data && list.data.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.moderation.grd.listLabel')}>
          {list.data.map((c) => (
            <li key={c.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  <Badge variant={c.state === 'open' ? 'warning' : 'neutral'}>
                    {c.state === 'open'
                      ? t('admin.moderation.grd.open')
                      : t('admin.moderation.grd.closed')}
                  </Badge>{' '}
                  {catLabel(t, c.category)} · @{c.root_username}
                </div>
                <div className="ss-meta">
                  {t('admin.moderation.grd.counts', {
                    nodes: c.nodes,
                    frozen: c.auto_frozen,
                    review: c.waiting_review,
                  })}{' '}
                  · {fmtMs(c.opened_at_ms)}
                </div>
              </div>
              <Button variant="secondary" onClick={() => setCaseId(c.id)}>
                {t('admin.moderation.grd.openCase')}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <RequestsPanel onOpenCase={setCaseId} />
      <RestrictionAppeals />
    </>
  );
}

function CaseDetail({ id }: { id: string }) {
  const t = useT();
  const viewer = useAdminViewer();
  const res = useResource(`grd-case:${id}`, () => g.getCase(id));
  const [act, setAct] = useState<Act | null>(null);
  const [closeOpen, setCloseOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  // Dialog fields.
  const [category, setCategory] = useState<ReportCategory>('other');
  const [minutes, setMinutes] = useState('60');
  const [days, setDays] = useState('14');
  const [branch, setBranch] = useState<'freeze' | 'ban'>('freeze');
  const done = async (message: string) => {
    setNotice(message);
    await res.reload();
  };
  const d = res.data;

  const run = async (reason: string) => {
    if (!act || !d) return;
    if (act.kind === 'branch') {
      await g.requestBranchAction(id, branch, reason);
      return done(t('admin.moderation.grd.requested'));
    }
    const acc = act.node.account_id;
    switch (act.kind) {
      case 'clear':
        await g.clearNode(id, acc, reason);
        return done(t('admin.moderation.grd.cleared'));
      case 'freeze':
        await g.freezeNode(id, acc, reason);
        return done(t('admin.moderation.grd.frozen'));
      case 'ban':
        await g.banNode(id, acc, category, reason);
        return done(t('admin.moderation.grd.banned'));
      case 'sterilize':
        await g.requestSterilization(id, acc, reason);
        return done(t('admin.moderation.grd.requested'));
      case 'warn':
        await g.warnMember(acc, reason);
        return done(t('admin.moderation.grd.warned'));
      case 'timeout':
        await g.timeoutMember(acc, Number(minutes), reason);
        return done(t('admin.moderation.grd.timedOut'));
      case 'inviteFreeze':
        await g.freezeInvites(acc, Number(days), reason);
        return done(t('admin.moderation.grd.invitesFrozen'));
      case 'revoke': {
        const r = await g.revokeInvites(acc, reason);
        return done(t('admin.moderation.grd.revoked', { n: r.codes_revoked }));
      }
    }
  };

  const titleKey: Record<Act['kind'], MessageKey> = {
    clear: 'admin.moderation.grd.act.clear',
    freeze: 'admin.moderation.grd.act.freeze',
    ban: 'admin.moderation.grd.act.ban',
    sterilize: 'admin.moderation.grd.act.sterilize',
    warn: 'admin.moderation.grd.act.warn',
    timeout: 'admin.moderation.grd.act.timeout',
    inviteFreeze: 'admin.moderation.grd.act.inviteFreeze',
    revoke: 'admin.moderation.grd.act.revoke',
    branch: 'admin.moderation.grd.act.branch',
  };
  const needsLong = act?.kind === 'sterilize' || act?.kind === 'branch';
  const validExtras =
    act?.kind === 'timeout'
      ? Number(minutes) >= 1 && Number(minutes) <= 40320
      : act?.kind === 'inviteFreeze'
        ? Number(days) >= 1 && Number(days) <= 365
        : true;

  return (
    <>
      <ApiProblem error={res.error} />
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      {d && (
        <>
          <h3>
            {catLabel(t, d.case.category)} · @{d.case.root_username}
          </h3>
          <p className="ss-meta">
            {t('admin.moderation.grd.counts', {
              nodes: d.case.nodes,
              frozen: d.case.auto_frozen,
              review: d.case.waiting_review,
            })}
          </p>
          <p className="ss-meta">{t('admin.moderation.grd.rule')}</p>
          {d.nodes_truncated && (
            <Notice variant="warning">{t('admin.moderation.grd.truncated')}</Notice>
          )}
          <div className="adm-table-wrap">
            <table className="adm-table">
              <thead>
                <tr>
                  <th>{t('admin.moderation.grd.col.account')}</th>
                  <th>{t('admin.moderation.grd.col.role')}</th>
                  <th>{t('admin.moderation.grd.col.registered')}</th>
                  <th>{t('admin.moderation.grd.col.activity')}</th>
                  <th>{t('admin.moderation.grd.col.status')}</th>
                  <th>{t('admin.moderation.grd.col.actions')}</th>
                </tr>
              </thead>
              <tbody>
                {d.nodes.map((n) => (
                  <NodeRow
                    key={n.account_id}
                    node={n}
                    closed={d.case.state === 'closed'}
                    onAct={(kind) => setAct({ kind, node: n } as Act)}
                  />
                ))}
              </tbody>
            </table>
          </div>
          {d.case.state === 'open' && (
            <div className="adm-actions">
              <Button variant="secondary" onClick={() => setCloseOpen(true)}>
                {t('admin.moderation.grd.closeCase')}
              </Button>
              {viewer.isOwner && (
                <Button variant="danger" onClick={() => setAct({ kind: 'branch' })}>
                  {t('admin.moderation.grd.branchButton')}
                </Button>
              )}
            </div>
          )}
          {d.requests.length > 0 && (
            <RequestList requests={d.requests} onChanged={() => void res.reload()} />
          )}
        </>
      )}
      <ReasonDialog
        open={act !== null}
        title={act ? t(titleKey[act.kind]) : ''}
        description={
          needsLong ? t('admin.moderation.grd.needsApproval') : t('admin.moderation.grd.noPenalty')
        }
        confirmLabel={t('admin.moderation.grd.confirm')}
        danger={act?.kind === 'ban' || act?.kind === 'branch'}
        valid={validExtras}
        onConfirm={async (reason) => {
          await run(reason);
          setAct(null);
        }}
        onCancel={() => setAct(null)}
      >
        {act?.kind === 'ban' && (
          <Select
            aria-label={t('admin.accounts.banReasonLabel')}
            options={REPORT_CATEGORIES.map((c) => ({ value: c, label: t(CATEGORY_KEYS[c]) }))}
            value={category}
            onValueChange={(v) => setCategory(v as ReportCategory)}
          />
        )}
        {act?.kind === 'timeout' && (
          <TextInput
            label={t('admin.moderation.grd.minutes')}
            type="number"
            value={minutes}
            onChange={(e) => setMinutes(e.target.value)}
          />
        )}
        {act?.kind === 'inviteFreeze' && (
          <TextInput
            label={t('admin.moderation.grd.days')}
            type="number"
            value={days}
            onChange={(e) => setDays(e.target.value)}
          />
        )}
        {act?.kind === 'branch' && (
          <Select
            aria-label={t('admin.moderation.grd.branchAction')}
            options={[
              { value: 'freeze', label: t('admin.moderation.grd.branchFreeze') },
              { value: 'ban', label: t('admin.moderation.grd.branchBan') },
            ]}
            value={branch}
            onValueChange={(v) => setBranch(v as 'freeze' | 'ban')}
          />
        )}
      </ReasonDialog>
      <ReasonDialog
        open={closeOpen}
        title={t('admin.moderation.grd.closeCase')}
        confirmLabel={t('admin.moderation.grd.confirm')}
        onConfirm={async (reason) => {
          await g.closeCase(id, reason);
          setCloseOpen(false);
          await done(t('admin.moderation.grd.closedNotice'));
        }}
        onCancel={() => setCloseOpen(false)}
      />
    </>
  );
}
