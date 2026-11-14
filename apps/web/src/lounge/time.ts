import { useSyncExternalStore } from 'react';
import { formatDate, formatDateTime, formatTime, t } from '../i18n';

const MINUTE = 60_000;
const HOUR = 3_600_000;
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

/**
 * How long ago `iso` was, as the thread row shows it (D-700): under a minute "just now", then "5m ago",
 * "3h ago", "2d ago" up to 30 days, then "more than 30 days ago". A time in the future (a clock that is off)
 * reads "just now". The words come from the catalog (`lounge.time.*`), not from Intl.
 */
export function formatRelativeTime(iso: string, now: number): string {
  const then = Date.parse(iso);
  const diff = Number.isNaN(then) ? 0 : now - then;
  const minutes = Math.floor(diff / MINUTE);
  if (minutes < 1) return t('lounge.time.justNow');
  if (minutes < 60) return t('lounge.time.minutesAgo', { n: minutes });
  const hours = Math.floor(diff / HOUR);
  if (hours < 24) return t('lounge.time.hoursAgo', { n: hours });
  const days = Math.floor(diff / DAY);
  if (days <= 30) return t('lounge.time.daysAgo', { n: days });
  return t('lounge.time.moreThan30Days');
}

/*
 * One shared minute tick for every row that shows a relative time: a single interval for all mounted users,
 * started with the first subscriber and stopped with the last, paused while the document is hidden and fired
 * once when it becomes visible again.
 */
const listeners = new Set<() => void>();
let ticks = 0;
let timer: ReturnType<typeof setInterval> | null = null;

const hidden = () => typeof document !== 'undefined' && document.visibilityState === 'hidden';

function fire() {
  ticks += 1;
  for (const l of [...listeners]) l();
}

function startTimer() {
  if (timer === null && listeners.size > 0 && !hidden()) timer = setInterval(fire, MINUTE);
}

function stopTimer() {
  if (timer !== null) clearInterval(timer);
  timer = null;
}

function onVisibility() {
  if (hidden()) stopTimer();
  else if (listeners.size > 0) {
    fire();
    startTimer();
  }
}

function subscribeTick(listener: () => void): () => void {
  listeners.add(listener);
  if (listeners.size === 1) {
    document.addEventListener('visibilitychange', onVisibility);
    startTimer();
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      stopTimer();
      document.removeEventListener('visibilitychange', onVisibility);
    }
  };
}

/** Re-renders the caller once a minute; returns the current time in ms (read at render). */
export function useMinuteTick(): number {
  useSyncExternalStore(
    subscribeTick,
    () => ticks,
    () => ticks,
  );
  return Date.now();
}
