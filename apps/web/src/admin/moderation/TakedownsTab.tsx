// Admin > Moderation > Takedowns (MOD-TKD-010; docs/specs/moderation-compliance.md section 5; D-293): the register of
// copyright and illegal-content notices that staff enter, with the intake form, the notice page (content, triage,
// uphold or reject, counter-notice, court filing, restore, strikes, timeline) and the CSV and JSON export.
import { Badge, Button, EmptyState, Notice, Select, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT, type MessageKey } from '../../i18n';
import * as api from '../../moderation/api';
import {
  ApiProblem,
  Async,
  FormDialog,
  ReasonDialog,
  fmtMs,
  useAdminViewer,
  useResource,
} from '../ui';

const KIND_KEYS: Record<api.TakedownKind, MessageKey> = {
  copyright: 'admin.takedowns.kind.copyright',
  illegal_content: 'admin.takedowns.kind.illegalContent',
};

export const NOTICE_STATE_KEYS: Record<api.NoticeState, MessageKey> = {
  received: 'admin.takedowns.state.received',
  triage: 'admin.takedowns.state.triage',
  rejected: 'admin.takedowns.state.rejected',
  removed: 'admin.takedowns.state.removed',
  counter_noticed: 'admin.takedowns.state.counterNoticed',
  restored: 'admin.takedowns.state.restored',
  withdrawn: 'admin.takedowns.state.withdrawn',
};

const ITEM_KIND_KEYS: Record<api.TakedownItemKind, MessageKey> = {
  message: 'admin.takedowns.item.message',
  attachment: 'admin.takedowns.item.attachment',
  library_series: 'admin.takedowns.item.librarySeries',
  library_chapter: 'admin.takedowns.item.libraryChapter',
  cinema_title: 'admin.takedowns.item.cinemaTitle',
  cinema_episode: 'admin.takedowns.item.cinemaEpisode',
  storage_object: 'admin.takedowns.item.storageObject',
};
const ITEM_KINDS = Object.keys(ITEM_KIND_KEYS) as api.TakedownItemKind[];

const ITEM_STATE_KEYS: Record<api.TakedownItemState, MessageKey> = {
  identified: 'admin.takedowns.itemState.identified',
  held: 'admin.takedowns.itemState.held',
  disabled: 'admin.takedowns.itemState.disabled',
  restored: 'admin.takedowns.itemState.restored',
};

const EVENT_KEYS: Record<string, MessageKey> = {
  received: 'admin.takedowns.event.received',
  item_added: 'admin.takedowns.event.itemAdded',
  item_removed: 'admin.takedowns.event.itemRemoved',
  held: 'admin.takedowns.event.held',
  rejected: 'admin.takedowns.event.rejected',
  upheld: 'admin.takedowns.event.upheld',
  counter_notice: 'admin.takedowns.event.counterNotice',
  court_filing_noted: 'admin.takedowns.event.courtFilingNoted',
  court_filing_cleared: 'admin.takedowns.event.courtFilingCleared',
  restored: 'admin.takedowns.event.restored',
  withdrawn: 'admin.takedowns.event.withdrawn',
};

type Dialog =
  | { kind: 'hold' }
  | { kind: 'reject' }
  | { kind: 'uphold' }
  | { kind: 'counter' }
  | { kind: 'court'; noted: boolean }
  | { kind: 'restore' }
  | { kind: 'withdraw' };

const stateBadge = (s: api.NoticeState) =>
  s === 'removed' || s === 'counter_noticed' ? 'danger' : s === 'triage' ? 'accent' : 'neutral';

export function TakedownsTab({
  onOpenCase,
  noticeId: openNoticeId = null,
}: {
  onOpenCase: (id: string) => void;
  /** A notice to open straight away (from the badge on a case). */
  noticeId?: string | null;
}) {
  const [chosen, setNoticeId] = useState<string | null>(null);
  const noticeId = chosen ?? openNoticeId;
  return noticeId ? (
    <NoticePage id={noticeId} onBack={() => setNoticeId(null)} onOpenCase={onOpenCase} />
  ) : (
    <Register onOpen={setNoticeId} />
  );
}

function Register({ onOpen }: { onOpen: (id: string) => void }) {
  const t = useT();
  const [state, setState] = useState<api.NoticeState | 'any'>('any');
  const [intake, setIntake] = useState(false);
  const res = useResource(`takedowns:${state}`, () =>
    api.listNotices({ state: state === 'any' ? '' : state, limit: 200 }),
  );
  return (
    <>
      <div className="ss-toolbar" role="group" aria-label={t('admin.takedowns.toolbar')}>
        <Select
          aria-label={t('admin.takedowns.stateFilter')}
          options={[
            { value: 'any', label: t('admin.takedowns.stateAny') },
            ...(Object.keys(NOTICE_STATE_KEYS) as api.NoticeState[]).map((s) => ({
              value: s,
              label: t(NOTICE_STATE_KEYS[s]),
            })),
          ]}
          value={state}
          onValueChange={(v) => setState(v as api.NoticeState | 'any')}
        />
        <Button onClick={() => setIntake(true)}>{t('admin.takedowns.enter')}</Button>
        <Button
          variant="secondary"
          onClick={() => window.location.assign(api.registerExportUrl('csv'))}
        >
          {t('admin.takedowns.exportCsv')}
        </Button>
        <Button
          variant="secondary"
          onClick={() => window.location.assign(api.registerExportUrl('json'))}
        >
          {t('admin.takedowns.exportJson')}
        </Button>
      </div>
      <Notice variant="info" className="ss-status">
        {t('admin.takedowns.policyNote')}
      </Notice>
      <ApiProblem error={res.error} />
      {res.data && res.data.length === 0 && (
        <EmptyState
          title={t('admin.takedowns.emptyTitle')}
          description={t('admin.takedowns.emptyDescription')}
        />
      )}
      {res.data && res.data.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.takedowns.listLabel')}>
          {res.data.map((n) => (
            <li key={n.id} className="ss-row">
              <div className="ss-grow">
                <div className="ss-name">
                  TK-{n.id}{' '}
                  <Badge variant={stateBadge(n.state)}>{t(NOTICE_STATE_KEYS[n.state])}</Badge>{' '}
                  <Badge>{t(KIND_KEYS[n.kind])}</Badge>
                  {n.triage_overdue && (
                    <>
                      {' '}
                      <Badge variant="danger">{t('admin.takedowns.overdue')}</Badge>
                    </>
                  )}
                  {n.court_filing_noted && (
                    <>
                      {' '}
                      <Badge variant="warning">{t('admin.takedowns.courtBadge')}</Badge>
                    </>
                  )}
                </div>
                <div className="ss-meta">
                  {n.notifier_name} ·{' '}
                  {t('admin.takedowns.received', { when: fmtMs(n.received_at_ms) })} ·{' '}
                  {t('admin.takedowns.itemCount', { n: n.items })}
                  {n.restore_after_ms
                    ? ` · ${t('admin.takedowns.restoreAfter', { when: fmtMs(n.restore_after_ms) })}`
                    : ''}
                </div>
              </div>
              <Button
                size="sm"
                variant="secondary"
                aria-label={t('admin.takedowns.openAria', { id: n.id })}
                onClick={() => onOpen(n.id)}
              >
                {t('admin.takedowns.open')}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <IntakeDialog
        open={intake}
        onCancel={() => setIntake(false)}
        onDone={async (id) => {
          setIntake(false);
          await res.reload();
          onOpen(id);
        }}
      />
    </>
  );
}

function IntakeDialog({
  open,
  onCancel,
  onDone,
}: {
  open: boolean;
  onCancel: () => void;
  onDone: (id: string) => Promise<void>;
}) {
  const t = useT();
  const [kind, setKind] = useState<api.TakedownKind>('copyright');
  const [name, setName] = useState('');
  const [contact, setContact] = useState('');
  const [basis, setBasis] = useState('');
  const [locations, setLocations] = useState('');
  const [original, setOriginal] = useState('');
  const [received, setReceived] = useState('');
  const valid = !!(name.trim() && contact.trim() && basis.trim() && original.trim());
  return (
    <FormDialog
      open={open}
      title={t('admin.takedowns.intake.title')}
      description={t('admin.takedowns.intake.description')}
      confirmLabel={t('admin.takedowns.intake.confirm')}
      valid={valid}
      onCancel={onCancel}
      onConfirm={async () => {
        const body: api.NewNotice = {
          kind,
          notifier_name: name.trim(),
          notifier_contact: contact.trim(),
          legal_basis: basis.trim(),
          locations: locations
            .split('\n')
            .map((l) => l.trim())
            .filter(Boolean),
          original_text: original.trim(),
          ...(received ? { received_at_ms: new Date(`${received}T12:00:00Z`).getTime() } : {}),
        };
        const created = await api.createNotice(body);
        setName('');
        setContact('');
        setBasis('');
        setLocations('');
        setOriginal('');
        setReceived('');
        await onDone(created.id);
      }}
    >
      <Select
        aria-label={t('admin.takedowns.intake.kind')}
        options={(Object.keys(KIND_KEYS) as api.TakedownKind[]).map((k) => ({
          value: k,
          label: t(KIND_KEYS[k]),
        }))}
        value={kind}
        onValueChange={(v) => setKind(v as api.TakedownKind)}
      />
      <TextInput
        label={t('admin.takedowns.intake.name')}
        value={name}
        maxLength={200}
        required
        onChange={(e) => setName(e.target.value)}
      />
      <TextInput
        label={t('admin.takedowns.intake.contact')}
        value={contact}
        maxLength={300}
        required
        onChange={(e) => setContact(e.target.value)}
      />
      <TextArea
        label={t('admin.takedowns.intake.basis')}
        value={basis}
        rows={2}
        maxLength={4000}
        required
        onChange={(e) => setBasis(e.target.value)}
      />
      <TextArea
        label={t('admin.takedowns.intake.locations')}
        value={locations}
        rows={3}
        onChange={(e) => setLocations(e.target.value)}
      />
      <TextArea
        label={t('admin.takedowns.intake.original')}
        value={original}
        rows={5}
        maxLength={40000}
        required
        onChange={(e) => setOriginal(e.target.value)}
      />
      <TextInput
        label={t('admin.takedowns.intake.received')}
        type="date"
        value={received}
        onChange={(e) => setReceived(e.target.value)}
      />
    </FormDialog>
  );
}

function NoticePage({
  id,
  onBack,
  onOpenCase,
}: {
  id: string;
  onBack: () => void;
  onOpenCase: (id: string) => void;
}) {
  const canCases = useAdminViewer().can('MANAGE_CASES');
  const t = useT();
  const res = useResource(`takedown:${id}`, () => api.getNotice(id));
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [reply, setReply] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [kind, setKind] = useState<api.TakedownItemKind>('library_series');
  const [target, setTarget] = useState('');
  const [objectKey, setObjectKey] = useState('');
  const [counter, setCounter] = useState('');

  const run = async (fn: () => Promise<unknown>, done?: string) => {
    setError(null);
    try {
      await fn();
      if (done) setNotice(done);
      await res.reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : t('admin.takedowns.failed'));
    }
  };
  const close = () => setDialog(null);

  return (
    <>
      <div className="ss-toolbar">
        <Button variant="ghost" onClick={onBack}>
          {t('admin.takedowns.back')}
        </Button>
      </div>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      {reply && (
        <Notice variant="info" className="ss-status" title={t('admin.takedowns.replyTitle')}>
          <p>{t('admin.takedowns.replyHelp')}</p>
          <TextArea
            label={t('admin.takedowns.replyLabel')}
            value={reply}
            rows={8}
            readOnly
            onChange={() => undefined}
          />
        </Notice>
      )}
      <ApiProblem error={error} />
      <Async res={res}>
        {(n) => {
          const early = n.state === 'received' || n.state === 'triage';
          const inForce = n.state === 'removed' || n.state === 'counter_noticed';
          const hasIdentified = n.items.some((i) => i.state === 'identified');
          return (
            <>
              <h2 className="ss-title">{t('admin.takedowns.title', { id: n.id })}</h2>
              <p className="ss-meta">
                <Badge variant={stateBadge(n.state)}>{t(NOTICE_STATE_KEYS[n.state])}</Badge>{' '}
                <Badge>{t(KIND_KEYS[n.kind])}</Badge> ·{' '}
                {t('admin.takedowns.received', { when: fmtMs(n.received_at_ms) })}
                {early &&
                  ` · ${t('admin.takedowns.triageDue', { when: fmtMs(n.triage_due_at_ms) })}`}
                {n.triage_overdue && (
                  <>
                    {' '}
                    <Badge variant="danger">{t('admin.takedowns.overdue')}</Badge>
                  </>
                )}
              </p>
              {n.state === 'counter_noticed' && n.restore_after_ms && (
                <Notice variant={n.court_filing_noted ? 'warning' : 'info'} className="ss-status">
                  {n.court_filing_noted
                    ? t('admin.takedowns.courtNoted')
                    : t('admin.takedowns.restoreScheduled', { when: fmtMs(n.restore_after_ms) })}
                </Notice>
              )}
              {n.state === 'rejected' && (
                <p>{t('admin.takedowns.rejectedBecause', { reason: n.rejection_reason })}</p>
              )}
              {n.case_id && canCases && (
                <Button size="sm" variant="ghost" onClick={() => onOpenCase(n.case_id!)}>
                  {t('admin.takedowns.openCase', { id: n.case_id })}
                </Button>
              )}

              <div className="ss-toolbar" role="group" aria-label={t('admin.takedowns.actions')}>
                {early && n.items.length > 0 && hasIdentified && (
                  <Button variant="secondary" onClick={() => setDialog({ kind: 'hold' })}>
                    {t('admin.takedowns.hold')}
                  </Button>
                )}
                {n.state === 'triage' && n.items.length > 0 && (
                  <Button variant="danger" onClick={() => setDialog({ kind: 'uphold' })}>
                    {t('admin.takedowns.uphold')}
                  </Button>
                )}
                {early && (
                  <Button variant="secondary" onClick={() => setDialog({ kind: 'reject' })}>
                    {t('admin.takedowns.reject')}
                  </Button>
                )}
                {n.state === 'removed' && (
                  <Button variant="secondary" onClick={() => setDialog({ kind: 'counter' })}>
                    {t('admin.takedowns.counter')}
                  </Button>
                )}
                {inForce && (
                  <Button
                    variant="secondary"
                    onClick={() => setDialog({ kind: 'court', noted: !n.court_filing_noted })}
                  >
                    {n.court_filing_noted
                      ? t('admin.takedowns.courtClear')
                      : t('admin.takedowns.courtNote')}
                  </Button>
                )}
                {inForce && (
                  <Button variant="secondary" onClick={() => setDialog({ kind: 'restore' })}>
                    {t('admin.takedowns.restore')}
                  </Button>
                )}
                {(early || inForce) && (
                  <Button variant="secondary" onClick={() => setDialog({ kind: 'withdraw' })}>
                    {t('admin.takedowns.withdraw')}
                  </Button>
                )}
              </div>

              <h3 className="ss-subtitle">{t('admin.takedowns.notifier')}</h3>
              <p>
                {n.notifier_name} · {n.notifier_contact}
              </p>
              <p>{t('admin.takedowns.basis', { basis: n.legal_basis })}</p>
              {n.locations.length > 0 && (
                <ul aria-label={t('admin.takedowns.locations')}>
                  {n.locations.map((l, i) => (
                    <li key={i}>{l}</li>
                  ))}
                </ul>
              )}
              <details>
                <summary>{t('admin.takedowns.original')}</summary>
                <pre className="ss-pre">{n.original_text}</pre>
              </details>
              {n.counter_notice_text && (
                <details>
                  <summary>
                    {t('admin.takedowns.counterText', { when: fmtMs(n.counter_noticed_at_ms) })}
                  </summary>
                  <pre className="ss-pre">{n.counter_notice_text}</pre>
                </details>
              )}

              <h3 className="ss-subtitle">{t('admin.takedowns.contentHeading')}</h3>
              {n.items.length === 0 && <p className="ss-meta">{t('admin.takedowns.noContent')}</p>}
              {n.items.length > 0 && (
                <ul className="ss-list" aria-label={t('admin.takedowns.contentList')}>
                  {n.items.map((i) => (
                    <li key={i.id} className="ss-row">
                      <div className="ss-grow">
                        <div className="ss-name">
                          {i.label}{' '}
                          <Badge variant={i.state === 'disabled' ? 'danger' : 'neutral'}>
                            {t(ITEM_STATE_KEYS[i.state])}
                          </Badge>
                        </div>
                        <div className="ss-meta">
                          {t(ITEM_KIND_KEYS[i.kind])} · {i.target_id}
                          {i.object_key
                            ? ` · ${t('admin.takedowns.objectKey', { key: i.object_key })}`
                            : ''}
                          {i.subject_account_id
                            ? ` · ${t('admin.takedowns.uploader', { id: i.subject_account_id })}`
                            : ''}
                        </div>
                      </div>
                      {early && i.state === 'identified' && (
                        <Button
                          size="sm"
                          variant="ghost"
                          aria-label={t('admin.takedowns.removeItemAria', { label: i.label })}
                          onClick={() => void run(() => api.removeNoticeItem(n.id, i.id))}
                        >
                          {t('admin.takedowns.removeItem')}
                        </Button>
                      )}
                    </li>
                  ))}
                </ul>
              )}
              {early && (
                <form
                  className="ss-toolbar"
                  aria-label={t('admin.takedowns.addForm')}
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (!target.trim() || (kind === 'storage_object' && !objectKey.trim())) return;
                    void run(async () => {
                      await api.addNoticeItem(
                        n.id,
                        kind,
                        target.trim(),
                        kind === 'storage_object' ? objectKey.trim() : undefined,
                      );
                      setTarget('');
                      setObjectKey('');
                    });
                  }}
                >
                  <Select
                    aria-label={t('admin.takedowns.addKind')}
                    options={ITEM_KINDS.map((k) => ({ value: k, label: t(ITEM_KIND_KEYS[k]) }))}
                    value={kind}
                    onValueChange={(v) => setKind(v as api.TakedownItemKind)}
                  />
                  <TextInput
                    label={t(
                      kind === 'storage_object'
                        ? 'admin.takedowns.addMountId'
                        : 'admin.takedowns.addId',
                    )}
                    value={target}
                    inputMode="numeric"
                    onChange={(e) => setTarget(e.target.value.replace(/\D/g, ''))}
                  />
                  {kind === 'storage_object' && (
                    <TextInput
                      label={t('admin.takedowns.addObjectKey')}
                      value={objectKey}
                      onChange={(e) => setObjectKey(e.target.value)}
                    />
                  )}
                  <Button
                    type="submit"
                    variant="secondary"
                    disabled={!target.trim() || (kind === 'storage_object' && !objectKey.trim())}
                  >
                    {t('admin.takedowns.addItem')}
                  </Button>
                </form>
              )}

              {n.strikes.length > 0 && (
                <>
                  <h3 className="ss-subtitle">{t('admin.takedowns.strikesHeading')}</h3>
                  <ul className="ss-list" aria-label={t('admin.takedowns.strikesList')}>
                    {n.strikes.map((s) => (
                      <li key={s.person_id} className="ss-row">
                        <div className="ss-grow">
                          <div className="ss-name">
                            {t('admin.takedowns.strikeLine', {
                              person: s.person_id,
                              n: s.strikes,
                              threshold: s.threshold,
                              months: s.window_months,
                            })}
                          </div>
                          {s.contested > 0 && (
                            <div className="ss-meta">
                              <Badge variant="warning">
                                {t('admin.takedowns.contested', { n: s.contested })}
                              </Badge>
                            </div>
                          )}
                          <ul aria-label={t('admin.takedowns.countedList')}>
                            {s.counted.map((c) => (
                              <li key={c.id} className="ss-meta">
                                {t('admin.takedowns.countedLine', {
                                  ref: c.id,
                                  kind: t(KIND_KEYS[c.kind]),
                                  state: t(NOTICE_STATE_KEYS[c.state]),
                                  when: fmtMs(c.removed_at_ms),
                                })}
                                {c.contested && (
                                  <>
                                    {' '}
                                    <Badge variant="warning">
                                      {t('admin.takedowns.counterPending')}
                                    </Badge>
                                  </>
                                )}
                              </li>
                            ))}
                          </ul>
                          <div className="ss-meta">{t('admin.takedowns.strikeNote')}</div>
                          <div className="ss-meta">{t('admin.takedowns.strikeHelp')}</div>
                        </div>
                        {s.review_case_id && canCases && (
                          <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => onOpenCase(s.review_case_id!)}
                          >
                            {t('admin.takedowns.openReview')}
                          </Button>
                        )}
                      </li>
                    ))}
                  </ul>
                </>
              )}

              <h3 className="ss-subtitle">{t('admin.takedowns.timeline')}</h3>
              <ol className="ss-list" aria-label={t('admin.takedowns.timelineList')}>
                {n.events.map((e) => (
                  <li key={e.id} className="ss-row">
                    <div className="ss-grow">
                      <div>
                        {EVENT_KEYS[e.kind]
                          ? t(EVENT_KEYS[e.kind]!, {
                              reason: String(e.detail['reason'] ?? ''),
                              label: String(e.detail['label'] ?? ''),
                              items: String(e.detail['items'] ?? ''),
                            })
                          : e.kind}
                      </div>
                      <div className="ss-meta">
                        {fmtMs(e.created_at_ms)}
                        {e.actor_id ? ` · ${t('admin.takedowns.by', { id: e.actor_id })}` : ''}
                      </div>
                    </div>
                  </li>
                ))}
              </ol>

              <ReasonDialog
                open={dialog?.kind === 'hold'}
                title={t('admin.takedowns.holdTitle')}
                description={t('admin.takedowns.holdDescription')}
                confirmLabel={t('admin.takedowns.hold')}
                onCancel={close}
                onConfirm={async (reason) => {
                  await api.holdNotice(n.id, reason);
                  close();
                  setNotice(t('admin.takedowns.heldNotice'));
                  await res.reload();
                }}
              />
              <ReasonDialog
                open={dialog?.kind === 'uphold'}
                danger
                title={t('admin.takedowns.upholdTitle')}
                description={t('admin.takedowns.upholdDescription')}
                confirmLabel={t('admin.takedowns.uphold')}
                onCancel={close}
                onConfirm={async (reason) => {
                  const out = await api.upholdNotice(n.id, reason);
                  close();
                  setNotice(
                    t('admin.takedowns.upheldNotice', {
                      items: out.items_disabled,
                      notified: out.notified,
                    }) +
                      (out.review_case_id
                        ? ` ${t('admin.takedowns.reviewOpened', { id: out.review_case_id })}`
                        : ''),
                  );
                  await res.reload();
                }}
              />
              <ReasonDialog
                open={dialog?.kind === 'reject'}
                title={t('admin.takedowns.rejectTitle')}
                description={t('admin.takedowns.rejectDescription')}
                reasonLabel={t('admin.takedowns.rejectReason')}
                confirmLabel={t('admin.takedowns.reject')}
                onCancel={close}
                onConfirm={async (reason) => {
                  const out = await api.rejectNotice(n.id, reason);
                  close();
                  setReply(out.reply);
                  await res.reload();
                }}
              />
              <FormDialog
                open={dialog?.kind === 'counter'}
                title={t('admin.takedowns.counterTitle')}
                description={t('admin.takedowns.counterDescription')}
                confirmLabel={t('admin.takedowns.counter')}
                valid={counter.trim().length > 0}
                onCancel={close}
                onConfirm={async () => {
                  await api.counterNotice(n.id, counter.trim());
                  setCounter('');
                  close();
                  setNotice(t('admin.takedowns.counterNotice'));
                  await res.reload();
                }}
              >
                <TextArea
                  label={t('admin.takedowns.counterLabel')}
                  value={counter}
                  rows={6}
                  maxLength={20000}
                  required
                  onChange={(e) => setCounter(e.target.value)}
                />
              </FormDialog>
              <ReasonDialog
                open={dialog?.kind === 'court'}
                title={
                  dialog?.kind === 'court' && !dialog.noted
                    ? t('admin.takedowns.courtClearTitle')
                    : t('admin.takedowns.courtNoteTitle')
                }
                description={t('admin.takedowns.courtDescription')}
                confirmLabel={
                  dialog?.kind === 'court' && !dialog.noted
                    ? t('admin.takedowns.courtClear')
                    : t('admin.takedowns.courtNote')
                }
                onCancel={close}
                onConfirm={async (reason) => {
                  if (dialog?.kind !== 'court') return;
                  await api.courtFiling(n.id, dialog.noted, reason);
                  close();
                  await res.reload();
                }}
              />
              <ReasonDialog
                open={dialog?.kind === 'restore'}
                title={t('admin.takedowns.restoreTitle')}
                description={t('admin.takedowns.restoreDescription')}
                confirmLabel={t('admin.takedowns.restore')}
                onCancel={close}
                onConfirm={async (reason) => {
                  await api.restoreNotice(n.id, reason);
                  close();
                  setNotice(t('admin.takedowns.restoredNotice'));
                  await res.reload();
                }}
              />
              <ReasonDialog
                open={dialog?.kind === 'withdraw'}
                title={t('admin.takedowns.withdrawTitle')}
                description={t('admin.takedowns.withdrawDescription')}
                confirmLabel={t('admin.takedowns.withdraw')}
                onCancel={close}
                onConfirm={async (reason) => {
                  await api.withdrawNotice(n.id, reason);
                  close();
                  setNotice(t('admin.takedowns.withdrawnNotice'));
                  await res.reload();
                }}
              />
            </>
          );
        }}
      </Async>
    </>
  );
}
