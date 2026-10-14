// One artwork slot: shows the current image, previews a chosen file locally, then uploads it through the artwork
// reservation route (presigned PUT) and hands the object key to the caller to save.
import { Button, ProgressBar } from '@sideroom/ui';
import { useEffect, useId, useState } from 'react';
import { useT } from '../../i18n';
import { errorText } from '../ui';
import { ARTWORK_TYPES, type ArtworkSlot } from './api';

const MAX_BYTES = 10 * 1024 * 1024;

export interface ArtworkPickerProps {
  label: string;
  slot: ArtworkSlot;
  currentUrl?: string | null;
  /** Upload the file and persist its key; resolve when saved. */
  onUpload: (file: File, onProgress: (fraction: number) => void) => Promise<void>;
  /** Clear the artwork (omit to hide the button). */
  onClear?: () => Promise<void>;
  disabled?: boolean;
  /** Wide frame for backdrops. */
  wide?: boolean;
}

export function ArtworkPicker({
  label,
  currentUrl,
  onUpload,
  onClear,
  disabled,
  wide,
}: ArtworkPickerProps) {
  const t = useT();
  const inputId = useId();
  const [file, setFile] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!file) {
      setPreview(null);
      return;
    }
    const url = URL.createObjectURL(file);
    setPreview(url);
    return () => URL.revokeObjectURL(url);
  }, [file]);

  const pick = (f: File | undefined) => {
    setError(null);
    if (!f) return setFile(null);
    if (!ARTWORK_TYPES.includes(f.type)) {
      setFile(null);
      return setError(t('admin.artworkPicker.badType'));
    }
    if (f.size > MAX_BYTES) {
      setFile(null);
      return setError(t('admin.artworkPicker.tooLarge'));
    }
    setFile(f);
  };

  const upload = async () => {
    if (!file) return;
    setError(null);
    setProgress(0);
    try {
      await onUpload(file, setProgress);
      setFile(null);
    } catch (e) {
      setError(errorText(e, t('admin.artworkPicker.uploadFailed')));
    } finally {
      setProgress(null);
    }
  };

  const shown = preview ?? currentUrl ?? null;
  return (
    <div className="adm-art">
      <div className="ss-field-label">{label}</div>
      <div className={`adm-art-frame${wide ? ' adm-art-wide' : ''}`}>
        {shown ? (
          <img src={shown} alt={t('admin.artworkPicker.previewAlt', { label })} />
        ) : (
          <span className="ss-na">
            {t('admin.artworkPicker.none', { label: label.toLowerCase() })}
          </span>
        )}
      </div>
      <div className="adm-actions">
        <input
          id={inputId}
          className="adm-file"
          type="file"
          accept={ARTWORK_TYPES.join(',')}
          disabled={disabled || progress !== null}
          onChange={(e) => pick(e.target.files?.[0])}
          aria-label={t('admin.artworkPicker.chooseFile', { label: label.toLowerCase() })}
        />
        {file && (
          <Button
            size="sm"
            loading={progress !== null}
            disabled={disabled}
            onClick={() => void upload()}
          >
            {t('admin.artworkPicker.upload', { label: label.toLowerCase() })}
          </Button>
        )}
        {file && progress === null && (
          <Button size="sm" variant="ghost" onClick={() => setFile(null)}>
            {t('admin.artworkPicker.discard')}
          </Button>
        )}
        {onClear && currentUrl && !file && (
          <Button
            size="sm"
            variant="ghost"
            disabled={disabled}
            onClick={() => void onClear().catch((e: unknown) => setError(errorText(e)))}
          >
            {t('admin.artworkPicker.remove', { label: label.toLowerCase() })}
          </Button>
        )}
      </div>
      {progress !== null && (
        <ProgressBar
          value={Math.round(progress * 100)}
          label={t('admin.artworkPicker.uploading', { label: label.toLowerCase() })}
        />
      )}
      {error && (
        <p role="alert" className="adm-art-error">
          {error}
        </p>
      )}
    </div>
  );
}
