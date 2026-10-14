import { Button, Modal, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { useT, type MessageKey } from '../../i18n';
import { useSelfPresence } from './selfPresence';

const CLEAR_AFTER: { value: string; labelKey: MessageKey; seconds: number | null }[] = [
  { value: 'never', labelKey: 'lounge.customStatus.clearNever', seconds: null },
  { value: '30m', labelKey: 'lounge.customStatus.clear30m', seconds: 1800 },
  { value: '1h', labelKey: 'lounge.customStatus.clear1h', seconds: 3600 },
  { value: '4h', labelKey: 'lounge.customStatus.clear4h', seconds: 14400 },
  { value: 'today', labelKey: 'lounge.customStatus.clearToday', seconds: -1 },
];

/** Seconds from `now` until the end of the local day (for "Today"). */
export function secondsUntilMidnight(now = new Date()): number {
  const end = new Date(now);
  end.setHours(24, 0, 0, 0);
  return Math.max(60, Math.round((end.getTime() - now.getTime()) / 1000));
}

/** Set or clear the custom status (LNG-HOME-011). */
export function CustomStatusDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const current = useSelfPresence((s) => s.customText);
  const update = useSelfPresence((s) => s.update);
  const [text, setText] = useState(current ?? '');
  const [clearAfter, setClearAfter] = useState('never');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const save = async (next: string | null) => {
    setBusy(true);
    setError(null);
    const opt = CLEAR_AFTER.find((o) => o.value === clearAfter);
    const secs = opt?.seconds === -1 ? secondsUntilMidnight() : (opt?.seconds ?? null);
    try {
      await update({
        custom_status: next
          ? {
              text: next,
              ...(secs ? { expires_at: new Date(Date.now() + secs * 1000).toISOString() } : {}),
            }
          : null,
      });
      onClose();
    } catch (e) {
      setError(e instanceof ApiError && e.message ? e.message : t('lounge.customStatus.saveError'));
      setBusy(false);
    }
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    void save(text.trim() || null);
  };

  return (
    <Modal open onClose={onClose} size="sm" title={t('lounge.customStatus.title')}>
      <form className="lng-form" onSubmit={submit}>
        <TextInput
          label={t('lounge.customStatus.fieldLabel')}
          value={text}
          maxLength={128}
          autoFocus
          error={error}
          onChange={(e) => setText(e.target.value)}
        />
        <label className="lng-field">
          <span className="lng-field-label">{t('lounge.customStatus.clearAfter')}</span>
          <select
            className="lng-select"
            value={clearAfter}
            onChange={(e) => setClearAfter(e.target.value)}
          >
            {CLEAR_AFTER.map((o) => (
              <option key={o.value} value={o.value}>
                {t(o.labelKey)}
              </option>
            ))}
          </select>
        </label>
        <div className="lng-form-actions">
          {current && (
            <Button variant="ghost" onClick={() => void save(null)} disabled={busy}>
              {t('lounge.customStatus.clearStatus')}
            </Button>
          )}
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.customStatus.cancel')}
          </Button>
          <Button type="submit" loading={busy}>
            {t('lounge.customStatus.save')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
