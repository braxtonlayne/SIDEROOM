import { Button } from '@sideroom/ui';
import { useId, useRef, useState } from 'react';
import { cdnUrl, type CdnKind } from '../../../store';
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
}: {
  label: string;
  hint?: string;
  ownerId: string;
  kind?: CdnKind;
  value: string | null;
  onChange: (next: string | null) => void;
  shape?: 'square' | 'wide';
  disabled?: boolean;
}) {
  const input = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const hintId = useId();
  const src = value
    ? value.startsWith('data:')
      ? value
      : cdnUrl(kind, ownerId, value, { size: shape === 'square' ? 128 : 512 })
    : null;

  const pick = async (file: File | undefined) => {
    if (!file) return;
    if (!/\.(png|jpe?g|gif|webp)$/i.test(file.name)) {
      setError(`${file.name} is not a PNG, JPEG, GIF or WebP image.`);
      return;
    }
    if (file.size > MAX_IMAGE_BYTES) {
      setError(`${file.name} is larger than 8 MB.`);
      return;
    }
    try {
      const uri = await readDataUri(file);
      setError(null);
      onChange(uri);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Could not read that file.');
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
          aria-label={src ? `${label} preview` : `No ${label.toLowerCase()} set`}
        >
          {src ? <img src={src} alt="" /> : <span aria-hidden="true">None</span>}
        </div>
        <div className="ss-actions">
          <Button
            variant="secondary"
            disabled={disabled}
            aria-describedby={hint ? hintId : undefined}
            onClick={() => input.current?.click()}
          >
            {value ? `Change ${label.toLowerCase()}` : `Upload ${label.toLowerCase()}`}
          </Button>
          {value && (
            <Button variant="ghost" disabled={disabled} onClick={() => onChange(null)}>
              {`Remove ${label.toLowerCase()}`}
            </Button>
          )}
        </div>
        <input
          ref={input}
          type="file"
          accept={ACCEPT}
          hidden
          data-testid={`file-${label.toLowerCase().replace(/\s+/g, '-')}`}
          aria-label={`${label} file`}
          onChange={(e) => {
            void pick(e.target.files?.[0]);
            e.target.value = '';
          }}
        />
      </div>
      {error && (
        <p role="alert" className="ss-error-text">
          {error}
        </p>
      )}
    </div>
  );
}
