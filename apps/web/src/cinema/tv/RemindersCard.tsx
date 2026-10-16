// "My reminders" on the Live TV page (H-01): the lead-time choice (settings blob key `tv.reminder_lead_minutes`,
// 0 to 120, default 5) and the pending reminders with Remove. Renders nothing when the server has no such route.
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../api/client';
import { formatDate, useT } from '../../i18n';
import { isNotAvailable, listReminders, setReminder, tvKeys } from './api';
import { timeLabel } from './model';

interface SettingsResponse {
  version: number;
  settings: Record<string, unknown>;
}

export const LEAD_CHOICES = [0, 1, 2, 5, 10, 15, 30, 60, 120] as const;
export const DEFAULT_LEAD = 5;

/** `tv.reminder_lead_minutes` out of the blob; anything that is not a number from 0 to 120 is the default. */
export function leadFrom(settings: Record<string, unknown> | undefined): number {
  const tv = settings?.tv;
  const v = tv && typeof tv === 'object' ? (tv as Record<string, unknown>).reminder_lead_minutes : undefined;
  return typeof v === 'number' && Number.isInteger(v) && v >= 0 && v <= 120 ? v : DEFAULT_LEAD;
}

export function RemindersCard({ tz }: { tz: string | null }) {
  const t = useT();
  const qc = useQueryClient();
  const list = useQuery({
    queryKey: tvKeys.reminders(),
    queryFn: listReminders,
    retry: false,
    staleTime: 30_000,
  });
  const blob = useQuery({
    queryKey: tvKeys.settings(),
    queryFn: () => apiFetch<SettingsResponse>('/users/@me/settings', { retries: 0 }),
    staleTime: 60_000,
  });
  const lead = leadFrom(blob.data?.settings);
  const saveLead = useMutation({
    mutationFn: (minutes: number) =>
      apiFetch<SettingsResponse>('/users/@me/settings', {
        method: 'PATCH',
        json: { settings: { tv: { reminder_lead_minutes: minutes } } },
        retries: 0,
      }),
    onSuccess: (res) => qc.setQueryData(tvKeys.settings(), res),
  });
  const remove = useMutation({
    mutationFn: (blockId: string) => setReminder(blockId, false),
    onSuccess: () => qc.invalidateQueries({ queryKey: tvKeys.reminders() }),
  });
  if (list.isError && isNotAvailable(list.error)) return null;
  const items = list.data ?? [];
  return (
    <section className="tv-reminders" aria-label={t('cinema.reminders.heading')}>
      <h2 className="tv-pop-sub">{t('cinema.reminders.heading')}</h2>
      <label className="tv-reminders-lead">
        {t('cinema.reminders.lead')}
        <select
          value={lead}
          disabled={saveLead.isPending}
          onChange={(e) => saveLead.mutate(Number(e.target.value))}
        >
          {LEAD_CHOICES.map((m) => (
            <option key={m} value={m}>
              {t('cinema.reminders.minutes', { count: m })}
            </option>
          ))}
        </select>
      </label>
      {(saveLead.isError || remove.isError) && (
        <p role="alert" className="tv-pop-err">
          {t('cinema.popover.reminderError')}
        </p>
      )}
      {items.length === 0 ? (
        <p>{t('cinema.reminders.none')}</p>
      ) : (
        <ul className="tv-pop-airings">
          {items.map((r) => (
            <li key={r.block_id}>
              <span className="tv-mono">
                {formatDate(r.starts_at, { weekday: 'short' })} {timeLabel(r.starts_at, tz)}
              </span>
              <span>{r.title ?? t('cinema.reminders.gone')}</span>
              <button
                type="button"
                className="tv-btn"
                disabled={remove.isPending}
                onClick={() => remove.mutate(r.block_id)}
              >
                {t('cinema.reminders.remove')}
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
