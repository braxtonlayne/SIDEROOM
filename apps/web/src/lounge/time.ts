import { usePrefsStore } from '../settings/user/prefs';

const DAY = 86_400_000;

/** The language/region and 12/24-hour choice from User Settings > Language (the browser's when left on auto). */
function localeOptions(): { locale: string | undefined; hour12: boolean | undefined } {
  const { locale, timeFormat } = usePrefsStore.getState().prefs;
  return {
    locale: locale === 'auto' ? undefined : locale,
    hour12: timeFormat === 'auto' ? undefined : timeFormat === '12h',
  };
}

export const sameDay = (a: Date, b: Date) =>
  a.getFullYear() === b.getFullYear() &&
  a.getMonth() === b.getMonth() &&
  a.getDate() === b.getDate();

export function formatClock(d: Date): string {
  const { locale, hour12 } = localeOptions();
  return d.toLocaleTimeString(locale, { hour: 'numeric', minute: '2-digit', hour12 });
}

/** Message timestamp: "Today at 4:05 PM" / "Yesterday at ..." / "MM/DD/YYYY". */
export function formatMessageTime(iso: string, now: Date = new Date()): string {
  const d = new Date(iso);
  if (sameDay(d, now)) return `Today at ${formatClock(d)}`;
  if (sameDay(d, new Date(now.getTime() - DAY))) return `Yesterday at ${formatClock(d)}`;
  return d.toLocaleDateString(localeOptions().locale, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  });
}

export const formatFullTime = (iso: string) => {
  const { locale, hour12 } = localeOptions();
  return new Date(iso).toLocaleString(locale, { dateStyle: 'full', timeStyle: 'short', hour12 });
};

export const formatDividerDate = (iso: string) =>
  new Date(iso).toLocaleDateString(localeOptions().locale, {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });
