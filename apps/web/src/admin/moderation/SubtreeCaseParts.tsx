// The pieces of the subtree-case screen (MOD-GRD-001): a node row with its buttons, the request list with the
// second approver's and the Operator's buttons, the cross-case waiting list and the appeals of restrictions.
import { Badge, Button, Notice } from '@sideroom/ui';
import { useState } from 'react';
import { formatNumber, useT, type MessageKey } from '../../i18n';
import * as g from '../../moderation/graduated';
import { ApiProblem, ReasonDialog, fmtMs, useAdminViewer, useResource } from '../ui';

export const STATUS_KEYS: Record<g.NodeStatus, MessageKey> = {
  auto_frozen: 'admin.moderation.grd.status.autoFrozen',
  review: 'admin.moderation.grd.status.review',
  frozen: 'admin.moderation.grd.status.frozen',
  banned: 'admin.moderation.grd.status.banned',
  sterilized: 'admin.moderation.grd.status.sterilized',
  cleared: 'admin.moderation.grd.status.cleared',
  removed: 'admin.moderation.grd.status.removed',
  not_active: 'admin.moderation.grd.status.notActive',
};

export const ROLE_KEYS: Record<g.SubtreeNode['role'], MessageKey> = {
  root: 'admin.moderation.grd.role.root',
  descendant: 'admin.moderation.grd.role.descendant',
  inviter: 'admin.moderation.grd.role.inviter',
  inviter2: 'admin.moderation.grd.role.inviter2',
};

export type NodeAct =
  'clear' | 'freeze' | 'ban' | 'sterilize' | 'warn' | 'timeout' | 'inviteFreeze' | 'revoke';

export function NodeRow({
  node: n,
  closed,
  onAct,
}: {
  node: g.SubtreeNode;
  closed: boolean;
  onAct: (kind: NodeAct) => void;
}) {
  const t = useT();
  const live =
    !closed && n.role !== 'root' && !['banned', 'removed', 'not_active'].includes(n.status);
  const variant =
    n.status === 'auto_frozen' || n.status === 'frozen'
      ? 'warning'
      : n.status === 'banned'
        ? 'danger'
        : n.status === 'review'
          ? 'accent'
          : 'neutral';
  return (
    <tr>
      <td>
        @{n.username}
        {n.parent_account_id && (
          <div className="ss-meta">
            {t('admin.moderation.grd.invitedBy', { id: n.parent_account_id })}
          </div>
        )}
      </td>
      <td>
        {t(ROLE_KEYS[n.role])}
        {n.depth !== 0 && <span className="ss-meta"> ({n.depth})</span>}
      </td>
      <td>{fmtMs(n.registered_at_ms)}</td>
      <td>
        {t('admin.moderation.grd.activity', {
          messages: formatNumber(n.messages),
          voice: formatNumber(n.voice_seconds),
        })}
        {n.unactivated && <> · {t('admin.moderation.grd.unactivated')}</>}
      </td>
      <td>
        <Badge variant={variant}>{t(STATUS_KEYS[n.status])}</Badge>
        {n.note && <div className="ss-meta">{n.note}</div>}
      </td>
      <td>
        {live && (
          <div className="adm-actions">
            {(n.status === 'auto_frozen' || n.status === 'frozen' || n.status === 'review') && (
              <Button size="sm" variant="secondary" onClick={() => onAct('clear')}>
                {t('admin.moderation.grd.clear')}
              </Button>
            )}
            {(n.status === 'review' || n.status === 'cleared') && (
              <Button size="sm" variant="secondary" onClick={() => onAct('freeze')}>
                {t('admin.moderation.grd.freeze')}
              </Button>
            )}
            <Button size="sm" variant="danger" onClick={() => onAct('ban')}>
              {t('admin.moderation.grd.ban')}
            </Button>
            {n.status !== 'sterilized' && (
              <Button size="sm" variant="secondary" onClick={() => onAct('sterilize')}>
                {t('admin.moderation.grd.sterilize')}
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={() => onAct('warn')}>
              {t('admin.moderation.grd.warn')}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => onAct('timeout')}>
              {t('admin.moderation.grd.timeout')}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => onAct('inviteFreeze')}>
              {t('admin.moderation.grd.inviteFreeze')}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => onAct('revoke')}>
              {t('admin.moderation.grd.revokeCodes')}
            </Button>
          </div>
        )}
      </td>
    </tr>
  );
}

const REQUEST_STATE_KEYS: Record<g.ModRequest['state'], MessageKey> = {
  pending: 'admin.moderation.grd.req.pending',
  approved: 'admin.moderation.grd.req.approved',
  executing: 'admin.moderation.grd.req.executing',
  executed: 'admin.moderation.grd.req.executed',
  rejected: 'admin.moderation.grd.req.rejected',
  cancelled: 'admin.moderation.grd.req.cancelled',
};

/** Requests of one case: approve (a second staff member, behind the step-up), reject, and the Operator's execute. */
export function RequestList({
  requests,
  onChanged,
}: {
  requests: g.ModRequest[];
  onChanged: () => void;
}) {
  const t = useT();
  const viewer = useAdminViewer();
  const [error, setError] = useState<string | null>(null);
  const [reject, setReject] = useState<g.ModRequest | null>(null);
  const [outcome, setOutcome] = useState<g.BranchOutcome | null>(null);
  const guard = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
      onChanged();
    } catch (e) {
      setError(e instanceof Error ? e.message : t('admin.moderation.case.failed'));
    }
  };
  return (
    <>
      <h4>{t('admin.moderation.grd.requestsTitle')}</h4>
      <ApiProblem error={error} />
      {outcome && (
        <Notice variant="success" role="status">
          {t('admin.moderation.grd.branchDone', {
            n: outcome.nodes_actioned,
            body: outcome.notice.body,
          })}
        </Notice>
      )}
      <ul className="ss-list" aria-label={t('admin.moderation.grd.requestsTitle')}>
        {requests.map((r) => (
          <li key={r.id} className="ss-row">
            <div className="ss-grow">
              <div className="ss-name">
                <Badge
                  variant={r.state === 'pending' || r.state === 'approved' ? 'warning' : 'neutral'}
                >
                  {t(REQUEST_STATE_KEYS[r.state])}
                </Badge>{' '}
                {r.kind === 'sterilize'
                  ? t('admin.moderation.grd.reqSterilize')
                  : t('admin.moderation.grd.reqBranch', { action: r.action ?? '' })}
              </div>
              <div className="ss-meta">{r.reason}</div>
            </div>
            {r.state === 'pending' && r.proposed_by !== viewer.userId && (
              <Button size="sm" onClick={() => void guard(() => g.approveRequest(r.id))}>
                {t('admin.moderation.grd.approve')}
              </Button>
            )}
            {(r.state === 'pending' || r.state === 'approved') && (
              <Button size="sm" variant="ghost" onClick={() => setReject(r)}>
                {r.proposed_by === viewer.userId
                  ? t('admin.moderation.grd.withdraw')
                  : t('admin.moderation.grd.reject')}
              </Button>
            )}
            {r.kind === 'branch_action' && r.state === 'approved' && viewer.isOwner && (
              <Button
                size="sm"
                variant="danger"
                onClick={() => void guard(async () => setOutcome(await g.branchAction(r.id)))}
              >
                {t('admin.moderation.grd.execute')}
              </Button>
            )}
          </li>
        ))}
      </ul>
      <ReasonDialog
        open={reject !== null}
        title={t('admin.moderation.grd.reject')}
        confirmLabel={t('admin.moderation.grd.confirm')}
        onConfirm={async (reason) => {
          if (reject) await g.rejectRequest(reject.id, reason);
          setReject(null);
          onChanged();
        }}
        onCancel={() => setReject(null)}
      />
    </>
  );
}

/** Every request waiting for a second staff member or the Operator, across cases. */
export function RequestsPanel({ onOpenCase }: { onOpenCase: (id: string) => void }) {
  const t = useT();
  const res = useResource('grd-requests', () => g.listRequests());
  const waiting = (res.data ?? []).filter((r) => r.state === 'pending' || r.state === 'approved');
  if (waiting.length === 0) return <ApiProblem error={res.error} />;
  return (
    <>
      <h4>{t('admin.moderation.grd.waitingTitle')}</h4>
      <ul className="ss-list" aria-label={t('admin.moderation.grd.waitingTitle')}>
        {waiting.map((r) => (
          <li key={r.id} className="ss-row">
            <div className="ss-grow">
              <div className="ss-name">
                {r.kind === 'sterilize'
                  ? t('admin.moderation.grd.reqSterilize')
                  : t('admin.moderation.grd.reqBranch', { action: r.action ?? '' })}
              </div>
              <div className="ss-meta">{r.reason}</div>
            </div>
            <Button size="sm" variant="secondary" onClick={() => onOpenCase(r.case_id)}>
              {t('admin.moderation.grd.openCase')}
            </Button>
          </li>
        ))}
      </ul>
    </>
  );
}

/** Appeals of restrictions (a freeze, a sterilization, a timeout, a warning): decided by someone other than the imposer. */
export function RestrictionAppeals() {
  const t = useT();
  const res = useResource('grd-appeals', () => g.restrictionAppeals());
  const [target, setTarget] = useState<{
    r: g.RestrictionView;
    decision: 'upheld' | 'overturned';
  } | null>(null);
  const items = res.data ?? [];
  if (items.length === 0) return <ApiProblem error={res.error} />;
  return (
    <>
      <h4>{t('admin.moderation.grd.appealsTitle')}</h4>
      <ul className="ss-list" aria-label={t('admin.moderation.grd.appealsTitle')}>
        {items.map((r) => (
          <li key={r.id} className="ss-row">
            <div className="ss-grow">
              <div className="ss-name">{r.kind.replace(/_/g, ' ')}</div>
              <div className="ss-meta">{r.appeal_message}</div>
            </div>
            <Button size="sm" onClick={() => setTarget({ r, decision: 'overturned' })}>
              {t('admin.moderation.grd.overturn')}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setTarget({ r, decision: 'upheld' })}>
              {t('admin.moderation.grd.uphold')}
            </Button>
          </li>
        ))}
      </ul>
      <ReasonDialog
        open={target !== null}
        title={t('admin.moderation.grd.appealsTitle')}
        confirmLabel={t('admin.moderation.grd.confirm')}
        onConfirm={async (reason) => {
          if (target) await g.decideRestrictionAppeal(target.r.id, target.decision, reason);
          setTarget(null);
          await res.reload();
        }}
        onCancel={() => setTarget(null)}
      />
    </>
  );
}
