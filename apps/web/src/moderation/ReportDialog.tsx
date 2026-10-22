// The report dialog (MOD-RPT-010; spec section 3.2). Mounted once at the app root. The client sends only ids and
// the note; the server snapshots the evidence, routes the report and keeps the reporter hidden from the reported
// person (spec 3.3).
import { Button, Checkbox, Modal, Notice, Select, TextArea } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../api/client';
import { useT, type MessageKey } from '../i18n';
import { blockUser } from '../lounge/shell/api';
import { fileReport, REPORT_CATEGORIES, type ReportCategory } from './api';
import { closeReport, useReports, type ReportKind, type ReportTarget } from './reports';
import './moderation.css';

/** Labels of the categories, resolved at render time. */
export const CATEGORY_KEYS: Record<ReportCategory, MessageKey> = {
  spam: 'moderation.category.spam',
  harassment: 'moderation.category.harassment',
  illegal_content: 'moderation.category.illegalContent',
  child_safety: 'moderation.category.childSafety',
  ban_evasion: 'moderation.category.banEvasion',
  fraud: 'moderation.category.fraud',
  platform_abuse: 'moderation.category.platformAbuse',
  nsfw_policy: 'moderation.category.nsfwPolicy',
  other: 'moderation.category.other',
};

const TITLE_KEYS: Record<ReportKind, MessageKey> = {
  message: 'moderation.reportDialog.title.message',
  user: 'moderation.reportDialog.title.user',
  channel: 'moderation.reportDialog.title.channel',
  guild: 'moderation.reportDialog.title.guild',
  library: 'moderation.reportDialog.title.library',
  cinema: 'moderation.reportDialog.title.cinema',
};

/** Which server moderators could get this report, so "may see my name" is worth offering. */
const mayReachModerators = (k: ReportKind) => k === 'message' || k === 'channel' || k === 'user';

function failure(e: unknown, t: ReturnType<typeof useT>): string {
  if (e instanceof ApiError) {
    if (e.status === 403 && e.body?.code === 900001) return t('moderation.reportDialog.errorOff');
    if (e.status === 409) return t('moderation.reportDialog.errorDuplicate');
    if (e.status === 429) return t('moderation.reportDialog.errorLimit');
    if (e.status === 404) return t('moderation.reportDialog.errorGone');
  }
  return t('moderation.reportDialog.errorGeneric');
}

function ReportForm({ target }: { target: ReportTarget }) {
  const t = useT();
  const [category, setCategory] = useState<ReportCategory | ''>('');
  const [note, setNote] = useState('');
  const [showName, setShowName] = useState(false);
  const [block, setBlock] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sent, setSent] = useState(false);
  const blockId = target.kind === 'user' ? target.id : target.authorId;

  const submit = async (e?: FormEvent) => {
    e?.preventDefault();
    if (!category || busy) return;
    setBusy(true);
    setError(null);
    try {
      await fileReport({
        target_kind: target.kind,
        target_id: target.id,
        ...(target.channelId ? { channel_id: target.channelId } : {}),
        ...(target.guildId ? { guild_id: target.guildId } : {}),
        category,
        note: note.trim(),
        moderators_may_see_name: mayReachModerators(target.kind) && showName,
      });
      if (block && blockId) await blockUser(blockId).catch(() => undefined);
      setSent(true);
    } catch (err) {
      setError(failure(err, t));
    } finally {
      setBusy(false);
    }
  };

  if (sent) {
    return (
      <Modal
        open
        onClose={closeReport}
        title={t('moderation.reportDialog.sentTitle')}
        size="sm"
        footer={<Button onClick={closeReport}>{t('moderation.report.close')}</Button>}
      >
        <p>{t('moderation.reportDialog.sentBody')}</p>
      </Modal>
    );
  }

  return (
    <Modal
      open
      onClose={busy ? () => undefined : closeReport}
      title={t(TITLE_KEYS[target.kind])}
      description={t('moderation.reportDialog.description')}
      size="sm"
      footer={
        <>
          <Button variant="ghost" onClick={closeReport} disabled={busy}>
            {t('moderation.reportDialog.cancel')}
          </Button>
          <Button disabled={!category} loading={busy} onClick={() => void submit()}>
            {t('moderation.reportDialog.send')}
          </Button>
        </>
      }
    >
      <form onSubmit={(e) => void submit(e)} className="mod-report-form">
        <Select
          aria-label={t('moderation.reportDialog.categoryLabel')}
          placeholder={t('moderation.reportDialog.categoryPlaceholder')}
          options={REPORT_CATEGORIES.map((c) => ({ value: c, label: t(CATEGORY_KEYS[c]) }))}
          value={category || null}
          onValueChange={(v) => setCategory(v as ReportCategory)}
        />
        <TextArea
          label={t('moderation.reportDialog.noteLabel')}
          value={note}
          rows={4}
          maxLength={2000}
          onChange={(e) => setNote(e.target.value)}
        />
        {mayReachModerators(target.kind) && (
          <Checkbox
            label={t('moderation.reportDialog.showName')}
            description={t('moderation.reportDialog.showNameHelp')}
            checked={showName}
            onChange={(e) => setShowName(e.target.checked)}
          />
        )}
        {blockId && (
          <Checkbox
            label={t('moderation.reportDialog.alsoBlock')}
            checked={block}
            onChange={(e) => setBlock(e.target.checked)}
          />
        )}
        {error && (
          <Notice variant="danger" role="alert">
            {error}
          </Notice>
        )}
      </form>
    </Modal>
  );
}

/** Mounted once at the app root. */
export function ReportDialog() {
  const target = useReports((s) => s.target);
  if (!target) return null;
  return <ReportForm key={`${target.kind}:${target.id}`} target={target} />;
}
