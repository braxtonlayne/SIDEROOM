// One case (MOD-CASE-012; spec 9.3): reports, evidence snapshots, notes, holds with release, the appeal, the
// timeline, and the actions the viewer's access allows. The server decides what is shown (the reporter is hidden
// from viewers who may not see it); this page only hides what would certainly be refused.
import { Badge, Button, Notice, Select, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT, type MessageKey } from '../../i18n';
import { CATEGORY_KEYS } from '../../moderation/ReportDialog';
import * as api from '../../moderation/api';
import { BanReasonFields } from './BanReasonFields';
import { TakedownBadge } from './TakedownBadge';
import { Link } from '@tanstack/react-router';
import { Async, FormDialog, ReasonDialog, fmtMs, useAdminViewer, useResource } from '../ui';
import {
  EVENT_KEYS,
  MOVES,
  RECORDABLE_ACTIONS,
  ROUTING_KEYS,
  SEVERITY_KEYS,
  STATE_KEYS,
  TARGET_KEYS,
} from './labels';

type Dialog =
  | { kind: 'move'; to: api.CaseState }
  | { kind: 'close' }
  | { kind: 'reopen' }
  | { kind: 'action' }
  | { kind: 'perform' }
  | { kind: 'age' }
  | { kind: 'warn' }
  | { kind: 'hold' }
  | { kind: 'ban' }
  | { kind: 'release'; hold: api.HoldView }
  | { kind: 'appeal'; decision: 'upheld' | 'overturned' };

const MOVE_KEYS: Record<api.CaseState, MessageKey> = {
  open: 'admin.moderation.case.moveOpen',
  triage: 'admin.moderation.case.moveTriage',
  investigating: 'admin.moderation.case.moveInvestigating',
  actioned: 'admin.moderation.case.moveActioned',
  dismissed: 'admin.moderation.case.moveDismissed',
  closed: 'admin.moderation.case.moveClosed',
};

/** The attachments the evidence snapshots show on the reported message, once each. */
function targetAttachments(d: api.CaseDetail): { id: string; filename: string }[] {
  const seen = new Map<string, string>();
  for (const ev of d.evidence)
    for (const m of ev.payload.messages ?? [])
      if (m.target) for (const a of m.attachments) seen.set(a.id, a.filename);
  return [...seen].map(([id, filename]) => ({ id, filename }));
}

/** What can be done from this case: the reported message, the account in the case's server, a suspension. */
function performable(d: api.CaseDetail, manage: boolean): api.PerformedAction[] {
  const out: api.PerformedAction[] = [];
  if (d.case.target_kind === 'message' && d.case.target_id) {
    out.push('delete_message', 'restore_message');
    if (targetAttachments(d).length > 0) out.push('remove_attachment');
  }
  if (d.case.guild_id && d.subject_account_id) out.push('timeout', 'kick', 'guild_ban');
  if (manage && d.subject_account_id) out.push('suspend_account');
  return out;
}

/** What staff can do about age restriction from this case (MOD-AGE-001 to -003); platform-routed cases only. */
type AgeOp =
  | 'restrict_channel'
  | 'unrestrict_channel'
  | 'restrict_series'
  | 'unrestrict_series'
  | 'restrict_title'
  | 'unrestrict_title'
  | 'under_18'
  | 'under_13';

function ageOps(d: api.CaseDetail, manage: boolean): AgeOp[] {
  if (d.case.routing === 'guild') return [];
  const out: AgeOp[] = [];
  const channel = d.case.target_kind === 'channel' ? d.case.target_id : d.case.target_channel_id;
  if (channel) out.push('restrict_channel', 'unrestrict_channel');
  if (d.case.target_kind === 'library' && d.case.target_id)
    out.push('restrict_series', 'unrestrict_series');
  if (d.case.target_kind === 'cinema' && d.case.target_id)
    out.push('restrict_title', 'unrestrict_title');
  if (manage && d.subject_account_id) out.push('under_18', 'under_13');
  return out;
}

const AGE_KEYS: Record<AgeOp, MessageKey> = {
  restrict_channel: 'admin.moderation.age.restrictChannel',
  unrestrict_channel: 'admin.moderation.age.unrestrictChannel',
  restrict_series: 'admin.moderation.age.restrictSeries',
  unrestrict_series: 'admin.moderation.age.unrestrictSeries',
  restrict_title: 'admin.moderation.age.restrictTitle',
  unrestrict_title: 'admin.moderation.age.unrestrictTitle',
  under_18: 'admin.moderation.age.under18',
  under_13: 'admin.moderation.age.under13',
};

const PERFORM_KEYS: Record<api.PerformedAction, MessageKey> = {
  delete_message: 'admin.moderation.perform.deleteMessage',
  restore_message: 'admin.moderation.perform.restoreMessage',
  remove_attachment: 'admin.moderation.perform.removeAttachment',
  timeout: 'admin.moderation.perform.timeout',
  kick: 'admin.moderation.perform.kick',
  guild_ban: 'admin.moderation.perform.guildBan',
  suspend_account: 'admin.moderation.perform.suspendAccount',
};

const HOLD_KINDS: { id: api.HoldKind; key: MessageKey }[] = [
  { id: 'message', key: 'admin.moderation.hold.kindMessage' },
  { id: 'attachment', key: 'admin.moderation.hold.kindAttachment' },
  { id: 'submission', key: 'admin.moderation.hold.kindSubmission' },
  { id: 'user_messages', key: 'admin.moderation.hold.kindUserMessages' },
];

function EventLine({ e }: { e: api.CaseEvent }) {
  const t = useT();
  const viewer = useAdminViewer();
  const d = (k: string) => {
    const v = e.detail[k];
    return v === undefined || v === null ? '' : String(v);
  };
  const state = (v: string) => (v in STATE_KEYS ? t(STATE_KEYS[v as api.CaseState]) : v);
  const key = EVENT_KEYS[e.kind];
  const text = key
    ? t(key, {
        from: state(d('from')),
        to: state(d('to')),
        action: d('action'),
        note: d('note'),
        reason: d('reason'),
        decision: d('decision'),
        category: d('category') || d('reason_category'),
        messages: d('messages'),
        attachments: d('attachments'),
        submissions: d('submissions'),
        reactions: d('reactions'),
        votes: d('poll_votes'),
        level: d('level'),
        label: d('label'),
        restricted: d('restricted'),
        band: d('band'),
        basis: d('basis'),
      })
    : e.kind;
  return (
    <li className="ss-row">
      <div className="ss-grow">
        <div className="ss-name">
          {text}
          {e.kind === 'underage_handled' &&
            Boolean(e.detail['erasure_request_id']) &&
            viewer.can('MANAGE_TAKEDOWNS') && (
              <>
                {' '}
                <Link to="/admin/$section" params={{ section: 'data-requests' }}>
                  {t('admin.moderation.event.erasureRequestLink', {
                    id: String(e.detail['erasure_request_id']),
                  })}
                </Link>
              </>
            )}
        </div>
        <div className="ss-meta">
          {fmtMs(e.created_at_ms)} ·{' '}
          {e.actor_id
            ? t('admin.moderation.case.byAccount', { id: e.actor_id })
            : t('admin.moderation.case.bySystem')}
        </div>
      </div>
    </li>
  );
}

function Evidence({ ev }: { ev: api.CaseEvidence }) {
  const t = useT();
  const msgs = ev.payload.messages;
  if (ev.kind === 'evasion_signal')
    return (
      <p>
        {t('admin.suspicion.evidence', {
          kind: String(ev.payload.kind ?? ''),
          n: Number(ev.payload.shared_addresses ?? 0),
          days: Number(ev.payload.window_days ?? 0),
        })}
      </p>
    );
  if (!msgs)
    return (
      <p>
        {t('admin.moderation.case.summaryTarget', {
          kind: String(ev.payload.kind ?? ''),
          label: String(ev.payload.label ?? ''),
        })}
      </p>
    );
  return (
    <>
      <p className="ss-meta">{t('admin.moderation.case.snapshotNote')}</p>
      <ol className="ss-list" aria-label={t('admin.moderation.case.snapshotLabel')}>
        {msgs.map((m) => (
          <li key={m.id} className="ss-row" aria-current={m.target ? 'true' : undefined}>
            <div className="ss-grow">
              <div className="ss-name">
                {m.author_username ?? m.author_id}{' '}
                {m.target && (
                  <Badge variant="danger">{t('admin.moderation.case.reportedMessage')}</Badge>
                )}
              </div>
              <div>{m.content || t('admin.moderation.case.noText')}</div>
              {m.attachments.length > 0 && (
                <div className="ss-meta">{m.attachments.map((a) => a.filename).join(', ')}</div>
              )}
              <div className="ss-meta">{fmtMs(m.created_at_ms)}</div>
            </div>
          </li>
        ))}
      </ol>
    </>
  );
}

export function CasePage({
  id,
  onBack,
  onOpenNotice,
}: {
  id: string;
  onBack: () => void;
  onOpenNotice?: ((noticeId: string) => void) | undefined;
}) {
  const t = useT();
  const res = useResource(`case:${id}`, () => api.getCase(id));
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  // A restore refused because a takedown notice holds the message (D-337): the text, and the notice to open.
  const [refusal, setRefusal] = useState<{ text: string; noticeId: string } | null>(null);
  const [note, setNote] = useState('');

  const run = async (fn: () => Promise<unknown>, done?: string) => {
    setError(null);
    setRefusal(null);
    try {
      await fn();
      if (done) setNotice(done);
      await res.reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : t('admin.moderation.case.failed'));
    }
  };

  return (
    <>
      <div className="ss-toolbar">
        <Button variant="ghost" onClick={onBack}>
          {t('admin.moderation.case.back')}
        </Button>
      </div>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      {error && (
        <Notice variant="danger" role="alert" className="ss-status">
          {error}
        </Notice>
      )}
      {refusal && (
        <Notice variant="danger" role="alert" className="ss-status">
          {refusal.text}{' '}
          {onOpenNotice && (
            <Button size="sm" variant="ghost" onClick={() => onOpenNotice(refusal.noticeId)}>
              {t('admin.moderation.takedownBadge.open', { id: refusal.noticeId })}
            </Button>
          )}
        </Notice>
      )}
      <Async res={res}>
        {(d) => {
          const c = d.case;
          const work = d.access !== 'read';
          const manage = d.access === 'manage';
          const moves = MOVES[c.state];
          return (
            <>
              <h2 className="ss-title">{t('admin.moderation.case.title', { id: c.id })}</h2>
              {c.takedown && (
                <p>
                  <TakedownBadge takedown={c.takedown} onOpen={onOpenNotice} />
                </p>
              )}
              <p className="ss-meta">
                <Badge variant={c.severity === 'serious' ? 'danger' : 'neutral'}>
                  {t(SEVERITY_KEYS[c.severity])}
                </Badge>{' '}
                <Badge variant={c.state === 'open' ? 'accent' : 'neutral'}>
                  {t(STATE_KEYS[c.state])}
                </Badge>{' '}
                {t(CATEGORY_KEYS[c.category])} · {t(TARGET_KEYS[c.target_kind])} ·{' '}
                {t(ROUTING_KEYS[c.routing])} ·{' '}
                {t('admin.moderation.queue.openedAt', { when: fmtMs(c.opened_at_ms) })}
                {c.assignee_id
                  ? ` · ${t('admin.moderation.queue.assignedTo', { id: c.assignee_id })}`
                  : ''}
              </p>
              {(d.subject_account_id || d.subject_person_id) && (
                <p className="ss-meta">
                  {t('admin.moderation.case.subject', {
                    account: d.subject_account_id ?? '-',
                    person: d.subject_person_id ?? '-',
                  })}
                </p>
              )}
              {d.outcome && <p>{t('admin.moderation.case.outcome', { outcome: d.outcome })}</p>}
              {d.linked_case_id && (
                <p className="ss-meta">
                  {t('admin.moderation.case.linked', { id: d.linked_case_id })}
                </p>
              )}

              {work && (
                <div
                  className="ss-toolbar"
                  role="group"
                  aria-label={t('admin.moderation.case.actions')}
                >
                  {c.assignee_id ? (
                    <Button variant="secondary" onClick={() => void run(() => api.releaseCase(id))}>
                      {t('admin.moderation.case.release')}
                    </Button>
                  ) : (
                    <Button variant="secondary" onClick={() => void run(() => api.claimCase(id))}>
                      {t('admin.moderation.case.claim')}
                    </Button>
                  )}
                  {moves.map((to) => (
                    <Button
                      key={to}
                      variant="secondary"
                      onClick={() => {
                        if (to === 'closed') setDialog({ kind: 'close' });
                        else if (to === 'actioned' || to === 'dismissed')
                          setDialog({ kind: 'move', to });
                        else void run(() => api.setCaseState(id, { to }));
                      }}
                    >
                      {t(MOVE_KEYS[to])}
                    </Button>
                  ))}
                  {c.state === 'closed' && (
                    <Button variant="secondary" onClick={() => setDialog({ kind: 'reopen' })}>
                      {t('admin.moderation.case.reopen')}
                    </Button>
                  )}
                  {performable(d, manage).length > 0 && (
                    <Button variant="secondary" onClick={() => setDialog({ kind: 'perform' })}>
                      {t('admin.moderation.perform.button')}
                    </Button>
                  )}
                  {ageOps(d, manage).length > 0 && (
                    <Button variant="secondary" onClick={() => setDialog({ kind: 'age' })}>
                      {t('admin.moderation.age.button')}
                    </Button>
                  )}
                  <Button variant="secondary" onClick={() => setDialog({ kind: 'action' })}>
                    {t('admin.moderation.case.recordAction')}
                  </Button>
                  {manage && (
                    <>
                      <Button variant="secondary" onClick={() => setDialog({ kind: 'warn' })}>
                        {t('admin.moderation.case.warn')}
                      </Button>
                      <Button variant="secondary" onClick={() => setDialog({ kind: 'hold' })}>
                        {t('admin.moderation.case.placeHold')}
                      </Button>
                      <Button
                        variant="danger"
                        disabled={!d.subject_person_id}
                        onClick={() => setDialog({ kind: 'ban' })}
                      >
                        {t('admin.moderation.case.ban')}
                      </Button>
                    </>
                  )}
                </div>
              )}

              <h3 className="ss-subtitle">{t('admin.moderation.case.reportsHeading')}</h3>
              <ul className="ss-list" aria-label={t('admin.moderation.case.reportsHeading')}>
                {d.reports.map((r) => (
                  <li key={r.id} className="ss-row">
                    <div className="ss-grow">
                      <div className="ss-name">
                        {t(CATEGORY_KEYS[r.category])} ·{' '}
                        {r.reporter
                          ? t('admin.moderation.case.reporter', { name: r.reporter.username })
                          : t('admin.moderation.case.reporterHidden')}
                      </div>
                      <div>{r.note || t('admin.moderation.case.noNote')}</div>
                      <div className="ss-meta">{fmtMs(r.created_at_ms)}</div>
                    </div>
                  </li>
                ))}
              </ul>

              <h3 className="ss-subtitle">{t('admin.moderation.case.evidenceHeading')}</h3>
              {d.evidence.length === 0 && <p>{t('admin.moderation.case.noEvidence')}</p>}
              {d.evidence.map((ev) => (
                <Evidence key={ev.id} ev={ev} />
              ))}

              <h3 className="ss-subtitle">{t('admin.moderation.case.holdsHeading')}</h3>
              {d.holds.length === 0 && <p>{t('admin.moderation.case.noHolds')}</p>}
              <ul className="ss-list" aria-label={t('admin.moderation.case.holdsHeading')}>
                {d.holds.map((h) => (
                  <li key={h.id} className="ss-row">
                    <div className="ss-grow">
                      <div className="ss-name">
                        {h.released_at_ms ? (
                          <Badge>{t('admin.moderation.hold.released')}</Badge>
                        ) : (
                          <Badge variant="warning">{t('admin.moderation.hold.inForce')}</Badge>
                        )}{' '}
                        {h.scope_kind ?? ''} {h.scope_id ?? ''}
                        {h.review_due && (
                          <>
                            {' '}
                            <Badge variant="danger">{t('admin.moderation.hold.reviewDue')}</Badge>
                          </>
                        )}
                      </div>
                      <div className="ss-meta">
                        {h.reason} ·{' '}
                        {t('admin.moderation.hold.size', {
                          messages: h.messages,
                          attachments: h.attachments,
                          submissions: h.submissions,
                        })}
                        {h.release_reason
                          ? ` · ${t('admin.moderation.hold.releaseReason', { reason: h.release_reason })}`
                          : ''}
                      </div>
                    </div>
                    {manage && !h.released_at_ms && (
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={t('admin.moderation.hold.releaseAria', { id: h.id })}
                        onClick={() => setDialog({ kind: 'release', hold: h })}
                      >
                        {t('admin.moderation.hold.release')}
                      </Button>
                    )}
                  </li>
                ))}
              </ul>

              {d.appeal && (
                <>
                  <h3 className="ss-subtitle">{t('admin.moderation.case.appealHeading')}</h3>
                  <p>{d.appeal.message}</p>
                  <p className="ss-meta">
                    {t('admin.moderation.appeal.state', { state: d.appeal.state })} ·{' '}
                    {fmtMs(d.appeal.created_at_ms)}
                    {d.appeal.decision_reason ? ` · ${d.appeal.decision_reason}` : ''}
                  </p>
                  {manage && d.appeal.state === 'open' && (
                    <div className="ss-toolbar">
                      <Button
                        variant="secondary"
                        onClick={() => setDialog({ kind: 'appeal', decision: 'upheld' })}
                      >
                        {t('admin.moderation.appeal.uphold')}
                      </Button>
                      <Button
                        variant="danger"
                        onClick={() => setDialog({ kind: 'appeal', decision: 'overturned' })}
                      >
                        {t('admin.moderation.appeal.overturn')}
                      </Button>
                    </div>
                  )}
                </>
              )}

              <h3 className="ss-subtitle">{t('admin.moderation.case.notesHeading')}</h3>
              {d.notes.length === 0 && <p>{t('admin.moderation.case.noNotes')}</p>}
              <ul className="ss-list" aria-label={t('admin.moderation.case.notesHeading')}>
                {d.notes.map((n) => (
                  <li key={n.id} className="ss-row">
                    <div className="ss-grow">
                      <div>{n.body}</div>
                      <div className="ss-meta">
                        {fmtMs(n.created_at_ms)} ·{' '}
                        {t('admin.moderation.case.byAccount', { id: n.author_id })}
                      </div>
                    </div>
                  </li>
                ))}
              </ul>
              {work && (
                <form
                  className="adm-form"
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (!note.trim()) return;
                    void run(async () => {
                      await api.addCaseNote(id, note.trim());
                      setNote('');
                    });
                  }}
                >
                  <TextArea
                    label={t('admin.moderation.case.noteLabel')}
                    rows={3}
                    maxLength={4000}
                    value={note}
                    onChange={(e) => setNote(e.target.value)}
                  />
                  <div>
                    <Button type="submit" disabled={!note.trim()}>
                      {t('admin.moderation.case.noteSave')}
                    </Button>
                  </div>
                </form>
              )}

              <h3 className="ss-subtitle">{t('admin.moderation.case.timelineHeading')}</h3>
              <ol className="ss-list" aria-label={t('admin.moderation.case.timelineHeading')}>
                {d.events.map((e) => (
                  <EventLine key={e.id} e={e} />
                ))}
              </ol>

              <CaseDialogs
                id={id}
                data={d}
                dialog={dialog}
                close={() => setDialog(null)}
                run={run}
                onTakedownRefusal={(text, noticeId) => {
                  setDialog(null);
                  setNotice(null);
                  setRefusal({ text, noticeId });
                }}
              />
            </>
          );
        }}
      </Async>
    </>
  );
}

/** A case restore of a message a takedown holds answers 409 naming the notice, "TK-n" (D-337); that refusal
 * leaves the dialog and shows on the page with a way to the notice. Every other error stays in the dialog. */
async function takedownAware(
  onRefusal: (text: string, noticeId: string) => void,
  fn: () => Promise<void>,
): Promise<void> {
  try {
    await fn();
  } catch (e) {
    const text = e instanceof Error ? e.message : '';
    const m = /takedown notice TK-(\d+)/.exec(text);
    if (!m?.[1]) throw e;
    onRefusal(text, m[1]);
  }
}

function CaseDialogs({
  id,
  data,
  dialog,
  close,
  run,
  onTakedownRefusal,
}: {
  id: string;
  data: api.CaseDetail;
  dialog: Dialog | null;
  close: () => void;
  run: (fn: () => Promise<unknown>, done?: string) => Promise<void>;
  onTakedownRefusal: (text: string, noticeId: string) => void;
}) {
  const t = useT();
  const [outcome, setOutcome] = useState('');
  const [line, setLine] = useState('');
  const [action, setAction] = useState('timeout');
  const [performKind, setPerformKind] = useState<api.PerformedAction>(
    performable(data, data.access === 'manage')[0] ?? 'kick',
  );
  const [minutes, setMinutes] = useState('60');
  const ageChoices = ageOps(data, data.access === 'manage');
  const [ageOp, setAgeOp] = useState<AgeOp>(ageChoices[0] ?? 'restrict_channel');
  const attachments = targetAttachments(data);
  const [attachmentId, setAttachmentId] = useState(attachments[0]?.id ?? '');
  const [text, setText] = useState('');
  const [holdKind, setHoldKind] = useState<api.HoldKind>('message');
  const [holdId, setHoldId] = useState('');
  const [hours, setHours] = useState('24');
  const [banReason, setBanReason] = useState<api.ReportCategory>(data.case.category);
  const [banSeverity, setBanSeverity] = useState<api.Severity>(data.case.severity);
  const done = (fn: () => Promise<unknown>, msg?: string) => async () => {
    await fn();
    close();
    await run(async () => undefined, msg);
  };

  return (
    <>
      <ReasonDialog
        open={dialog?.kind === 'move'}
        title={t('admin.moderation.dialog.moveTitle')}
        confirmLabel={t('admin.moderation.dialog.moveConfirm')}
        reasonOptional
        reasonLabel={t('admin.moderation.dialog.outcomeLabel')}
        onCancel={close}
        onConfirm={async (reason) => {
          if (dialog?.kind !== 'move') return;
          await done(() =>
            api.setCaseState(id, { to: dialog.to, ...(reason ? { outcome: reason } : {}) }),
          )();
        }}
      />
      <FormDialog
        open={dialog?.kind === 'close'}
        title={t('admin.moderation.dialog.closeTitle')}
        confirmLabel={t('admin.moderation.dialog.closeConfirm')}
        valid={outcome.trim().length > 0}
        onCancel={close}
        onConfirm={done(
          () =>
            api.setCaseState(id, {
              to: 'closed',
              outcome: outcome.trim(),
              ...(line.trim() ? { reporter_line: line.trim() } : {}),
            }),
          t('admin.moderation.dialog.closedNotice'),
        )}
      >
        <TextInput
          label={t('admin.moderation.dialog.outcomeLabel')}
          value={outcome}
          maxLength={500}
          onChange={(e) => setOutcome(e.target.value)}
        />
        <TextInput
          label={t('admin.moderation.dialog.reporterLineLabel')}
          value={line}
          maxLength={200}
          onChange={(e) => setLine(e.target.value)}
        />
      </FormDialog>
      <ReasonDialog
        open={dialog?.kind === 'reopen'}
        title={t('admin.moderation.dialog.reopenTitle')}
        confirmLabel={t('admin.moderation.dialog.reopenConfirm')}
        onCancel={close}
        onConfirm={async (reason) => done(() => api.reopenCase(id, reason))()}
      />
      <ReasonDialog
        open={dialog?.kind === 'action'}
        title={t('admin.moderation.dialog.actionTitle')}
        description={t('admin.moderation.dialog.actionDescription')}
        confirmLabel={t('admin.moderation.dialog.actionConfirm')}
        reasonOptional
        reasonLabel={t('admin.moderation.dialog.noteLabel')}
        onCancel={close}
        onConfirm={async (reason) => done(() => api.recordCaseAction(id, action, reason))()}
      >
        <Select
          aria-label={t('admin.moderation.dialog.actionKind')}
          options={RECORDABLE_ACTIONS.map((a) => ({ value: a.id, label: t(a.key) }))}
          value={action}
          onValueChange={setAction}
        />
      </ReasonDialog>
      <ReasonDialog
        open={dialog?.kind === 'perform'}
        danger
        title={t('admin.moderation.perform.title')}
        description={t('admin.moderation.perform.description')}
        confirmLabel={t('admin.moderation.perform.confirm')}
        valid={
          (performKind !== 'timeout' || Number(minutes) >= 1) &&
          (performKind !== 'remove_attachment' || attachmentId !== '')
        }
        onCancel={close}
        onConfirm={async (reason) =>
          takedownAware(onTakedownRefusal, () =>
            done(
              () =>
                api.performCaseAction(
                  id,
                  performKind,
                  reason,
                  performKind === 'timeout' ? Number(minutes) : undefined,
                  performKind === 'remove_attachment' ? attachmentId : undefined,
                ),
              t('admin.moderation.perform.done'),
            )(),
          )
        }
      >
        <Select
          aria-label={t('admin.moderation.perform.kind')}
          options={performable(data, data.access === 'manage').map((a) => ({
            value: a,
            label: t(PERFORM_KEYS[a]),
          }))}
          value={performKind}
          onValueChange={(v) => setPerformKind(v as api.PerformedAction)}
        />
        {performKind === 'remove_attachment' && (
          <Select
            aria-label={t('admin.moderation.perform.attachment')}
            options={attachments.map((a) => ({ value: a.id, label: a.filename }))}
            value={attachmentId}
            onValueChange={setAttachmentId}
          />
        )}
        {performKind === 'timeout' && (
          <TextInput
            label={t('admin.moderation.perform.minutes')}
            value={minutes}
            inputMode="numeric"
            onChange={(e) => setMinutes(e.target.value.replace(/\D/g, ''))}
          />
        )}
      </ReasonDialog>
      <ReasonDialog
        open={dialog?.kind === 'age'}
        danger={ageOp === 'under_13'}
        title={t('admin.moderation.age.title')}
        description={
          ageOp === 'under_13'
            ? t('admin.moderation.age.descriptionUnder13')
            : ageOp === 'under_18'
              ? t('admin.moderation.age.descriptionUnder18')
              : t('admin.moderation.age.description')
        }
        confirmLabel={t('admin.moderation.age.confirm')}
        reasonLabel={t('admin.moderation.age.reason')}
        onCancel={close}
        onConfirm={async (reason) => {
          const restricted = ageOp.startsWith('restrict_');
          const channel =
            data.case.target_kind === 'channel' ? data.case.target_id : data.case.target_channel_id;
          await done(() => {
            if (ageOp === 'under_18' || ageOp === 'under_13')
              return api.handleUnderage({
                account_id: data.subject_account_id ?? '',
                band: ageOp,
                note: reason,
                case_id: id,
              });
            const kind: api.AgeTargetKind = ageOp.endsWith('channel')
              ? 'channel'
              : ageOp.endsWith('series')
                ? 'library_series'
                : 'cinema_title';
            return api.setAgeRestricted({
              kind,
              id: (kind === 'channel' ? channel : data.case.target_id) ?? '',
              restricted,
              reason,
              case_id: id,
            });
          }, t('admin.moderation.age.done'))();
        }}
      >
        <Select
          aria-label={t('admin.moderation.age.kind')}
          options={ageChoices.map((a) => ({ value: a, label: t(AGE_KEYS[a]) }))}
          value={ageOp}
          onValueChange={(v) => setAgeOp(v as AgeOp)}
        />
      </ReasonDialog>
      <FormDialog
        open={dialog?.kind === 'warn'}
        title={t('admin.moderation.dialog.warnTitle')}
        description={t('admin.moderation.dialog.warnDescription')}
        confirmLabel={t('admin.moderation.dialog.warnConfirm')}
        valid={text.trim().length > 0}
        onCancel={close}
        onConfirm={done(
          () => api.warnCase(id, text.trim()),
          t('admin.moderation.dialog.warnedNotice'),
        )}
      >
        <TextArea
          label={t('admin.moderation.dialog.warnLabel')}
          rows={4}
          maxLength={1000}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      </FormDialog>
      <ReasonDialog
        open={dialog?.kind === 'hold'}
        title={t('admin.moderation.dialog.holdTitle')}
        description={t('admin.moderation.dialog.holdDescription')}
        confirmLabel={t('admin.moderation.dialog.holdConfirm')}
        valid={holdId.trim().length > 0}
        onCancel={close}
        onConfirm={async (reason) => {
          const now = Date.now();
          const win =
            holdKind === 'user_messages'
              ? { since_ms: now - Number(hours || '0') * 3_600_000, until_ms: now }
              : {};
          await done(
            () => api.placeHold(id, { kind: holdKind, id: holdId.trim(), reason, ...win }),
            t('admin.moderation.dialog.heldNotice'),
          )();
        }}
      >
        <Select
          aria-label={t('admin.moderation.dialog.holdKind')}
          options={HOLD_KINDS.map((k) => ({ value: k.id, label: t(k.key) }))}
          value={holdKind}
          onValueChange={(v) => setHoldKind(v as api.HoldKind)}
        />
        <TextInput
          label={t('admin.moderation.dialog.holdId')}
          value={holdId}
          inputMode="numeric"
          onChange={(e) => setHoldId(e.target.value)}
        />
        {holdKind === 'user_messages' && (
          <TextInput
            label={t('admin.moderation.dialog.holdHours')}
            value={hours}
            inputMode="numeric"
            onChange={(e) => setHours(e.target.value.replace(/\D/g, ''))}
          />
        )}
      </ReasonDialog>
      <ReasonDialog
        open={dialog?.kind === 'release'}
        title={t('admin.moderation.dialog.releaseTitle')}
        description={t('admin.moderation.dialog.releaseDescription')}
        confirmLabel={t('admin.moderation.dialog.releaseConfirm')}
        onCancel={close}
        onConfirm={async (reason) => {
          if (dialog?.kind !== 'release') return;
          await done(
            () => api.releaseHold(dialog.hold.id, reason),
            t('admin.moderation.dialog.releasedNotice'),
          )();
        }}
      />
      <ReasonDialog
        open={dialog?.kind === 'ban'}
        danger
        title={t('admin.moderation.dialog.banTitle')}
        description={t('admin.moderation.dialog.banDescription')}
        confirmLabel={t('admin.moderation.dialog.banConfirm')}
        onCancel={close}
        onConfirm={async (reason) =>
          done(
            () =>
              api.banFromCase(id, {
                reason_category: banReason,
                severity: banSeverity,
                note: reason,
              }),
            t('admin.moderation.dialog.bannedNotice'),
          )()
        }
      >
        <BanReasonFields
          reason={banReason}
          severity={banSeverity}
          onChange={(r, sev) => {
            setBanReason(r);
            setBanSeverity(sev);
          }}
        />
      </ReasonDialog>
      <ReasonDialog
        open={dialog?.kind === 'appeal'}
        danger={dialog?.kind === 'appeal' && dialog.decision === 'overturned'}
        title={
          dialog?.kind === 'appeal' && dialog.decision === 'overturned'
            ? t('admin.moderation.dialog.overturnTitle')
            : t('admin.moderation.dialog.upholdTitle')
        }
        description={t('admin.moderation.dialog.appealDescription')}
        confirmLabel={
          dialog?.kind === 'appeal' && dialog.decision === 'overturned'
            ? t('admin.moderation.appeal.overturn')
            : t('admin.moderation.appeal.uphold')
        }
        onCancel={close}
        onConfirm={async (reason) => {
          const appeal = data.appeal;
          if (dialog?.kind !== 'appeal' || !appeal) return;
          await done(
            () => api.decideAppeal(appeal.id, dialog.decision, reason),
            t('admin.moderation.dialog.decidedNotice'),
          )();
        }}
      />
    </>
  );
}
