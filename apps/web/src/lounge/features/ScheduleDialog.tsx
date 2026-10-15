import { Button, DateTimePicker, Modal, useToast } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { formatDateTime, useT } from '../../i18n';
import { useSaved } from '../shell/savedMessages';
import { scheduleMessage } from './api';
import './features.css';

/** `YYYY-MM-DDTHH:MM` in local time, the value format of a datetime-local field. */
export function toLocalInput(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** The suggested times: an hour from now, tomorrow at 9:00, and the next Monday at 9:00. */
export function schedulePresets(now: Date): { id: 'hour' | 'tomorrow' | 'monday'; at: Date }[] {
  const hour = new Date(now.getTime() + 60 * 60_000);
  const tomorrow = new Date(now);
  tomorrow.setDate(tomorrow.getDate() + 1);
  tomorrow.setHours(9, 0, 0, 0);
  const monday = new Date(now);
  const ahead = (8 - monday.getDay()) % 7 || 7;
  monday.setDate(monday.getDate() + ahead);
  monday.setHours(9, 0, 0, 0);
  return [
    { id: 'hour', at: hour },
    { id: 'tomorrow', at: tomorrow },
    { id: 'monday', at: monday },
  ];
}

/** A scheduled message must be at least a minute away (the server enforces it too). */
export const MIN_LEAD_MS = 60_000;

/** Schedule the composer's text for later (FTR-SCH-001): the clock button next to the emoji picker opens this. */
export function ScheduleDialog({
  open,
  channelId,
  content,
  onClose,
  onScheduled,
}: {
  open: boolean;
  channelId: string;
  content: string;
  onClose: () => void;
  onScheduled: () => void;
}) {
  const t = useT();
  const toast = useToast();
  const [value, setValue] = useState(() => toLocalInput(schedulePresets(new Date())[0]!.at));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const when = new Date(value);
  const valid = !Number.isNaN(when.getTime());
  const tooSoon = valid && when.getTime() < Date.now() + MIN_LEAD_MS;
  const text = content.trim();

  const submit = async () => {
    if (!valid || tooSoon || !text) return;
    setBusy(true);
    setError(null);
    try {
      const row = await scheduleMessage(channelId, text, when);
      useSaved.getState().addScheduled(row);
      toast.show({
        title: t('lounge.schedule.scheduled', { time: formatDateTime(when) }),
        variant: 'success',
        duration: 3500,
      });
      onScheduled();
    } catch (e) {
      setError(e instanceof ApiError && e.message ? e.message : t('lounge.schedule.failed'));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      size="sm"
      title={t('lounge.schedule.title')}
      description={t('lounge.schedule.description')}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('lounge.schedule.cancel')}
          </Button>
          <Button
            loading={busy}
            disabled={!valid || tooSoon || !text}
            onClick={() => void submit()}
          >
            {t('lounge.schedule.confirm')}
          </Button>
        </>
      }
    >
      <div className="lng-sched-presets" role="group" aria-label={t('lounge.schedule.presets')}>
        {schedulePresets(new Date()).map((p) => (
          <Button
            key={p.id}
            size="sm"
            variant="secondary"
            onClick={() => setValue(toLocalInput(p.at))}
          >
            {p.id === 'hour'
              ? t('lounge.schedule.presetHour')
              : p.id === 'tomorrow'
                ? t('lounge.schedule.presetTomorrow')
                : t('lounge.schedule.presetMonday')}
          </Button>
        ))}
      </div>
      <DateTimePicker
        label={t('lounge.schedule.sendAt')}
        value={value}
        min={toLocalInput(new Date(Date.now() + MIN_LEAD_MS))}
        onValueChange={setValue}
        error={tooSoon ? t('lounge.schedule.tooSoon') : undefined}
      />
      <p className="lng-muted">{t('lounge.schedule.messageLabel')}</p>
      <blockquote className="lng-sched-preview">{text || t('lounge.schedule.empty')}</blockquote>
      {error && (
        <p role="alert" className="lng-form-error">
          {error}
        </p>
      )}
    </Modal>
  );
}
