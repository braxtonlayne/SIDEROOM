import { describe, expect, it } from 'vitest';
import { addPerson, loadPeople, MAX_PEOPLE } from '../people';
import {
  allZones,
  bestColumn,
  buildGrid,
  DEFAULT_GRID_OPTIONS,
  formatOffset,
  isValidZone,
  localParts,
  localToUtc,
  offsetMinutes,
  overlapRuns,
  parseYmd,
  pickedTime,
  timestampTag,
  ymd,
  type PersonZone,
} from '../zones';

const utc = (iso: string) => Date.parse(iso);
const p = (id: string, zone: string): PersonZone => ({ id, name: id, zone });

describe('offsets (fixed zones, no clock)', () => {
  const cases: [string, string, number][] = [
    ['UTC', '2026-07-01T12:00:00Z', 0],
    ['America/New_York', '2026-01-15T12:00:00Z', -300],
    ['America/New_York', '2026-07-15T12:00:00Z', -240],
    ['Europe/Berlin', '2026-01-15T12:00:00Z', 60],
    ['Europe/Berlin', '2026-07-15T12:00:00Z', 120],
    ['Europe/London', '2026-07-15T12:00:00Z', 60],
    ['Asia/Kolkata', '2026-07-15T12:00:00Z', 330],
    ['Asia/Kathmandu', '2026-07-15T12:00:00Z', 345],
    ['Asia/Tokyo', '2026-07-15T12:00:00Z', 540],
    ['Pacific/Kiritimati', '2026-07-15T12:00:00Z', 840],
    ['Pacific/Honolulu', '2026-07-15T12:00:00Z', -600],
    ['Australia/Lord_Howe', '2026-01-15T12:00:00Z', 660],
    ['Australia/Lord_Howe', '2026-07-15T12:00:00Z', 630],
    ['Australia/Sydney', '2026-01-15T12:00:00Z', 660],
    ['Australia/Sydney', '2026-07-15T12:00:00Z', 600],
  ];
  it.each(cases)('%s at %s is %i minutes from UTC', (zone, iso, want) => {
    expect(offsetMinutes(zone, utc(iso))).toBe(want);
  });

  it('switches on the day the clocks change (New York, 2026)', () => {
    expect(offsetMinutes('America/New_York', utc('2026-03-08T06:59:00Z'))).toBe(-300);
    expect(offsetMinutes('America/New_York', utc('2026-03-08T07:00:00Z'))).toBe(-240);
    expect(offsetMinutes('America/New_York', utc('2026-11-01T05:59:00Z'))).toBe(-240);
    expect(offsetMinutes('America/New_York', utc('2026-11-01T06:00:00Z'))).toBe(-300);
  });

  it('knows an unknown zone', () => {
    expect(isValidZone('Europe/Berlin')).toBe(true);
    expect(isValidZone('UTC')).toBe(true);
    expect(isValidZone('Mars/Olympus_Mons')).toBe(false);
    expect(isValidZone('')).toBe(false);
    expect(localParts('Mars/Olympus_Mons', 0)).toBeNull();
    expect(offsetMinutes('Mars/Olympus_Mons', 0)).toBe(0);
  });

  it('reads wall clock fields, midnight as hour 0', () => {
    const lp = localParts('Asia/Tokyo', utc('2026-07-15T15:00:00Z'))!;
    expect(lp).toMatchObject({ year: 2026, month: 7, day: 16, hour: 0, minute: 0, weekday: 4 });
    expect(ymd('Asia/Tokyo', utc('2026-07-15T15:00:00Z'))).toBe('2026-07-16');
    expect(ymd('America/New_York', utc('2026-07-15T03:00:00Z'))).toBe('2026-07-14');
  });

  it('formats offsets', () => {
    expect(formatOffset(0)).toBe('UTC');
    expect(formatOffset(330)).toBe('UTC+5:30');
    expect(formatOffset(-480)).toBe('UTC-8');
    expect(formatOffset(345)).toBe('UTC+5:45');
    expect(formatOffset(840)).toBe('UTC+14');
  });
});

describe('local time to an instant', () => {
  it('converts ordinary times', () => {
    expect(localToUtc('America/New_York', 2026, 7, 1, 9, 0)).toBe(utc('2026-07-01T13:00:00Z'));
    expect(localToUtc('America/New_York', 2026, 1, 1, 9, 0)).toBe(utc('2026-01-01T14:00:00Z'));
    expect(localToUtc('Asia/Kolkata', 2026, 1, 1, 0, 0)).toBe(utc('2025-12-31T18:30:00Z'));
    expect(localToUtc('UTC', 2026, 5, 5, 5, 5)).toBe(utc('2026-05-05T05:05:00Z'));
    expect(localToUtc('Pacific/Kiritimati', 2026, 5, 5, 0, 0)).toBe(utc('2026-05-04T10:00:00Z'));
  });

  it('moves a skipped hour forward by the gap', () => {
    // 02:30 on 2026-03-08 does not exist in New York: clocks jump from 02:00 to 03:00.
    expect(localToUtc('America/New_York', 2026, 3, 8, 2, 30)).toBe(utc('2026-03-08T07:30:00Z'));
    expect(localToUtc('America/New_York', 2026, 3, 8, 1, 59)).toBe(utc('2026-03-08T06:59:00Z'));
    expect(localToUtc('America/New_York', 2026, 3, 8, 3, 0)).toBe(utc('2026-03-08T07:00:00Z'));
  });

  it('takes the earlier instant of a repeated hour', () => {
    // 01:30 on 2026-11-01 happens twice in New York; the first is still daylight time.
    expect(localToUtc('America/New_York', 2026, 11, 1, 1, 30)).toBe(utc('2026-11-01T05:30:00Z'));
    expect(localToUtc('America/New_York', 2026, 11, 1, 2, 30)).toBe(utc('2026-11-01T07:30:00Z'));
  });

  it('round-trips ordinary instants through the wall clock in many zones', () => {
    const zones = [
      'UTC',
      'America/New_York',
      'America/Los_Angeles',
      'Europe/Berlin',
      'Europe/London',
      'Asia/Kolkata',
      'Asia/Kathmandu',
      'Asia/Tokyo',
      'Australia/Lord_Howe',
      'Pacific/Auckland',
      'Pacific/Kiritimati',
    ];
    let checked = 0;
    for (const zone of zones) {
      for (let day = 0; day < 365; day += 7) {
        // Mid-day instants: far from every transition, so the wall time is unambiguous.
        const ms = Date.UTC(2026, 0, 1 + day, 10, 15);
        const lp = localParts(zone, ms)!;
        const back = localToUtc(zone, lp.year, lp.month, lp.day, lp.hour, lp.minute);
        expect(back, `${zone} ${new Date(ms).toISOString()}`).toBe(ms);
        checked += 1;
      }
    }
    expect(checked).toBeGreaterThan(500);
  });
});

describe('dates', () => {
  it('parses and refuses', () => {
    expect(parseYmd('2026-10-30')).toEqual({ year: 2026, month: 10, day: 30 });
    expect(parseYmd('2028-02-29')).not.toBeNull();
    expect(parseYmd('2026-02-29')).toBeNull();
    expect(parseYmd('2026-13-01')).toBeNull();
    expect(parseYmd('30/10/2026')).toBeNull();
    expect(parseYmd('')).toBeNull();
  });
  it("writes the tag a message renders in each reader's own zone", () => {
    expect(timestampTag(utc('2026-01-01T00:00:00Z'))).toBe('<t:1767225600:F>');
    expect(timestampTag(utc('2026-01-01T00:00:00.999Z'), 't')).toBe('<t:1767225600:t>');
    expect(timestampTag(1_000, 'R')).toBe('<t:1:R>');
  });
  it('lists zones for the picker', () => {
    const zones = allZones();
    expect(zones).toContain('UTC');
    expect(zones.length).toBeGreaterThan(15);
    expect(zones.every((z) => isValidZone(z))).toBe(true);
  });
});

describe('the overlap grid (fixed zones and a fixed date)', () => {
  // 2026-10-30 is a Friday. Berlin left summer time on 25 October (UTC+1); New York leaves it on 1 November
  // (UTC-4), so the two are five hours apart.
  const ny = p('ny', 'America/New_York');
  const berlin = p('berlin', 'Europe/Berlin');
  const grid = buildGrid([ny, berlin], '2026-10-30', DEFAULT_GRID_OPTIONS);

  it("has a column per hour of the first person's day", () => {
    expect(grid).toHaveLength(24);
    expect(grid.map((c) => c.cells[0]!.hour)).toEqual(Array.from({ length: 24 }, (_, h) => h));
    expect(grid[0]!.utcMs).toBe(utc('2026-10-30T04:00:00Z'));
  });

  it('shows the other zone five hours ahead', () => {
    expect(grid[9]!.cells[1]).toMatchObject({ hour: 14, minute: 0, dayShift: 0, working: true });
    expect(grid[20]!.cells[1]).toMatchObject({ hour: 1, dayShift: 1, working: false });
  });

  it('finds nine to noon in New York as the overlap', () => {
    expect(grid.filter((c) => c.allWorking).map((c) => c.cells[0]!.hour)).toEqual([9, 10, 11]);
    expect(overlapRuns(grid)).toEqual([[9, 11]]);
    expect(bestColumn(grid)).toBe(10);
  });

  it('moves with the working hours', () => {
    // New York 9 to 20 against Berlin 9 to 20: Berlin is five hours ahead, so New York 9 to 14 overlaps.
    const late = buildGrid([ny, berlin], '2026-10-30', {
      workStart: 9,
      workEnd: 20,
      skipWeekends: true,
    });
    expect(overlapRuns(late)).toEqual([[9, 14]]);
    expect(bestColumn(late)).toBe(11);
    // Seven to noon in both leaves no hour in common (they are five hours apart).
    const early = buildGrid([ny, berlin], '2026-10-30', {
      workStart: 7,
      workEnd: 12,
      skipWeekends: true,
    });
    expect(overlapRuns(early)).toEqual([]);
  });

  it('keeps weekends out unless asked', () => {
    // 2026-10-31 is a Saturday.
    const sat = buildGrid([ny, berlin], '2026-10-31', DEFAULT_GRID_OPTIONS);
    expect(overlapRuns(sat)).toEqual([]);
    expect(sat.every((c) => c.workingCount === 0)).toBe(true);
    const open = buildGrid([ny, berlin], '2026-10-31', {
      ...DEFAULT_GRID_OPTIONS,
      skipWeekends: false,
    });
    expect(overlapRuns(open)).toEqual([[9, 11]]);
  });

  it('suggests the hour with the most people when nobody overlaps', () => {
    const far = buildGrid(
      [ny, p('tokyo', 'Asia/Tokyo'), p('la', 'America/Los_Angeles')],
      '2026-10-30',
    );
    expect(overlapRuns(far)).toEqual([]);
    const best = bestColumn(far);
    expect(far[best]!.workingCount).toBe(Math.max(...far.map((c) => c.workingCount)));
    expect(far[best]!.workingCount).toBeGreaterThanOrEqual(2);
  });

  it('shows half-hour zones with their minutes', () => {
    const g = buildGrid(
      [p('utc', 'UTC'), p('delhi', 'Asia/Kolkata'), p('kathmandu', 'Asia/Kathmandu')],
      '2026-10-30',
    );
    expect(g[0]!.cells[1]).toMatchObject({ hour: 5, minute: 30 });
    expect(g[0]!.cells[2]).toMatchObject({ hour: 5, minute: 45 });
    expect(g[20]!.cells[1]).toMatchObject({ hour: 1, minute: 30, dayShift: 1 });
  });

  it('marks the day before and after', () => {
    const g = buildGrid([p('hon', 'Pacific/Honolulu'), p('akl', 'Pacific/Auckland')], '2026-10-30');
    // 00:00 in Honolulu is 23:00 the same day in Auckland (UTC+13), and the next hour is already tomorrow.
    expect(g[0]!.cells[1]!.dayShift).toBe(0);
    expect(g[1]!.cells[1]!.dayShift).toBe(1);
  });

  it('is empty for no people, a bad date or an unknown first zone', () => {
    expect(buildGrid([], '2026-10-30')).toEqual([]);
    expect(buildGrid([ny], 'nope')).toEqual([]);
    expect(buildGrid([p('x', 'Mars/Base')], '2026-10-30')).toEqual([]);
    expect(bestColumn([])).toBe(-1);
  });

  it('keeps 24 columns on the day the clocks change', () => {
    const g = buildGrid([ny, berlin], '2026-11-01');
    expect(g).toHaveLength(24);
    // The skipped or repeated hour never produces two columns with the same instant.
    expect(new Set(g.map((c) => c.utcMs)).size).toBe(24);
  });
});

describe('people', () => {
  it('adds with checks', () => {
    const one = addPerson([], '  Alex  ', 'Europe/Berlin', 'a')!;
    expect(one).toEqual([{ id: 'a', name: 'Alex', zone: 'Europe/Berlin' }]);
    expect(addPerson(one, '', 'Europe/Berlin', 'b')).toBeNull();
    expect(addPerson(one, 'Bo', 'Nowhere/Land', 'b')).toBeNull();
    const full = Array.from({ length: MAX_PEOPLE }, (_, i) => p(`p${i}`, 'UTC'));
    expect(addPerson(full, 'Extra', 'UTC', 'x')).toBeNull();
    expect(addPerson([], 'x'.repeat(100), 'UTC', 'y')![0]!.name).toHaveLength(40);
  });
  it('survives storage that is empty, broken or blocked', () => {
    localStorage.removeItem('sr.tools.timezone.people');
    expect(loadPeople()).toEqual([]);
    localStorage.setItem('sr.tools.timezone.people', '{not json');
    expect(loadPeople()).toEqual([]);
    localStorage.setItem(
      'sr.tools.timezone.people',
      JSON.stringify([p('ok', 'Asia/Tokyo'), p('bad', 'Mars/Base'), { nope: true }]),
    );
    expect(loadPeople()).toEqual([]);
    localStorage.setItem(
      'sr.tools.timezone.people',
      JSON.stringify([p('ok', 'Asia/Tokyo'), p('bad', 'Mars/Base')]),
    );
    expect(loadPeople().map((x) => x.id)).toEqual(['ok']);
    localStorage.removeItem('sr.tools.timezone.people');
  });
});

describe('the hand-off to the invite cards', () => {
  it('remembers the picked instant and tells subscribers', () => {
    const seen: (number | null)[] = [];
    const off = pickedTime.subscribe((v) => seen.push(v));
    pickedTime.set(123);
    expect(pickedTime.get()).toBe(123);
    pickedTime.set(null);
    off();
    pickedTime.set(5);
    expect(seen).toEqual([123, null]);
    pickedTime.set(null);
  });
});
