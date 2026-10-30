/**
 * UTL-CRD-001: turning the time a host picked ("Saturday 20:00 in Europe/Berlin") into the instant the card stores
 * (unix milliseconds, UTC). The browser's Intl does the zone arithmetic, so there is no zone database in the bundle.
 * The card shows the instant in each reader's own zone; the zone the host picked it in is only a label.
 */

const LOCAL = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?$/;

/** The zone of this device, or UTC when the browser does not say. */
export function deviceZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
  } catch {
    return 'UTC';
  }
}

/** All zone names the browser knows (a short list where `Intl.supportedValuesOf` is missing). */
export function zoneNames(): string[] {
  const supported = (Intl as unknown as { supportedValuesOf?: (k: string) => string[] })
    .supportedValuesOf;
  let names: string[] = [];
  try {
    names = supported ? supported('timeZone') : [];
  } catch {
    names = [];
  }
  const own = deviceZone();
  const all = new Set<string>(['UTC', own, ...names]);
  return [...all].sort((a, b) => a.localeCompare(b));
}

/** `zone` minus UTC at the instant `ms`, in milliseconds. */
function offsetAt(ms: number, zone: string): number {
  const f = new Intl.DateTimeFormat('en-US', {
    timeZone: zone,
    hourCycle: 'h23',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
  const p: Record<string, number> = {};
  for (const part of f.formatToParts(new Date(ms))) {
    if (part.type !== 'literal') p[part.type] = Number(part.value);
  }
  const asUtc = Date.UTC(p.year!, p.month! - 1, p.day!, p.hour!, p.minute!, p.second!);
  return asUtc - Math.floor(ms / 1000) * 1000;
}

/**
 * The instant at which the wall clock in `zone` reads `local` (`YYYY-MM-DDTHH:mm`, what a `datetime-local` input
 * gives). `null` for an unreadable value or an unknown zone. A wall time that does not exist (the hour skipped when
 * clocks go forward) resolves to the instant after the gap; an ambiguous one (clocks go back) to the earlier.
 */
export function zonedToUtcMs(local: string, zone: string): number | null {
  const m = LOCAL.exec(local);
  if (!m) return null;
  const [y, mo, d, h, mi, s] = [m[1], m[2], m[3], m[4], m[5], m[6] ?? '0'].map(Number) as [
    number,
    number,
    number,
    number,
    number,
    number,
  ];
  const wall = Date.UTC(y, mo - 1, d, h, mi, s);
  if (Number.isNaN(wall)) return null;
  try {
    const first = offsetAt(wall, zone);
    let t = wall - first;
    const second = offsetAt(t, zone);
    if (second !== first) t = wall - second;
    return t;
  } catch {
    return null;
  }
}
