import { Button } from '@sideroom/ui';
import { useId, useRef, useState } from 'react';
import { useT } from '../../i18n';
import { cdnUrl } from '../../store';

/** Largest cover the API accepts (sr-storage `MediaKind::EventCover`). */
export const MAX_COVER_BYTES = 2 * 1024 * 1024;
const ACCEPT = '.png,.jpg,.jpeg,.gif';

function readDataUri(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(new Error('read'));
    r.readAsDataURL(file);
  });
}

/**
 * Pick, preview and remove the cover image of an event (G-93): JPEG, PNG or GIF up to 2 MiB.
 * `value` is the stored hash, a `data:` URI chosen in this session, or null (none / removed).
 */
export function EventCoverField({
  eventId,
  value,
  onChange,
  disabled,
}: {
  /** The event the stored hash belongs to; none while creating. */
  eventId?: string | undefined;
  value: string | null;
  onChange: (next: string | null) => void;
  disabled?: boolean | undefined;
}) {
  const t = useT();
  const input = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const hintId = useId();
  const src = value
    ? value.startsWith('data:')
      ? value
      : eventId
        ? cdnUrl('guild-events', eventId, value, { size: 512 })
        : null
    : null;

  const pick = async (file: File | undefined) => {
    if (!file) return;
    if (!/\.(png|jpe?g|gif)$/i.test(file.name)) {
      setError(t('lounge.eventCover.badType', { name: file.name }));
      return;
    }
    if (file.size > MAX_COVER_BYTES) {
      setError(t('lounge.eventCover.tooLarge', { name: file.name }));
      return;
    }
    try {
      const uri = await readDataUri(file);
      setError(null);
      onChange(uri);
    } catch {
      setError(t('lounge.eventCover.readFailed'));
    }
  };

  return (
    <div className="lng-event-cover-field">
      <span className="lng-field-label">{t('lounge.eventCover.label')}</span>
      <p id={hintId} className="lng-muted">
        {t('lounge.eventCover.hint')}
      </p>
      <div className="lng-event-cover-row">
        <div
          className="lng-event-cover-preview"
          role="img"
          aria-label={src ? t('lounge.eventCover.preview') : t('lounge.eventCover.none')}
        >
          {src ? (
            <img src={src} alt="" />
          ) : (
            <span aria-hidden="true">{t('lounge.eventCover.noneShort')}</span>
          )}
        </div>
        <div className="lng-event-cover-actions">
          <Button
            size="sm"
            variant="secondary"
            disabled={disabled}
            aria-describedby={hintId}
            onClick={() => input.current?.click()}
          >
            {value ? t('lounge.eventCover.change') : t('lounge.eventCover.upload')}
          </Button>
          {value && (
            <Button size="sm" variant="ghost" disabled={disabled} onClick={() => onChange(null)}>
              {t('lounge.eventCover.remove')}
            </Button>
          )}
        </div>
        <input
          ref={input}
          type="file"
          accept={ACCEPT}
          hidden
          data-testid="event-cover-file"
          aria-label={t('lounge.eventCover.file')}
          onChange={(e) => {
            void pick(e.target.files?.[0]);
            e.target.value = '';
          }}
        />
      </div>
      {error && (
        <p role="alert" className="lng-failed">
          {error}
        </p>
      )}
    </div>
  );
}
