import { Button, Modal, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { useSelfPresence } from './selfPresence';

const CLEAR_AFTER: { value: string; label: string; seconds: number | null }[] = [
  { value: 'never', label: "Don't clear", seconds: null },
  { value: '30m', label: '30 minutes', seconds: 1800 },
  { value: '1h', label: '1 hour', seconds: 3600 },
  { value: '4h', label: '4 hours', seconds: 14400 },
  { value: 'today', label: 'Today', seconds: -1 },
];

/** Seconds from `now` until the end of the local day (for "Today"). */
export function secondsUntilMidnight(now = new Date()): number {
  const end = new Date(now);
  end.setHours(24, 0, 0, 0);
  return Math.max(60, Math.round((end.getTime() - now.getTime()) / 1000));
}

/** Set or clear the custom status (LNG-HOME-011). */
export function CustomStatusDialog({ onClose }: { onClose: () => void }) {
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
      setError(e instanceof ApiError && e.message ? e.message : 'Could not save your status.');
      setBusy(false);
    }
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    void save(text.trim() || null);
  };

  return (
    <Modal open onClose={onClose} size="sm" title="Set a custom status">
      <form className="lng-form" onSubmit={submit}>
        <TextInput
          label="What's happening?"
          value={text}
          maxLength={128}
          autoFocus
          error={error}
          onChange={(e) => setText(e.target.value)}
        />
        <label className="lng-field">
          <span className="lng-field-label">Clear after</span>
          <select
            className="lng-select"
            value={clearAfter}
            onChange={(e) => setClearAfter(e.target.value)}
          >
            {CLEAR_AFTER.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
        <div className="lng-form-actions">
          {current && (
            <Button variant="ghost" onClick={() => void save(null)} disabled={busy}>
              Clear Status
            </Button>
          )}
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" loading={busy}>
            Save
          </Button>
        </div>
      </form>
    </Modal>
  );
}
