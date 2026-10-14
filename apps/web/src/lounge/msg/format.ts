import { t } from '../../i18n';
/** Small formatting helpers shared by the message renderers. */

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

/** 83.2 -> "1:23". */
export function formatDuration(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** Only http(s) URLs from message data are ever put into href or src. */
export function isHttpUrl(u: string | undefined | null): u is string {
  if (!u) return false;
  try {
    const p = new URL(u, 'http://x.invalid').protocol;
    return p === 'http:' || p === 'https:';
  } catch {
    return false;
  }
}

/** "3 hours left", "Poll closed". */
export function timeLeft(iso: string | null, now = Date.now()): string | null {
  if (!iso) return null;
  const ms = new Date(iso).getTime() - now;
  if (Number.isNaN(ms)) return null;
  if (ms <= 0) return t('lounge.poll.closed');
  const min = Math.ceil(ms / 60_000);
  if (min < 60) return t('lounge.msgFormat.minutesLeft', { n: min });
  const h = Math.ceil(min / 60);
  if (h < 48) return t('lounge.msgFormat.hoursLeft', { n: h });
  const d = Math.ceil(h / 24);
  return t('lounge.msgFormat.daysLeft', { n: d });
}
