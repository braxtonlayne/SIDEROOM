import type { TimestampStyle } from "./ast";

export interface TimestampFormatOptions {
  locale?: string;
  timeZone?: string;
  /** Reference "now" in ms for relative timestamps. */
  now?: number;
}

const OPTIONS: Record<Exclude<TimestampStyle, "relative">, Intl.DateTimeFormatOptions> = {
  short_time: { hour: "numeric", minute: "2-digit" },
  long_time: { hour: "numeric", minute: "2-digit", second: "2-digit" },
  short_date: { year: "numeric", month: "numeric", day: "numeric" },
  long_date: { year: "numeric", month: "long", day: "numeric" },
  short_date_time: {
    year: "numeric",
    month: "long",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  },
  long_date_time: {
    weekday: "long",
    year: "numeric",
    month: "long",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  },
};

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 86400],
  ["month", 30 * 86400],
  ["day", 86400],
  ["hour", 3600],
  ["minute", 60],
  ["second", 1],
];

export function formatRelative(unix: number, opts: TimestampFormatOptions = {}): string {
  const now = (opts.now ?? Date.now()) / 1000;
  const diff = unix - now;
  const abs = Math.abs(diff);
  const rtf = new Intl.RelativeTimeFormat(opts.locale, { numeric: "auto" });
  for (const [unit, secs] of UNITS) {
    if (abs >= secs || unit === "second") {
      return rtf.format(Math.trunc(diff / secs), unit);
    }
  }
  return rtf.format(0, "second");
}

/** Format a Discord-style timestamp. `null` style means the default (`short_date_time`). */
export function formatTimestamp(
  unix: number,
  style: TimestampStyle | null,
  opts: TimestampFormatOptions = {},
): string {
  const s = style ?? "short_date_time";
  if (!Number.isFinite(unix) || Math.abs(unix) > 8.64e12) return "Invalid Date";
  if (s === "relative") return formatRelative(unix, opts);
  const fmt = new Intl.DateTimeFormat(opts.locale, { ...OPTIONS[s], timeZone: opts.timeZone });
  return fmt.format(new Date(unix * 1000));
}

/** Full date/time for the tooltip / `title`. */
export function formatTimestampTitle(unix: number, opts: TimestampFormatOptions = {}): string {
  return formatTimestamp(unix, "long_date_time", opts);
}
