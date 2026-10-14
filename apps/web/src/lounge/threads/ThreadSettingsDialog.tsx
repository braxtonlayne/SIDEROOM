import { Button, Modal, Select, Switch, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { useT } from '../../i18n';
import { ARCHIVE_CHOICES, THREAD_NAME_MAX } from './CreateThreadDialog';
import { useThreads, type ThreadInfo } from './threadStore';

/** Slowmode choices in seconds, as in Edit Channel. */
const SLOWMODE = [0, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200, 21600] as const;

const slowLabel = (s: number, t: ReturnType<typeof useT>) =>
  s === 0
    ? t('lounge.threadSettings.slowOff')
    : s < 60
      ? t('lounge.threadSettings.slowSeconds', { n: s })
      : s < 3600
        ? t('lounge.threadSettings.slowMinutes', { n: s / 60 })
        : t('lounge.threadSettings.slowHours', { n: s / 3600 });

/** Thread settings: name, slowmode, archive duration, lock, delete (LNG-THR-009). */
export function ThreadSettingsDialog({
  thread,
  onClose,
  onDeleted,
}: {
  thread: ThreadInfo;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const t = useT();
  const [name, setName] = useState(thread.name);
  const [slow, setSlow] = useState(String(thread.slowmode));
  const [archive, setArchive] = useState(String(thread.autoArchive));
  const [locked, setLocked] = useState(thread.locked);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const archiveOptions: { value: string; label: string }[] = [
    ...ARCHIVE_CHOICES.map((c) => ({ value: c.value, label: t(c.labelKey) })),
  ];
  if (!archiveOptions.some((o) => o.value === archive))
    archiveOptions.push({
      value: archive,
      label: t('lounge.threadSettings.archiveMinutes', { n: archive }),
    });
  const slowValues = SLOWMODE.includes(thread.slowmode as never)
    ? SLOWMODE
    : [...SLOWMODE, thread.slowmode].sort((a, b) => a - b);

  const run = async (fn: () => Promise<void>, done: () => void) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
      done();
    } catch {
      setError(t('lounge.threadSettings.error'));
      setBusy(false);
    }
  };

  const save = (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) {
      setError(t('lounge.createThread.nameRequired'));
      return;
    }
    void run(
      () =>
        useThreads.getState().update(thread.id, {
          name: trimmed,
          rate_limit_per_user: Number(slow),
          auto_archive_duration: Number(archive),
          locked,
        }),
      onClose,
    );
  };

  return (
    <Modal open onClose={onClose} size="sm" title={t('lounge.threadSettings.title')}>
      <form className="lng-form" onSubmit={save}>
        <TextInput
          label={t('lounge.createThread.nameLabel')}
          value={name}
          maxLength={THREAD_NAME_MAX}
          onChange={(e) => setName(e.target.value)}
        />
        <label className="lng-field-label" htmlFor="thread-set-slow">
          {t('lounge.threadSettings.slowmode')}
        </label>
        <Select
          id="thread-set-slow"
          value={slow}
          onValueChange={setSlow}
          options={slowValues.map((s) => ({ value: String(s), label: slowLabel(s, t) }))}
        />
        <label className="lng-field-label" htmlFor="thread-set-archive">
          {t('lounge.createThread.hideAfter')}
        </label>
        <Select
          id="thread-set-archive"
          value={archive}
          onValueChange={setArchive}
          options={archiveOptions}
        />
        <Switch
          label={t('lounge.threadSettings.lock')}
          description={t('lounge.threadSettings.lockHelp')}
          checked={locked}
          onChange={(e) => setLocked(e.target.checked)}
        />
        {error && (
          <p className="lng-muted" role="alert">
            {error}
          </p>
        )}
        {confirmDelete ? (
          <div className="lng-form-actions">
            <span>{t('lounge.threadSettings.deleteConfirm', { name: thread.name })}</span>
            <Button variant="ghost" onClick={() => setConfirmDelete(false)}>
              {t('lounge.threadSettings.keep')}
            </Button>
            <Button
              variant="danger"
              loading={busy}
              onClick={() => void run(() => useThreads.getState().destroy(thread.id), onDeleted)}
            >
              {t('lounge.threadSettings.deleteConfirmButton')}
            </Button>
          </div>
        ) : (
          <div className="lng-form-actions">
            <Button variant="danger" onClick={() => setConfirmDelete(true)}>
              {t('lounge.threadSettings.delete')}
            </Button>
            <Button variant="ghost" onClick={onClose}>
              {t('lounge.createThread.cancel')}
            </Button>
            <Button type="submit" loading={busy}>
              {t('lounge.threadSettings.save')}
            </Button>
          </div>
        )}
      </form>
    </Modal>
  );
}
