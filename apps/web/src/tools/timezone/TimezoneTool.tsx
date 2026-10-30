import { useEffect, useId, useMemo, useState } from 'react';
import { formatDateTime, useT } from '../../i18n';
import type { ToolProps } from '../registry';
import { useCopy } from '../calc/hooks';
import '../calc/calc.css';
import { addPerson, loadPeople, MAX_PEOPLE, savePeople } from './people';
import {
  allZones,
  bestColumn,
  browserZone,
  buildGrid,
  DEFAULT_GRID_OPTIONS,
  formatOffset,
  offsetMinutes,
  overlapRuns,
  pickedTime,
  timestampTag,
  ymd,
  type GridOptions,
  type PersonZone,
  type TimestampStyle,
} from './zones';

const TAG_STYLES: { style: TimestampStyle; key: 'full' | 'time' | 'relative' }[] = [
  { style: 'F', key: 'full' },
  { style: 't', key: 'time' },
  { style: 'R', key: 'relative' },
];

const TAG_LABELS = {
  full: 'tools.timezone.tagFull',
  time: 'tools.timezone.tagTime',
  relative: 'tools.timezone.tagRelative',
} as const;

const PROBLEMS = {
  name: 'tools.timezone.problemName',
  zone: 'tools.timezone.problemZone',
  full: 'tools.timezone.problemFull',
} as const;

const hours = Array.from({ length: 25 }, (_, h) => h);

/** The meeting time finder: who is awake when, the best hour for everyone, and a timestamp tag to paste. */
export default function TimezoneTool({ host }: ToolProps) {
  const t = useT();
  const listId = useId();
  const me = useMemo(browserZone, []);
  const [others, setOthers] = useState<PersonZone[]>(() => loadPeople());
  const [date, setDate] = useState(() => ymd(me, Date.now()));
  const [opts, setOpts] = useState<GridOptions>(DEFAULT_GRID_OPTIONS);
  const [name, setName] = useState('');
  const [zone, setZone] = useState('');
  const [problem, setProblem] = useState<'name' | 'zone' | 'full' | null>(null);
  const [picked, setPicked] = useState<number | null>(null);
  const { copied, copy } = useCopy();

  const people = useMemo<PersonZone[]>(
    () => [{ id: 'me', name: t('tools.timezone.you'), zone: me }, ...others],
    [t, me, others],
  );
  const grid = useMemo(() => buildGrid(people, date, opts), [people, date, opts]);
  const best = useMemo(() => bestColumn(grid), [grid]);
  const runs = useMemo(() => overlapRuns(grid), [grid]);
  const zones = useMemo(allZones, []);

  // Start on the best hour; keep the choice while the grid is only re-drawn for the same people.
  const selected = picked !== null && picked < grid.length ? picked : best;
  const column = selected >= 0 ? grid[selected] : undefined;

  useEffect(() => {
    pickedTime.set(column ? column.utcMs : null);
  }, [column]);

  const add = () => {
    if (name.trim() === '') return setProblem('name');
    const next = addPerson(others, name, zone, `p${Date.now().toString(36)}${others.length}`);
    if (!next) return setProblem(others.length >= MAX_PEOPLE ? 'full' : 'zone');
    setOthers(next);
    savePeople(next);
    setName('');
    setZone('');
    setProblem(null);
  };

  const remove = (id: string) => {
    const next = others.filter((p) => p.id !== id);
    setOthers(next);
    savePeople(next);
  };

  const hourLabel = (h: number, m: number) =>
    m === 0 ? String(h) : `${h}:${String(m).padStart(2, '0')}`;

  return (
    <div className="sr-calc sr-tz" data-host={host}>
      <div className="sr-calc-opts">
        <label className="sr-calc-field">
          {t('tools.timezone.date')}
          <input
            type="date"
            value={date}
            onChange={(e) => e.target.value && setDate(e.target.value)}
          />
        </label>
        <label className="sr-calc-field">
          {t('tools.timezone.workStart')}
          <select
            value={opts.workStart}
            onChange={(e) =>
              setOpts({ ...opts, workStart: Math.min(Number(e.target.value), opts.workEnd - 1) })
            }
          >
            {hours.slice(0, 24).map((h) => (
              <option key={h} value={h}>
                {h}:00
              </option>
            ))}
          </select>
        </label>
        <label className="sr-calc-field">
          {t('tools.timezone.workEnd')}
          <select
            value={opts.workEnd}
            onChange={(e) =>
              setOpts({ ...opts, workEnd: Math.max(Number(e.target.value), opts.workStart + 1) })
            }
          >
            {hours.slice(1).map((h) => (
              <option key={h} value={h}>
                {h}:00
              </option>
            ))}
          </select>
        </label>
        <label className="sr-calc-check">
          <input
            type="checkbox"
            checked={opts.skipWeekends}
            onChange={(e) => setOpts({ ...opts, skipWeekends: e.target.checked })}
          />
          {t('tools.timezone.skipWeekends')}
        </label>
      </div>

      <div className="sr-tz-scroll">
        <table className="sr-tz-grid">
          <caption className="sr-calc-sr-only">
            {t('tools.timezone.gridCaption', { zone: me })}
          </caption>
          <thead>
            <tr>
              <th scope="col">{t('tools.timezone.person')}</th>
              {grid.map((c, i) => (
                <th key={c.utcMs} scope="col" data-all={c.allWorking || undefined}>
                  <button
                    type="button"
                    className="sr-tz-pick"
                    aria-pressed={i === selected}
                    aria-label={t('tools.timezone.pickHour', { hour: c.cells[0]?.hour ?? i })}
                    onClick={() => setPicked(i)}
                  >
                    {c.cells[0]?.hour ?? i}
                  </button>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {people.map((p, row) => (
              <tr key={p.id}>
                <th scope="row">
                  <span className="sr-tz-name">{p.name}</span>
                  <span className="sr-tl-muted sr-tz-zone" dir="ltr">
                    {p.zone} {formatOffset(offsetMinutes(p.zone, column?.utcMs ?? Date.now()))}
                  </span>
                  {p.id !== 'me' && (
                    <button
                      type="button"
                      className="sr-tl-btn"
                      aria-label={t('tools.timezone.remove', { name: p.name })}
                      onClick={() => remove(p.id)}
                    >
                      {'×'}
                    </button>
                  )}
                </th>
                {grid.map((c, i) => {
                  const cell = c.cells[row];
                  if (!cell) return <td key={c.utcMs} />;
                  return (
                    <td
                      key={c.utcMs}
                      className="sr-tz-cell"
                      data-working={cell.working || undefined}
                      data-all={c.allWorking || undefined}
                      data-selected={i === selected || undefined}
                      data-shift={cell.dayShift || undefined}
                      dir="ltr"
                    >
                      {hourLabel(cell.hour, cell.minute)}
                      {cell.dayShift !== 0 && (
                        <sup
                          aria-label={
                            cell.dayShift > 0
                              ? t('tools.timezone.nextDay')
                              : t('tools.timezone.prevDay')
                          }
                        >
                          {cell.dayShift > 0 ? '+1' : '−1'}
                        </sup>
                      )}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <p className="sr-tl-muted" role="status" aria-live="polite">
        {runs.length > 0
          ? t('tools.timezone.bestAll', {
              time: column
                ? formatDateTime(column.utcMs, { timeZone: me, hour: 'numeric', minute: '2-digit' })
                : '',
            })
          : t('tools.timezone.bestSome', {
              count: column?.workingCount ?? 0,
              total: people.length,
            })}
      </p>

      {column && (
        <section aria-label={t('tools.timezone.selected')} className="sr-tz-selected">
          <h3 className="sr-calc-h">{t('tools.timezone.selected')}</h3>
          <ul className="sr-calc-history">
            {people.map((p) => (
              <li key={p.id}>
                <span>{p.name}</span>
                <span dir="auto">
                  {formatDateTime(column.utcMs, {
                    timeZone: p.zone,
                    weekday: 'short',
                    month: 'short',
                    day: 'numeric',
                    hour: 'numeric',
                    minute: '2-digit',
                  })}
                </span>
              </li>
            ))}
          </ul>
          <div className="sr-calc-opts">
            {TAG_STYLES.map(({ style, key }) => {
              const tag = timestampTag(column.utcMs, style);
              return (
                <button
                  key={style}
                  type="button"
                  className="sr-tl-btn"
                  onClick={() => copy(style, tag)}
                >
                  {copied === style ? t('tools.calc.copied') : t(TAG_LABELS[key])}
                </button>
              );
            })}
          </div>
          <p className="sr-tl-muted" dir="ltr">
            {timestampTag(column.utcMs, 'F')}
          </p>
        </section>
      )}

      <form
        className="sr-tz-add"
        onSubmit={(e) => {
          e.preventDefault();
          add();
        }}
      >
        <h3 className="sr-calc-h">{t('tools.timezone.addPerson')}</h3>
        <div className="sr-calc-opts">
          <label className="sr-calc-field">
            {t('tools.timezone.personName')}
            <input
              type="text"
              maxLength={40}
              autoComplete="off"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <label className="sr-calc-field">
            {t('tools.timezone.zone')}
            <input
              type="text"
              list={listId}
              autoComplete="off"
              spellCheck={false}
              dir="ltr"
              placeholder="Europe/Berlin"
              value={zone}
              onChange={(e) => setZone(e.target.value)}
            />
          </label>
          <datalist id={listId}>
            {zones.map((z) => (
              <option key={z} value={z} />
            ))}
          </datalist>
          <button type="submit" className="sr-tl-btn">
            {t('tools.timezone.add')}
          </button>
        </div>
        {problem && (
          <p role="alert" className="sr-calc-error">
            {t(PROBLEMS[problem], { max: MAX_PEOPLE })}
          </p>
        )}
        <p className="sr-tl-muted">{t('tools.timezone.privacy')}</p>
      </form>
    </div>
  );
}
