import { Button, Icon, Modal, ProgressBar } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
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
      <ul className="lng-tray" aria-label="Files to upload">
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
                label={`Uploading ${f.name}`}
                size={6}
              />
            )}
            {f.status === 'error' && (
              <span className="lng-failed" role="alert">
                {f.error ?? 'Upload failed.'}
              </span>
            )}
            <span className="lng-tray-actions">
              <button
                type="button"
                className="lng-icon-btn"
                aria-label={`Edit ${f.name}`}
                disabled={busy}
                onClick={() => setEditing(f.id)}
              >
                <Icon name="edit" size={16} />
              </button>
              <button
                type="button"
                className="lng-icon-btn lng-danger"
                aria-label={busy ? `Cancel upload of ${f.name}` : `Remove ${f.name}`}
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
        title="Modify Attachment"
        size="sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setEditing(null)}>
              Cancel
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
              Save
            </Button>
          </>
        }
      >
        <form className="lng-form" onSubmit={(e) => e.preventDefault()}>
          <label className="lng-field">
            <span>Filename</span>
            <input type="text" value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <label className="lng-field">
            <span>Alt text</span>
            <input
              type="text"
              value={alt}
              maxLength={1024}
              placeholder="Describe this for people who cannot see it"
              onChange={(e) => setAlt(e.target.value)}
            />
          </label>
          <label className="lng-check">
            <input
              type="checkbox"
              checked={spoiler}
              onChange={(e) => setSpoiler(e.target.checked)}
            />
            <span>Mark as spoiler</span>
          </label>
        </form>
      </Modal>
    </>
  );
}
