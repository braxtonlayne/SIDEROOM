// Schedule rules for a rule-based channel (TV-ADM-002; spec cinema-live-tv section 8.2): the grid, the no-repeat
// window and dayparts, each with its own mode, fit and content pool. Controlled: the channel editor owns the
// value and the save.
import { Button, Select, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { toList } from '../hooks';
import {
  newDaypart,
  type Daypart,
  type DaypartPool,
  type PoolKind,
  type RestrictedMode,
  type ScheduleRules,
} from './tvApi';

const KINDS: { value: PoolKind; label: string }[] = [
  { value: 'movie', label: 'Movies' },
  { value: 'series', label: 'Series' },
  { value: 'anime', label: 'Anime' },
];
const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
const HHMM = /^([01]\d|2[0-3]):[0-5]\d$/;

const num = (s: string): number | null => {
  const t = s.trim();
  if (!t) return null;
  const n = Number(t);
  return Number.isFinite(n) ? Math.trunc(n) : null;
};

/** First problem with the rules, or null. Mirrors what the server will refuse. */
export function validateRules(r: ScheduleRules): string | null {
  if (!Number.isInteger(r.grid_minutes) || r.grid_minutes < 0 || r.grid_minutes > 1440)
    return 'The grid is between 0 and 1440 minutes (0 means back to back).';
  if (!Number.isInteger(r.no_repeat_window) || r.no_repeat_window < 0 || r.no_repeat_window > 64)
    return 'The no-repeat window is between 0 and 64.';
  if (r.dayparts.length === 0)
    return 'Add at least one daypart: without one the channel has nothing to air.';
  const names = new Set<string>();
  for (const [i, d] of r.dayparts.entries()) {
    const n = i + 1;
    if (!d.name.trim()) return `Daypart ${n} needs a name.`;
    if (names.has(d.name.trim())) return `Daypart names must be unique (${d.name.trim()}).`;
    names.add(d.name.trim());
    if (!HHMM.test(d.start) || !HHMM.test(d.end))
      return `Daypart ${n} needs a start and an end time.`;
    const { year_from: yf, year_to: yt, min_runtime_min: mn, max_runtime_min: mx } = d.pool;
    if (yf !== null && yt !== null && yf > yt)
      return `Daypart ${n}: the first year is after the last.`;
    if (mn !== null && mx !== null && mn > mx)
      return `Daypart ${n}: the minimum runtime is above the maximum.`;
    if (mn !== null && mn < 0) return `Daypart ${n}: runtime cannot be negative.`;
  }
  return null;
}

/** A comma-separated field that keeps what the person typed (a trailing comma must survive the keystroke). */
function ListField({
  label,
  hint,
  list,
  onList,
}: {
  label: string;
  hint?: string;
  list: readonly (string | number)[];
  onList: (items: string[]) => void;
}) {
  const [raw, setRaw] = useState(() => list.join(', '));
  return (
    <TextInput
      label={label}
      hint={hint}
      value={raw}
      onChange={(e) => {
        setRaw(e.target.value);
        onList(toList(e.target.value));
      }}
    />
  );
}

function NumberField({
  label,
  value,
  onValue,
  hint,
}: {
  label: string;
  value: number | null;
  onValue: (n: number | null) => void;
  hint?: string;
}) {
  return (
    <TextInput
      label={label}
      hint={hint}
      type="number"
      value={value === null ? '' : String(value)}
      onChange={(e) => onValue(num(e.target.value))}
    />
  );
}

function DaypartEditor({
  index,
  value,
  onChange,
  onRemove,
}: {
  index: number;
  value: Daypart;
  onChange: (next: Daypart) => void;
  onRemove: () => void;
}) {
  const n = index + 1;
  const pool = (patch: Partial<DaypartPool>) =>
    onChange({ ...value, pool: { ...value.pool, ...patch } });
  return (
    <fieldset className="adm-cin-daypart" aria-label={`Daypart ${n}`}>
      <legend className="ss-field-label">Daypart {n}</legend>
      <div className="adm-cin-grid">
        <TextInput
          label={`Daypart ${n} name`}
          value={value.name}
          onChange={(e) => onChange({ ...value, name: e.target.value })}
        />
        <TextInput
          label={`Daypart ${n} start`}
          type="time"
          value={value.start}
          onChange={(e) => onChange({ ...value, start: e.target.value })}
        />
        <TextInput
          label={`Daypart ${n} end`}
          type="time"
          hint="An end at or before the start wraps past midnight."
          value={value.end}
          onChange={(e) => onChange({ ...value, end: e.target.value })}
        />
        <Select
          aria-label={`Daypart ${n} mode`}
          options={[
            { value: 'shuffle', label: 'Shuffle' },
            { value: 'sequential', label: 'Sequential (keeps counting across days)' },
          ]}
          value={value.mode}
          onValueChange={(v) => onChange({ ...value, mode: v as Daypart['mode'] })}
        />
        <Select
          aria-label={`Daypart ${n} fit`}
          options={[
            { value: 'overrun', label: 'Overrun (the last programme may pass the end)' },
            { value: 'fit', label: 'Fit (only items that fit)' },
          ]}
          value={value.fit}
          onValueChange={(v) => onChange({ ...value, fit: v as Daypart['fit'] })}
        />
      </div>
      <fieldset className="adm-check-row">
        <legend className="ss-field-label">Days (none means every day)</legend>
        {DAYS.map((d, i) => (
          <Switch
            key={d}
            label={`Daypart ${n} ${d}`}
            checked={value.days.includes(i)}
            onChange={(e) =>
              onChange({
                ...value,
                days: e.target.checked
                  ? [...value.days, i].sort((a, b) => a - b)
                  : value.days.filter((x) => x !== i),
              })
            }
          />
        ))}
      </fieldset>
      <fieldset className="adm-check-row">
        <legend className="ss-field-label">Content pool: kinds</legend>
        {KINDS.map((k) => (
          <Switch
            key={k.value}
            label={`Daypart ${n} ${k.label}`}
            checked={value.pool.kinds.includes(k.value)}
            onChange={(e) =>
              pool({
                kinds: e.target.checked
                  ? [...value.pool.kinds, k.value]
                  : value.pool.kinds.filter((x) => x !== k.value),
              })
            }
          />
        ))}
      </fieldset>
      <div className="adm-cin-grid">
        <ListField
          label={`Daypart ${n} genres`}
          hint="Separate with commas. Empty means any."
          list={value.pool.genres}
          onList={(genres) => pool({ genres })}
        />
        <ListField
          label={`Daypart ${n} tags`}
          list={value.pool.tags}
          onList={(tags) => pool({ tags })}
        />
        <ListField
          label={`Daypart ${n} title ids`}
          hint="Pin the pool to specific titles (a series marathon is one id)."
          list={value.pool.title_ids}
          onList={(title_ids) => pool({ title_ids })}
        />
        <TextInput
          label={`Daypart ${n} collection id`}
          value={value.pool.collection_id ?? ''}
          onChange={(e) => pool({ collection_id: e.target.value.trim() || null })}
        />
        <NumberField
          label={`Daypart ${n} year from`}
          value={value.pool.year_from}
          onValue={(year_from) => pool({ year_from })}
        />
        <NumberField
          label={`Daypart ${n} year to`}
          value={value.pool.year_to}
          onValue={(year_to) => pool({ year_to })}
        />
        <NumberField
          label={`Daypart ${n} minimum runtime (minutes)`}
          value={value.pool.min_runtime_min}
          onValue={(min_runtime_min) => pool({ min_runtime_min })}
        />
        <NumberField
          label={`Daypart ${n} maximum runtime (minutes)`}
          value={value.pool.max_runtime_min}
          onValue={(max_runtime_min) => pool({ max_runtime_min })}
        />
        <Select
          aria-label={`Daypart ${n} age-restricted titles`}
          options={[
            { value: 'exclude', label: 'Exclude age-restricted titles' },
            { value: 'include', label: 'Include age-restricted titles' },
            { value: 'only', label: 'Only age-restricted titles' },
          ]}
          value={value.pool.restricted}
          onValueChange={(v) => pool({ restricted: v as RestrictedMode })}
        />
      </div>
      <Button variant="secondary" size="sm" onClick={onRemove}>
        Remove daypart {n}
      </Button>
    </fieldset>
  );
}

export interface RulesEditorProps {
  value: ScheduleRules;
  onChange: (next: ScheduleRules) => void;
}

export function RulesEditor({ value, onChange }: RulesEditorProps) {
  const set = (patch: Partial<ScheduleRules>) => onChange({ ...value, ...patch });
  const problem = validateRules(value);
  return (
    <div className="adm-form adm-cin-form">
      <div className="adm-cin-grid">
        <NumberField
          label="Grid (minutes)"
          hint="Programmes start on this grid in channel time. 0 means back to back."
          value={value.grid_minutes}
          onValue={(n) => set({ grid_minutes: n ?? 0 })}
        />
        <NumberField
          label="No-repeat window"
          hint="Shuffle: the last N picks are not repeated (0 to 64)."
          value={value.no_repeat_window}
          onValue={(n) => set({ no_repeat_window: n ?? 0 })}
        />
      </div>
      {value.dayparts.map((d, i) => (
        <DaypartEditor
          key={i}
          index={i}
          value={d}
          onChange={(next) => set({ dayparts: value.dayparts.map((x, j) => (j === i ? next : x)) })}
          onRemove={() => set({ dayparts: value.dayparts.filter((_, j) => j !== i) })}
        />
      ))}
      <div>
        <Button
          variant="secondary"
          size="sm"
          onClick={() =>
            set({
              dayparts: [...value.dayparts, newDaypart(`Daypart ${value.dayparts.length + 1}`)],
            })
          }
        >
          Add daypart
        </Button>
      </div>
      {problem && (
        <p role="alert" className="ss-status ss-error">
          {problem}
        </p>
      )}
    </div>
  );
}
