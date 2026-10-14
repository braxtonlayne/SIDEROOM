// Schedule rules for a rule-based channel (TV-ADM-002; spec cinema-live-tv section 8.2): the grid, the no-repeat
// window and dayparts, each with its own mode, fit and content pool. Controlled: the channel editor owns the
// value and the save.
import { Button, Select, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { t, useT } from '../../../i18n';
import { toList } from '../hooks';
import {
  newDaypart,
  type Daypart,
  type DaypartPool,
  type PoolKind,
  type RestrictedMode,
  type ScheduleRules,
} from './tvApi';

const HHMM = /^([01]\d|2[0-3]):[0-5]\d$/;

const num = (s: string): number | null => {
  const trimmed = s.trim();
  if (!trimmed) return null;
  const n = Number(trimmed);
  return Number.isFinite(n) ? Math.trunc(n) : null;
};

/** First problem with the rules, or null. Mirrors what the server will refuse. */
export function validateRules(r: ScheduleRules): string | null {
  if (!Number.isInteger(r.grid_minutes) || r.grid_minutes < 0 || r.grid_minutes > 1440)
    return t('admin.rulesEditor.err.grid');
  if (!Number.isInteger(r.no_repeat_window) || r.no_repeat_window < 0 || r.no_repeat_window > 64)
    return t('admin.rulesEditor.err.noRepeat');
  if (r.dayparts.length === 0) return t('admin.rulesEditor.err.noDayparts');
  const names = new Set<string>();
  for (const [i, d] of r.dayparts.entries()) {
    const n = i + 1;
    if (!d.name.trim()) return t('admin.rulesEditor.err.needsName', { n });
    if (names.has(d.name.trim())) return t('admin.rulesEditor.err.unique', { name: d.name.trim() });
    names.add(d.name.trim());
    if (!HHMM.test(d.start) || !HHMM.test(d.end))
      return t('admin.rulesEditor.err.needsTimes', { n });
    const { year_from: yf, year_to: yt, min_runtime_min: mn, max_runtime_min: mx } = d.pool;
    if (yf !== null && yt !== null && yf > yt) return t('admin.rulesEditor.err.years', { n });
    if (mn !== null && mx !== null && mn > mx) return t('admin.rulesEditor.err.runtime', { n });
    if (mn !== null && mn < 0) return t('admin.rulesEditor.err.negative', { n });
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
  const t = useT();
  const kinds: { value: PoolKind; label: string }[] = [
    { value: 'movie', label: t('admin.rulesEditor.kind.movies') },
    { value: 'series', label: t('admin.rulesEditor.kind.series') },
    { value: 'anime', label: t('admin.rulesEditor.kind.anime') },
  ];
  const days = [
    t('admin.rulesEditor.day.mon'),
    t('admin.rulesEditor.day.tue'),
    t('admin.rulesEditor.day.wed'),
    t('admin.rulesEditor.day.thu'),
    t('admin.rulesEditor.day.fri'),
    t('admin.rulesEditor.day.sat'),
    t('admin.rulesEditor.day.sun'),
  ];
  const n = index + 1;
  const pool = (patch: Partial<DaypartPool>) =>
    onChange({ ...value, pool: { ...value.pool, ...patch } });
  return (
    <fieldset className="adm-cin-daypart" aria-label={t('admin.rulesEditor.daypart', { n })}>
      <legend className="ss-field-label">{t('admin.rulesEditor.daypart', { n })}</legend>
      <div className="adm-cin-grid">
        <TextInput
          label={t('admin.rulesEditor.name', { n })}
          value={value.name}
          onChange={(e) => onChange({ ...value, name: e.target.value })}
        />
        <TextInput
          label={t('admin.rulesEditor.start', { n })}
          type="time"
          value={value.start}
          onChange={(e) => onChange({ ...value, start: e.target.value })}
        />
        <TextInput
          label={t('admin.rulesEditor.end', { n })}
          type="time"
          hint={t('admin.rulesEditor.endHint')}
          value={value.end}
          onChange={(e) => onChange({ ...value, end: e.target.value })}
        />
        <Select
          aria-label={t('admin.rulesEditor.mode', { n })}
          options={[
            { value: 'shuffle', label: t('admin.rulesEditor.mode.shuffle') },
            { value: 'sequential', label: t('admin.rulesEditor.mode.sequential') },
          ]}
          value={value.mode}
          onValueChange={(v) => onChange({ ...value, mode: v as Daypart['mode'] })}
        />
        <Select
          aria-label={t('admin.rulesEditor.fit', { n })}
          options={[
            { value: 'overrun', label: t('admin.rulesEditor.fit.overrun') },
            { value: 'fit', label: t('admin.rulesEditor.fit.fit') },
          ]}
          value={value.fit}
          onValueChange={(v) => onChange({ ...value, fit: v as Daypart['fit'] })}
        />
      </div>
      <fieldset className="adm-check-row">
        <legend className="ss-field-label">{t('admin.rulesEditor.days')}</legend>
        {days.map((d, i) => (
          <Switch
            key={d}
            label={t('admin.rulesEditor.dayLabel', { n, day: d })}
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
        <legend className="ss-field-label">{t('admin.rulesEditor.poolKinds')}</legend>
        {kinds.map((k) => (
          <Switch
            key={k.value}
            label={t('admin.rulesEditor.kindLabel', { n, label: k.label })}
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
          label={t('admin.rulesEditor.genres', { n })}
          hint={t('admin.rulesEditor.genresHint')}
          list={value.pool.genres}
          onList={(genres) => pool({ genres })}
        />
        <ListField
          label={t('admin.rulesEditor.tags', { n })}
          list={value.pool.tags}
          onList={(tags) => pool({ tags })}
        />
        <ListField
          label={t('admin.rulesEditor.titleIds', { n })}
          hint={t('admin.rulesEditor.titleIdsHint')}
          list={value.pool.title_ids}
          onList={(title_ids) => pool({ title_ids })}
        />
        <TextInput
          label={t('admin.rulesEditor.collectionId', { n })}
          value={value.pool.collection_id ?? ''}
          onChange={(e) => pool({ collection_id: e.target.value.trim() || null })}
        />
        <NumberField
          label={t('admin.rulesEditor.yearFrom', { n })}
          value={value.pool.year_from}
          onValue={(year_from) => pool({ year_from })}
        />
        <NumberField
          label={t('admin.rulesEditor.yearTo', { n })}
          value={value.pool.year_to}
          onValue={(year_to) => pool({ year_to })}
        />
        <NumberField
          label={t('admin.rulesEditor.minRuntime', { n })}
          value={value.pool.min_runtime_min}
          onValue={(min_runtime_min) => pool({ min_runtime_min })}
        />
        <NumberField
          label={t('admin.rulesEditor.maxRuntime', { n })}
          value={value.pool.max_runtime_min}
          onValue={(max_runtime_min) => pool({ max_runtime_min })}
        />
        <Select
          aria-label={t('admin.rulesEditor.restricted', { n })}
          options={[
            { value: 'exclude', label: t('admin.rulesEditor.restricted.exclude') },
            { value: 'include', label: t('admin.rulesEditor.restricted.include') },
            { value: 'only', label: t('admin.rulesEditor.restricted.only') },
          ]}
          value={value.pool.restricted}
          onValueChange={(v) => pool({ restricted: v as RestrictedMode })}
        />
      </div>
      <Button variant="secondary" size="sm" onClick={onRemove}>
        {t('admin.rulesEditor.remove', { n })}
      </Button>
    </fieldset>
  );
}

export interface RulesEditorProps {
  value: ScheduleRules;
  onChange: (next: ScheduleRules) => void;
}

export function RulesEditor({ value, onChange }: RulesEditorProps) {
  const t = useT();
  const set = (patch: Partial<ScheduleRules>) => onChange({ ...value, ...patch });
  const problem = validateRules(value);
  return (
    <div className="adm-form adm-cin-form">
      <div className="adm-cin-grid">
        <NumberField
          label={t('admin.rulesEditor.grid')}
          hint={t('admin.rulesEditor.gridHint')}
          value={value.grid_minutes}
          onValue={(n) => set({ grid_minutes: n ?? 0 })}
        />
        <NumberField
          label={t('admin.rulesEditor.noRepeat')}
          hint={t('admin.rulesEditor.noRepeatHint')}
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
              dayparts: [
                ...value.dayparts,
                newDaypart(t('admin.rulesEditor.daypart', { n: value.dayparts.length + 1 })),
              ],
            })
          }
        >
          {t('admin.rulesEditor.add')}
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
