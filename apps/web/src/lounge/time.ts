import { formatDate, formatDateTime, formatTime, t } from '../i18n';

const DAY = 86_400_000;

/*
 * Message times follow the language/region and 12/24-hour choice from User Settings > Language (the
 * browser's when left on auto). The i18n runtime holds both (`settings/user/prefs.ts` keeps it in step),
 * so these are thin wrappers that pick the shape of each label.
 */

export const sameDay = (a: Date, b: Date) =>
  a.getFullYear() === b.getFullYear() &&
  a.getMonth() === b.getMonth() &&
  a.getDate() === b.getDate();

export function formatClock(d: Date): string {
  return formatTime(d, { hour: 'numeric', minute: '2-digit' });
}

/** Message timestamp: "Today at 4:05 PM" / "Yesterday at ..." / "MM/DD/YYYY". */
export function formatMessageTime(iso: string, now: Date = new Date()): string {
  const d = new Date(iso);
  if (sameDay(d, now)) return t('lounge.time.todayAt', { time: formatClock(d) });
  if (sameDay(d, new Date(now.getTime() - DAY))) {
    return t('lounge.time.yesterdayAt', { time: formatClock(d) });
  }
  return formatDate(d, { year: 'numeric', month: '2-digit', day: '2-digit' });
}

export const formatFullTime = (iso: string) =>
  formatDateTime(new Date(iso), { dateStyle: 'full', timeStyle: 'short' });

export const formatDividerDate = (iso: string) =>
  formatDate(new Date(iso), { year: 'numeric', month: 'long', day: 'numeric' });
