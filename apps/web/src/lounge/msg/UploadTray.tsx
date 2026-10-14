import { Button, Icon, Modal, ProgressBar } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { useT } from '../../i18n';
import { fileIcon } from './Attachments';
import { formatBytes } from './format';
import type { PendingFile } from './upload';

function Thumb({ file }: { file: File }) {
  const url = useMemo(
    () =>
      file.type.startsWith('image/') && typeof URL.createObjectURL === 'function'
        ? URL.createObjectURL(file)
        : null,
    [file],
  );
  useEffect(() => () => void (url && URL.revokeObjectURL?.(url)), [url]);
  return url ? (
    <img src={url} alt="" className="lng-tray-thumb" />
  ) : (
    <span className="lng-tray-thumb lng-tray-icon">
      <Icon name={fileIcon({ contentType: file.type, filename: file.name })} size={24} />
    </span>
  );
}

/** Files waiting to be sent: thumbnail, name, per-file progress, edit and remove (LNG-CMP-005/007). */
export function UploadTray({
  files,
  busy,
  onChange,
  onRemove,
}: {
  files: PendingFile[];
  /** An upload is in flight; edits are locked and Remove becomes Cancel for the whole send. */
  busy: boolean;
  onChange: (id: string, patch: Partial<PendingFile>) => void;
  onRemove: (id: string) => void;
}) {
  const t = useT();
  const [editing, setEditing] = useState<string | null>(null);
  const target = files.find((f) => f.id === editing);
  const [name, setName] = useState('');
  const [alt, setAlt] = useState('');
  const [spoiler, setSpoiler] = useState(false);

  useEffect(() => {
    if (target) {
      setName(target.name);
      setAlt(target.description);
      setSpoiler(target.spoiler);
    }
  }, [target?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  if (files.length === 0) return null;
  return (
    <>
      <ul className="lng-tray" aria-label={t('lounge.uploadTray.filesToUpload')}>
        {files.map((f) => (
          <li key={f.id} className="lng-tray-card" data-status={f.status}>
            <Thumb file={f.file} />
            <span className="lng-tray-name" title={f.name}>
              {f.spoiler ? 'SPOILER_' : ''}
              {f.name}
            </span>
            <span className="lng-muted lng-tray-size">{formatBytes(f.file.size)}</span>
            {(f.status === 'uploading' || f.status === 'done') && (
              <ProgressBar
                value={Math.round(f.progress * 100)}
                label={t('lounge.uploadTray.uploading', { name: f.name })}
                size={6}
              />
            )}
            {f.status === 'error' && (
              <span className="lng-failed" role="alert">
                {f.error ?? t('lounge.uploadTray.uploadFailed')}
              </span>
            )}
            <span className="lng-tray-actions">
              <button
                type="button"
                className="lng-icon-btn"
                aria-label={t('lounge.uploadTray.edit', { name: f.name })}
                disabled={busy}
                onClick={() => setEditing(f.id)}
              >
                <Icon name="edit" size={16} />
              </button>
              <button
                type="button"
                className="lng-icon-btn lng-danger"
                aria-label={
                  busy
                    ? t('lounge.uploadTray.cancelUpload', { name: f.name })
                    : t('lounge.uploadTray.remove', { name: f.name })
                }
                onClick={() => onRemove(f.id)}
              >
                <Icon name="trash" size={16} />
              </button>
            </span>
          </li>
        ))}
      </ul>
      <Modal
        open={!!target}
        onClose={() => setEditing(null)}
        title={t('lounge.uploadTray.modifyTitle')}
        size="sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setEditing(null)}>
              {t('lounge.uploadTray.cancel')}
            </Button>
            <Button
              onClick={() => {
                if (target)
                  onChange(target.id, {
                    name: name.trim() || target.name,
                    description: alt.trim(),
                    spoiler,
                  });
                setEditing(null);
              }}
            >
              {t('lounge.uploadTray.save')}
            </Button>
          </>
        }
      >
        <form className="lng-form" onSubmit={(e) => e.preventDefault()}>
          <label className="lng-field">
            <span>{t('lounge.uploadTray.filename')}</span>
            <input type="text" value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <label className="lng-field">
            <span>{t('lounge.uploadTray.altText')}</span>
            <input
              type="text"
              value={alt}
              maxLength={1024}
              placeholder={t('lounge.uploadTray.altPlaceholder')}
              onChange={(e) => setAlt(e.target.value)}
            />
          </label>
          <label className="lng-check">
            <input
              type="checkbox"
              checked={spoiler}
              onChange={(e) => setSpoiler(e.target.checked)}
            />
            <span>{t('lounge.uploadTray.markSpoiler')}</span>
          </label>
        </form>
      </Modal>
    </>
  );
}
