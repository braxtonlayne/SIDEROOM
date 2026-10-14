// Schedule rules for a rule-based channel (TV-ADM-002): content pool filters, block length, ordering, dayparts and
// rerun spacing (spec cinema-live-tv section 4). Controlled: the channel editor owns the value and the save.
import { Button, Select, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { toList } from '../hooks';
import { ORDERINGS, type Daypart, type Ordering, type ScheduleRules } from './tvApi';

const KINDS: { value: ScheduleRules['pool']['kinds'][number]; label: string }[] = [
  { value: 'movie', label: 'Movies' },
  { value: 'series', label: 'Series' },
  { value: 'anime', label: 'Anime' },
];
const HHMM = /^([01]\d|2[0-3]):[0-5]\d$/;

/** First problem with the rules, or null. Mirrors what the server will refuse. */
export function validateRules(r: ScheduleRules, channelRestricted: boolean): string | null {
  if (!Number.isInteger(r.block_minutes) || r.block_minutes < 5 || r.block_minutes > 480)
    return 'Block length is between 5 and 480 minutes.';
  if (!Number.isFinite(r.rerun_spacing_hours) || r.rerun_spacing_hours < 0)
    return 'Rerun spacing cannot be negative.';
  if (r.pool.decades.some((d) => !Number.isInteger(d) || d < 1890 || d % 10 !== 0))
    return 'Decades are years like 1990 or 2000.';
  for (const [i, d] of r.dayparts.entries()) {
    if (!HHMM.test(d.start) || !HHMM.test(d.end))
      return `Daypart ${i + 1} needs a start and an end time.`;
    if (d.start === d.end) return `Daypart ${i + 1} starts and ends at the same time.`;
    if (d.restricted_only && !channelRestricted)
      return `Daypart ${i + 1} is age-restricted only: mark the channel age-restricted first.`;
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

export interface RulesEditorProps {
  value: ScheduleRules;
  onChange: (next: ScheduleRules) => void;
  channelRestricted: boolean;
}

export function RulesEditor({ value, onChange, channelRestricted }: RulesEditorProps) {
  const set = (patch: Partial<ScheduleRules>) => onChange({ ...value, ...patch });
  const pool = (patch: Partial<ScheduleRules['pool']>) =>
    set({ pool: { ...value.pool, ...patch } });
  const part = (i: number, patch: Partial<Daypart>) =>
    set({ dayparts: value.dayparts.map((d, j) => (j === i ? { ...d, ...patch } : d)) });
  const problem = validateRules(value, channelRestricted);
  return (
    <div className="adm-form adm-cin-form">
      <fieldset className="adm-check-row">
        <legend className="ss-field-label">Content pool: kinds</legend>
        {KINDS.map((k) => (
          <Switch
            key={k.value}
            label={k.label}
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
          label="Genres"
          hint="Separate with commas. Empty means any genre."
          list={value.pool.genres}
          onList={(genres) => pool({ genres })}
        />
        <ListField
          label="Decades"
          hint="Years like 1990, 2000. Empty means any."
          list={value.pool.decades}
          onList={(items) => pool({ decades: items.map(Number).filter((n) => Number.isFinite(n)) })}
        />
        <ListField label="Tags" list={value.pool.tags} onList={(tags) => pool({ tags })} />
        <TextInput
          label="Collection id"
          hint="Limit the pool to one collection."
          value={value.pool.collection ?? ''}
          onChange={(e) => pool({ collection: e.target.value.trim() || null })}
        />
        <TextInput
          label="Block length (minutes)"
          type="number"
          min={5}
          max={480}
          value={String(value.block_minutes)}
          onChange={(e) => set({ block_minutes: Math.trunc(Number(e.target.value)) })}
        />
        <Select
          aria-label="Ordering"
          options={ORDERINGS}
          value={value.ordering}
          onValueChange={(v) => set({ ordering: v as Ordering })}
        />
        <TextInput
          label="Rerun spacing (hours)"
          type="number"
          min={0}
          value={String(value.rerun_spacing_hours)}
          onChange={(e) => set({ rerun_spacing_hours: Number(e.target.value) })}
        />
      </div>

      <fieldset className="adm-cin-dayparts">
        <legend className="ss-field-label">Dayparts</legend>
        {value.dayparts.length === 0 && (
          <p className="ss-meta">No dayparts: the same pool airs around the clock.</p>
        )}
        {value.dayparts.map((d, i) => (
          <div key={i} className="adm-cin-grid" role="group" aria-label={`Daypart ${i + 1}`}>
            <TextInput
              label={`Daypart ${i + 1} start`}
              type="time"
              value={d.start}
              onChange={(e) => part(i, { start: e.target.value })}
            />
            <TextInput
              label={`Daypart ${i + 1} end`}
              type="time"
              value={d.end}
              onChange={(e) => part(i, { end: e.target.value })}
            />
            <Switch
              label={`Daypart ${i + 1} age-restricted only`}
              checked={d.restricted_only}
              onChange={(e) => part(i, { restricted_only: e.target.checked })}
            />
            <Button
              variant="secondary"
              size="sm"
              onClick={() => set({ dayparts: value.dayparts.filter((_, j) => j !== i) })}
            >
              Remove daypart {i + 1}
            </Button>
          </div>
        ))}
        <Button
          variant="secondary"
          size="sm"
          onClick={() =>
            set({
              dayparts: [
                ...value.dayparts,
                { start: '22:00', end: '02:00', restricted_only: false },
              ],
            })
          }
        >
          Add daypart
        </Button>
      </fieldset>
      {problem && (
        <p role="alert" className="ss-status ss-error">
          {problem}
        </p>
      )}
    </div>
  );
}
