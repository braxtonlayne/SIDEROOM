/**
 * UTL-CALC-001: time zone arithmetic over `Intl.DateTimeFormat` (no tz database is shipped; the browser's is
 * used). Pure functions: the overlap grid, the best time, `<t:unix:F>` tags. Local times that do not exist
 * (the hour skipped when clocks go forward) are moved forward by the length of the gap, and ambiguous ones
 * (clocks go back) to the earlier instant.
 */

export interface LocalParts {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
  /** 0 is Sunday. */
  weekday: number;
}

const formatters = new Map<string, Intl.DateTimeFormat | null>();

function formatter(zone: string): Intl.DateTimeFormat | null {
  if (formatters.has(zone)) return formatters.get(zone) ?? null;
  let f: Intl.DateTimeFormat | null = null;
  try {
    f = new Intl.DateTimeFormat('en-US', {
      timeZone: zone,
      hourCycle: 'h23',
      year: 'numeric',
      month: 'numeric',
      day: 'numeric',
      hour: 'numeric',
      minute: 'numeric',
      second: 'numeric',
    });
  } catch {
    f = null;
  }
  formatters.set(zone, f);
  return f;
}

export function isValidZone(zone: string): boolean {
  return zone.trim() !== '' && formatter(zone.trim()) !== null;
}

/** The wall-clock fields of an instant in a zone; null for an unknown zone. */
export function localParts(zone: string, ms: number): LocalParts | null {
  const f = formatter(zone);
  if (!f || !Number.isFinite(ms)) return null;
  const get: Record<string, number> = {};
  for (const p of f.formatToParts(new Date(ms))) {
    if (p.type !== 'literal') get[p.type] = Number(p.value);
  }
  const hour = get.hour === 24 ? 0 : (get.hour ?? 0);
  const year = get.year ?? 1970;
  const month = get.month ?? 1;
  const day = get.day ?? 1;
  return {
    year,
    month,
    day,
    hour,
    minute: get.minute ?? 0,
    second: get.second ?? 0,
    weekday: new Date(Date.UTC(year, month - 1, day)).getUTCDay(),
  };
}

/** Minutes the zone's clock is ahead of UTC at that instant (negative when behind). */
export function offsetMinutes(zone: string, ms: number): number {
  const p = localParts(zone, ms);
  if (!p) return 0;
  const asUtc = Date.UTC(p.year, p.month - 1, p.day, p.hour, p.minute, p.second);
  return Math.round((asUtc - Math.floor(ms / 1000) * 1000) / 60000);
}

/** The instant (ms) at which the zone's clock reads the given date and time. */
export function localToUtc(
  zone: string,
  year: number,
  month: number,
  day: number,
  hour: number,
  minute: number,
): number {
  const wall = Date.UTC(year, month - 1, day, hour, minute);
  const t1 = wall - offsetMinutes(zone, wall) * 60000;
  const o2 = offsetMinutes(zone, t1);
  const t2 = wall - o2 * 60000;
  // The second offset settles a guess that landed on the wrong side of a transition. When even that offset
  // does not hold at its own instant the local time was skipped, and the later guess is the same wall time
  // moved forward by the gap, which is what a calendar does.
  return offsetMinutes(zone, t2) === o2 ? t2 : Math.max(t1, t2);
}

/** `UTC+5:30`, `UTC-8`, `UTC`. */
export function formatOffset(minutes: number): string {
  if (minutes === 0) return 'UTC';
  const sign = minutes < 0 ? '-' : '+';
  const abs = Math.abs(minutes);
  const h = Math.floor(abs / 60);
  const m = abs % 60;
  return `UTC${sign}${h}${m ? `:${String(m).padStart(2, '0')}` : ''}`;
}

/** `YYYY-MM-DD` of an instant in a zone. */
export function ymd(zone: string, ms: number): string {
  const p = localParts(zone, ms);
  if (!p) return '1970-01-01';
  return `${String(p.year).padStart(4, '0')}-${String(p.month).padStart(2, '0')}-${String(p.day).padStart(2, '0')}`;
}

export function parseYmd(text: string): { year: number; month: number; day: number } | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text);
  if (!m) return null;
  const [year, month, day] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const d = new Date(Date.UTC(year, month - 1, day));
  if (d.getUTCFullYear() !== year || d.getUTCMonth() !== month - 1 || d.getUTCDate() !== day)
    return null;
  return { year, month, day };
}

/** The browser's zone, or UTC when it cannot say. */
export function browserZone(): string {
  try {
    const z = Intl.DateTimeFormat().resolvedOptions().timeZone;
    return z && isValidZone(z) ? z : 'UTC';
  } catch {
    return 'UTC';
  }
}

const FALLBACK_ZONES = [
  'UTC',
  'Europe/London',
  'Europe/Paris',
  'Europe/Berlin',
  'Europe/Moscow',
  'Africa/Cairo',
  'Africa/Johannesburg',
  'Asia/Dubai',
  'Asia/Kolkata',
  'Asia/Bangkok',
  'Asia/Shanghai',
  'Asia/Tokyo',
  'Australia/Sydney',
  'Pacific/Auckland',
  'America/Sao_Paulo',
  'America/New_York',
  'America/Chicago',
  'America/Denver',
  'America/Los_Angeles',
  'America/Anchorage',
  'Pacific/Honolulu',
];

/** Every zone the browser knows (for the suggestion list); a short list on engines without the call. */
export function allZones(): string[] {
  try {
    const f = (Intl as unknown as { supportedValuesOf?: (k: string) => string[] })
      .supportedValuesOf;
    const list = f ? f('timeZone') : [];
    if (list.length > 0) return list.includes('UTC') ? list : ['UTC', ...list];
  } catch {
    // fall through
  }
  return FALLBACK_ZONES;
}

// ------------------------------------------------------------------ the overlap grid

export interface PersonZone {
  id: string;
  name: string;
  zone: string;
}

export interface GridOptions {
  /** First working hour (0 to 23). */
  workStart: number;
  /** Hour the work day ends, exclusive (1 to 24). */
  workEnd: number;
  skipWeekends: boolean;
}

export const DEFAULT_GRID_OPTIONS: GridOptions = { workStart: 9, workEnd: 17, skipWeekends: true };

export interface GridCell {
  hour: number;
  minute: number;
  /** -1, 0 or 1: the zone's date compared with the first person's date at that column. */
  dayShift: number;
  weekday: number;
  working: boolean;
}

export interface GridColumn {
  utcMs: number;
  cells: GridCell[];
  workingCount: number;
  allWorking: boolean;
}

const dayNumber = (p: LocalParts) => Math.floor(Date.UTC(p.year, p.month - 1, p.day) / 86400000);

/**
 * 24 columns, one per hour of `date` in the FIRST person's zone; each column lists every person's local time
 * and whether it falls in their working hours.
 */
export function buildGrid(
  people: readonly PersonZone[],
  date: string,
  opts: GridOptions = DEFAULT_GRID_OPTIONS,
): GridColumn[] {
  const ymdParts = parseYmd(date);
  const ref = people[0];
  if (!ref || !ymdParts || !isValidZone(ref.zone)) return [];
  const columns: GridColumn[] = [];
  for (let h = 0; h < 24; h += 1) {
    const utcMs = localToUtc(ref.zone, ymdParts.year, ymdParts.month, ymdParts.day, h, 0);
    const refParts = localParts(ref.zone, utcMs);
    const cells: GridCell[] = people.map((p) => {
      const lp = localParts(p.zone, utcMs);
      if (!lp || !refParts) return { hour: 0, minute: 0, dayShift: 0, weekday: 0, working: false };
      const minutes = lp.hour * 60 + lp.minute;
      const day = !opts.skipWeekends || (lp.weekday !== 0 && lp.weekday !== 6);
      return {
        hour: lp.hour,
        minute: lp.minute,
        dayShift: dayNumber(lp) - dayNumber(refParts),
        weekday: lp.weekday,
        working: day && minutes >= opts.workStart * 60 && minutes < opts.workEnd * 60,
      };
    });
    const workingCount = cells.filter((c) => c.working).length;
    columns.push({
      utcMs,
      cells,
      workingCount,
      allWorking: people.length > 0 && workingCount === people.length,
    });
  }
  return columns;
}

/** Runs of consecutive columns where everyone is working: `[start, end]` inclusive indices. */
export function overlapRuns(grid: readonly GridColumn[]): [number, number][] {
  const runs: [number, number][] = [];
  let start = -1;
  grid.forEach((c, i) => {
    if (c.allWorking && start < 0) start = i;
    if (!c.allWorking && start >= 0) {
      runs.push([start, i - 1]);
      start = -1;
    }
  });
  if (start >= 0) runs.push([start, grid.length - 1]);
  return runs;
}

/**
 * The column to suggest: the middle of the longest all-working run; with no such run, the column with the most
 * people working (the first of equals). -1 for an empty grid.
 */
export function bestColumn(grid: readonly GridColumn[]): number {
  if (grid.length === 0) return -1;
  const runs = overlapRuns(grid);
  if (runs.length > 0) {
    const longest = runs.reduce((a, b) => (b[1] - b[0] > a[1] - a[0] ? b : a));
    return Math.floor((longest[0] + longest[1]) / 2);
  }
  let best = 0;
  grid.forEach((c, i) => {
    if (c.workingCount > (grid[best]?.workingCount ?? 0)) best = i;
  });
  return best;
}

// ------------------------------------------------------------------ timestamps

export type TimestampStyle = 't' | 'T' | 'd' | 'D' | 'f' | 'F' | 'R';

/** The tag a message renders in each reader's own zone: `<t:1767225600:F>`. */
export function timestampTag(ms: number, style: TimestampStyle = 'F'): string {
  return `<t:${Math.floor(ms / 1000)}:${style}>`;
}

// ------------------------------------------------------------------ the hand-off to the invite cards (wave 33)

type Listener = (ms: number | null) => void;
let picked: number | null = null;
const listeners = new Set<Listener>();

/** The instant the member last picked (unix ms), so the invite card tool can start from it. */
export const pickedTime = {
  get: (): number | null => picked,
  set: (ms: number | null): void => {
    picked = ms;
    listeners.forEach((l) => l(ms));
  },
  subscribe: (l: Listener): (() => void) => {
    listeners.add(l);
    return () => listeners.delete(l);
  },
};
