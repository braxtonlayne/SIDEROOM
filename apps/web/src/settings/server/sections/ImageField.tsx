import { Button } from '@sideroom/ui';
import { useId, useRef, useState } from 'react';
import { useT } from '../../../i18n';
import { cdnUrl, type CdnKind } from '../../../store';
import { ImageCropDialog } from '../../user/ImageCropDialog';
import { readDataUri } from './Expressions';

/** Largest file the API accepts for a guild image (sr-storage MAX_UPLOAD_BYTES). */
export const MAX_IMAGE_BYTES = 8 * 1024 * 1024;
const ACCEPT = '.png,.jpg,.jpeg,.gif,.webp';

/**
 * Pick, preview and remove one guild image (icon, banner, invite splash).
 * `value` is the stored hash, a `data:` URI chosen in this session, or null (none / removed).
 */
export function ImageField({
  label,
  hint,
  ownerId,
  kind = 'icons',
  value,
  onChange,
  shape = 'wide',
  disabled,
  crop,
}: {
  label: string;
  hint?: string;
  ownerId: string;
  kind?: CdnKind;
  value: string | null;
  onChange: (next: string | null) => void;
  shape?: 'square' | 'wide';
  disabled?: boolean;
  /** Ask for a crop window (aspect = width / height) before the picture is used; GIFs skip it so they keep moving. */
  crop?: { aspect: number; outWidth: number };
}) {
  const t = useT();
  const input = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const hintId = useId();
  const [cropFile, setCropFile] = useState<File | null>(null);
  const src = value
    ? value.startsWith('data:')
      ? value
      : cdnUrl(kind, ownerId, value, { size: shape === 'square' ? 128 : 512 })
    : null;

  const pick = async (file: File | undefined) => {
    if (!file) return;
    if (!/\.(png|jpe?g|gif|webp)$/i.test(file.name)) {
      setError(t('settings.imageField.badType', { name: file.name }));
      return;
    }
    if (file.size > MAX_IMAGE_BYTES) {
      setError(t('settings.imageField.tooLarge', { name: file.name }));
      return;
    }
    if (crop && !/.gif$/i.test(file.name)) {
      setError(null);
      setCropFile(file);
      return;
    }
    try {
      const uri = await readDataUri(file);
      setError(null);
      onChange(uri);
    } catch (e) {
      setError(e instanceof Error ? e.message : t('settings.imageField.readFailed'));
    }
  };

  return (
    <div className="ss-image-field">
      <span className="ss-field-label">{label}</span>
      {hint && (
        <p id={hintId} className="ss-sub">
          {hint}
        </p>
      )}
      <div className="ss-image-row">
        <div
          className={`ss-image-preview ss-image-${shape}`}
          role="img"
          aria-label={
            src
              ? t('settings.imageField.preview', { label })
              : t('settings.imageField.none', { label: label.toLowerCase() })
          }
        >
          {src ? (
            <img src={src} alt="" />
          ) : (
            <span aria-hidden="true">{t('settings.imageField.noneShort')}</span>
          )}
        </div>
        <div className="ss-actions">
          <Button
            variant="secondary"
            disabled={disabled}
            aria-describedby={hint ? hintId : undefined}
            onClick={() => input.current?.click()}
          >
            {value
              ? t('settings.imageField.change', { label: label.toLowerCase() })
              : t('settings.imageField.upload', { label: label.toLowerCase() })}
          </Button>
          {value && (
            <Button variant="ghost" disabled={disabled} onClick={() => onChange(null)}>
              {t('settings.imageField.remove', { label: label.toLowerCase() })}
            </Button>
          )}
        </div>
        <input
          ref={input}
          type="file"
          accept={ACCEPT}
          hidden
          data-testid={`file-${label.toLowerCase().replace(/\s+/g, '-')}`}
          aria-label={t('settings.imageField.file', { label })}
          onChange={(e) => {
            void pick(e.target.files?.[0]);
            e.target.value = '';
          }}
        />
      </div>
      {crop && (
        <ImageCropDialog
          open={!!cropFile}
          file={cropFile}
          title={t('settings.imageField.cropTitle', { label })}
          aspect={crop.aspect}
          outWidth={crop.outWidth}
          onCancel={() => setCropFile(null)}
          onApply={(uri) => {
            setCropFile(null);
            onChange(uri);
          }}
        />
      )}
      {error && (
        <p role="alert" className="ss-error-text">
          {error}
        </p>
      )}
    </div>
  );
}
