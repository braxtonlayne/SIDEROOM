// Platform announcement composer (ADM-PLT-009): create, edit while showing (G-69), reuse an ended one as a template,
// and end. Severity, the critical flag and the message to everyone are fixed once published.
import {
  Badge,
  Button,
  Checkbox,
  ConfirmDialog,
  Notice,
  Select,
  TextArea,
  TextInput,
} from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { useT, type MessageKey } from '../../i18n';
import {
  ActionStatus,
  Async,
  ReasonDialog,
  SectionHeader,
  fmtMs,
  useAction,
  useResource,
} from '../ui';

const TITLE_MAX = 120;
const BODY_MAX = 2000;

const SEVERITY_KEY: Record<api.AnnouncementSeverity, MessageKey> = {
  info: 'admin.announcements.severityInfo',
  warning: 'admin.announcements.severityWarning',
  critical: 'admin.announcements.severityCritical',
};
const STATUS_KEY: Record<'active' | 'scheduled' | 'ended' | 'expired', MessageKey> = {
  active: 'admin.announcements.statusActive',
  scheduled: 'admin.announcements.statusScheduled',
  ended: 'admin.announcements.statusEnded',
  expired: 'admin.announcements.statusExpired',
};

type Status = 'active' | 'scheduled' | 'ended' | 'expired';
const statusOf = (a: api.PlatformAnnouncement, now = Date.now()): Status => {
  if (a.ended_at_ms) return 'ended';
  if (a.ends_at_ms && a.ends_at_ms <= now) return 'expired';
  if (a.starts_at_ms > now) return 'scheduled';
  return 'active';
};

interface Draft {
  title: string;
  body: string;
  severity: api.AnnouncementSeverity;
  endsAt: string;
  sendDm: boolean;
}
const EMPTY: Draft = { title: '', body: '', severity: 'info', endsAt: '', sendDm: false };

export function AnnouncementsSection() {
  const t = useT();
  const list = useResource('admin:announcements', api.listAnnouncements);
  const [composing, setComposing] = useState(false);
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [ending, setEnding] = useState<api.PlatformAnnouncement | null>(null);
  const [editing, setEditing] = useState<api.PlatformAnnouncement | null>(null);
  const [edit, setEdit] = useState({ title: '', body: '', endsAt: '' });
  const [notice, setNotice] = useState<string | null>(null);
  const action = useAction();

  const endsMs = draft.endsAt ? new Date(draft.endsAt).getTime() : null;
  const endsValid = endsMs === null || (Number.isFinite(endsMs) && endsMs > Date.now());
  const valid =
    draft.title.trim().length > 0 &&
    draft.title.length <= TITLE_MAX &&
    draft.body.trim().length > 0 &&
    draft.body.length <= BODY_MAX &&
    endsValid;

  // A critical notice is always sent as a DM (D-192): the toggle is forced on and locked.
  const critical = draft.severity === 'critical';
  const dm = critical || draft.sendDm;

  const editEndsMs = edit.endsAt ? new Date(edit.endsAt).getTime() : null;
  const editEndsValid =
    editEndsMs === null || (Number.isFinite(editEndsMs) && editEndsMs > Date.now());
  const editValid =
    edit.title.trim().length > 0 &&
    edit.title.length <= TITLE_MAX &&
    edit.body.trim().length > 0 &&
    edit.body.length <= BODY_MAX &&
    editEndsValid;
  /** The local date-time input value for a stored end time. */
  const toLocalInput = (ms: number | null | undefined) => {
    if (!ms) return '';
    const d = new Date(ms - new Date(ms).getTimezoneOffset() * 60_000);
    return d.toISOString().slice(0, 16);
  };
  const startEdit = (a: api.PlatformAnnouncement) => {
    setEdit({ title: a.title, body: a.body, endsAt: toLocalInput(a.ends_at_ms) });
    setEditing(a);
  };

  const open = (d: Draft) => {
    setDraft(d);
    setComposing(true);
  };

  return (
    <>
      <SectionHeader
        title={t('admin.announcements.title')}
        description={t('admin.announcements.description')}
        actions={<Button onClick={() => open(EMPTY)}>{t('admin.announcements.new')}</Button>}
      />
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <ActionStatus error={action.error} />
      <Async res={list} label={t('admin.announcements.loading')}>
        {(rows) =>
          rows.length === 0 ? (
            <p className="ss-sub">{t('admin.announcements.empty')}</p>
          ) : (
            <ul className="ss-list" aria-label={t('admin.announcements.listLabel')}>
              {rows.map((a) => {
                const status = statusOf(a);
                return (
                  <li key={a.id} className="ss-row">
                    <div className="ss-grow">
                      <div className="ss-name">
                        {a.title}{' '}
                        <Badge
                          variant={
                            a.severity === 'critical'
                              ? 'danger'
                              : a.severity === 'warning'
                                ? 'warning'
                                : 'neutral'
                          }
                        >
                          {t(SEVERITY_KEY[a.severity])}
                        </Badge>{' '}
                        <Badge variant={status === 'active' ? 'success' : 'neutral'}>
                          {t(STATUS_KEY[status])}
                        </Badge>
                        {a.send_dm && (
                          <>
                            {' '}
                            <Badge variant="accent">{t('admin.announcements.sentAsDm')}</Badge>
                          </>
                        )}
                      </div>
                      <div className="ss-meta">
                        {a.body}
                        <br />
                        {fmtMs(a.starts_at_ms)}
                        {a.ends_at_ms
                          ? ` · ${t('admin.announcements.until', { when: fmtMs(a.ends_at_ms) })}`
                          : ''}
                      </div>
                    </div>
                    {status === 'active' || status === 'scheduled' ? (
                      <>
                        <Button
                          size="sm"
                          variant="secondary"
                          aria-label={t('admin.announcements.editAria', { title: a.title })}
                          onClick={() => startEdit(a)}
                        >
                          {t('admin.announcements.edit')}
                        </Button>
                        <Button
                          size="sm"
                          variant="danger"
                          aria-label={t('admin.announcements.endAria', { title: a.title })}
                          onClick={() => setEnding(a)}
                        >
                          {t('admin.announcements.end')}
                        </Button>
                      </>
                    ) : (
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={t('admin.announcements.reuseAria', { title: a.title })}
                        onClick={() =>
                          open({
                            title: a.title,
                            body: a.body,
                            severity: a.severity,
                            endsAt: '',
                            sendDm: false,
                          })
                        }
                      >
                        {t('admin.announcements.reuse')}
                      </Button>
                    )}
                  </li>
                );
              })}
            </ul>
          )
        }
      </Async>
      <p className="ss-sub" style={{ marginTop: 16 }}>
        {t('admin.announcements.noEditNote')}
      </p>

      <ReasonDialog
        open={!!editing}
        title={t('admin.announcements.editTitle')}
        confirmLabel={t('admin.announcements.editSave')}
        reasonOptional
        valid={editValid}
        onCancel={() => setEditing(null)}
        onConfirm={async (reason) => {
          if (!editing) return;
          await api.editAnnouncement(editing.id, {
            title: edit.title.trim(),
            body: edit.body.trim(),
            ...(editEndsMs !== null ? { ends_at_ms: editEndsMs } : {}),
            reason,
          });
          setEditing(null);
          setNotice(t('admin.announcements.edited'));
          await list.reload();
        }}
      >
        <TextInput
          label={t('admin.announcements.fieldTitle')}
          maxLength={TITLE_MAX}
          value={edit.title}
          onChange={(e) => setEdit({ ...edit, title: e.target.value })}
        />
        <TextArea
          label={t('admin.announcements.fieldBody')}
          maxLength={BODY_MAX}
          rows={5}
          value={edit.body}
          onChange={(e) => setEdit({ ...edit, body: e.target.value })}
        />
        <TextInput
          label={t('admin.announcements.fieldEnds')}
          hint={t('admin.announcements.fieldEndsHint')}
          type="datetime-local"
          value={edit.endsAt}
          error={editEndsValid ? undefined : t('admin.announcements.endsInPast')}
          onChange={(e) => setEdit({ ...edit, endsAt: e.target.value })}
        />
        <p className="ss-sub">{t('admin.announcements.editFixed')}</p>
      </ReasonDialog>

      <ReasonDialog
        open={composing}
        title={t('admin.announcements.composeTitle')}
        confirmLabel={
          dm ? t('admin.announcements.publishAndMessage') : t('admin.announcements.publish')
        }
        danger={dm}
        reasonOptional
        valid={valid}
        onCancel={() => setComposing(false)}
        onConfirm={async (reason) => {
          await api.createAnnouncement({
            title: draft.title.trim(),
            body: draft.body.trim(),
            severity: draft.severity,
            critical,
            send_dm: dm,
            ends_at_ms: endsMs,
            reason,
          });
          setComposing(false);
          setNotice(
            dm ? t('admin.announcements.publishedWithDm') : t('admin.announcements.published'),
          );
          await list.reload();
        }}
      >
        <TextInput
          label={t('admin.announcements.fieldTitle')}
          maxLength={TITLE_MAX}
          value={draft.title}
          onChange={(e) => setDraft({ ...draft, title: e.target.value })}
        />
        <TextArea
          label={t('admin.announcements.fieldBody')}
          maxLength={BODY_MAX}
          rows={5}
          value={draft.body}
          onChange={(e) => setDraft({ ...draft, body: e.target.value })}
        />
        <Select
          aria-label={t('admin.announcements.fieldSeverity')}
          options={(Object.keys(SEVERITY_KEY) as api.AnnouncementSeverity[]).map((v) => ({
            value: v,
            label: t(SEVERITY_KEY[v]),
          }))}
          value={draft.severity}
          onValueChange={(v) => setDraft({ ...draft, severity: v as api.AnnouncementSeverity })}
        />
        {draft.severity === 'critical' && (
          <p className="ss-sub">{t('admin.announcements.criticalNote')}</p>
        )}
        <TextInput
          label={t('admin.announcements.fieldEnds')}
          hint={t('admin.announcements.fieldEndsHint')}
          type="datetime-local"
          value={draft.endsAt}
          error={endsValid ? undefined : t('admin.announcements.endsInPast')}
          onChange={(e) => setDraft({ ...draft, endsAt: e.target.value })}
        />
        <Checkbox
          label={t('admin.announcements.sendDm')}
          description={t('admin.announcements.sendDmHint')}
          checked={dm}
          disabled={critical}
          onChange={(e) => setDraft({ ...draft, sendDm: e.target.checked })}
        />
        {dm && (
          <Notice variant="warning" role="alert" title={t('admin.announcements.dmWarningTitle')}>
            {critical
              ? t('admin.announcements.dmWarning')
              : t('admin.announcements.dmWarningOptOut')}
          </Notice>
        )}
      </ReasonDialog>

      <ConfirmDialog
        open={!!ending}
        danger
        title={t('admin.announcements.endTitle', { title: ending?.title ?? '' })}
        description={t('admin.announcements.endDescription')}
        confirmLabel={t('admin.announcements.endConfirm')}
        loading={action.busy}
        onCancel={() => setEnding(null)}
        onConfirm={() => {
          if (!ending) return;
          void action
            .run(async () => {
              await api.endAnnouncement(ending.id);
              return true;
            })
            .then(async (ok) => {
              setEnding(null);
              if (ok) {
                setNotice(t('admin.announcements.ended'));
                await list.reload();
              }
            });
        }}
      />
    </>
  );
}
