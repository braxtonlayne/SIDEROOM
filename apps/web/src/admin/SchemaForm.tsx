// Renders a feature's settings schema (crates/sr-features/src/schema.rs) as a form (ADM-PLT-001). Values are
// plain JSON: bool, integer, string, whole seconds, `#rrggbb`, snowflake strings, arrays and groups.
// Lists of groups are editable rows (FTR-* modules: reaction-role panels, sticky messages, slowmode schedules)
// and lists of enum options are a set of checkboxes. When the caller passes the server's channels and roles,
// pickers are real choosers instead of id text boxes.
// The server validates every value; this form only keeps the shape right and shows the schema's own limits.
import { Button, Checkbox, MultiSelect, Select, TextInput } from '@sideroom/ui';
import type { ReactNode } from 'react';
import { useT } from '../i18n';
import './admin.css';

export interface SchemaField {
  key: string;
  label: string;
  help?: string;
  type: string;
  default?: unknown;
  min?: number;
  max?: number;
  min_secs?: number;
  max_secs?: number;
  max_len?: number;
  options?: { value: string; label: string }[];
  multi?: boolean;
  max_items?: number;
  /** What a list holds: text or ints (comma separated), enum options (checkboxes) or groups (rows). */
  item?: {
    type: string;
    fields?: SchemaField[];
    options?: { value: string; label: string }[];
  };
  fields?: SchemaField[];
}
export type SettingsValues = Record<string, unknown>;

/** The server's channels and roles. With them, channel and role pickers are choosers, not id text boxes. */
export interface PickerChoices {
  channels: { id: string; name: string }[];
  roles: { id: string; name: string }[];
}

/** Fields of a schema value as the API sends it; anything malformed is an empty form. */
export function schemaFields(schema: unknown): SchemaField[] {
  const f = (schema as { fields?: unknown } | null)?.fields;
  return Array.isArray(f) ? (f as SchemaField[]) : [];
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/** Drop unset (`null`, empty string) keys of a row, recursively, so the server sees only real values. */
function pruneRow(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(pruneRow);
  if (!isObj(v)) return v;
  const out: Record<string, unknown> = {};
  for (const [k, x] of Object.entries(v)) {
    if (x === null || x === undefined || x === '') continue;
    out[k] = pruneRow(x);
  }
  return out;
}

/** A new row of a list of groups, seeded with the group's defaults. */
export function defaultsOf(fields: SchemaField[]): SettingsValues {
  const out: SettingsValues = {};
  for (const f of fields) {
    if (f.type === 'group') out[f.key] = defaultsOf(f.fields ?? []);
    else if (f.default !== undefined) out[f.key] = f.default;
  }
  return out;
}

/**
 * The overrides to save: every value that differs from the schema default. Groups recurse and only
 * appear when something inside differs. A field the schema leaves unset with an empty value is not an override.
 * Rows of a list of groups are saved whole, minus the keys the user left empty.
 */
export function overridesOf(fields: SchemaField[], values: SettingsValues): SettingsValues {
  const out: SettingsValues = {};
  for (const f of fields) {
    const v = values[f.key];
    if (f.type === 'group') {
      const inner = overridesOf(f.fields ?? [], isObj(v) ? v : {});
      if (Object.keys(inner).length > 0) out[f.key] = inner;
      continue;
    }
    if (v === undefined || v === null || v === '') continue;
    if (f.type === 'list' && f.item?.type === 'group') {
      const rows = pruneRow(v);
      if (JSON.stringify(rows) !== JSON.stringify(f.default ?? [])) out[f.key] = rows;
      continue;
    }
    if (JSON.stringify(v) === JSON.stringify(f.default)) continue;
    if (Array.isArray(v) && v.length === 0 && f.default === undefined) continue;
    out[f.key] = v;
  }
  return out;
}

const textList = (v: unknown) => (Array.isArray(v) ? v.join(', ') : '');

/**
 * Lets a screen draw a plain text field its own way (for example a message picker for an id field). Return
 * `undefined` to keep the default text box.
 */
export type CustomText = (
  field: SchemaField,
  value: string,
  set: (next: string) => void,
) => ReactNode | undefined;

export function SchemaForm({
  fields,
  values,
  onChange,
  pickers,
  customText,
}: {
  fields: SchemaField[];
  values: SettingsValues;
  onChange: (next: SettingsValues) => void;
  pickers?: PickerChoices;
  customText?: CustomText;
}) {
  const t = useT();
  const set = (key: string, v: unknown) => onChange({ ...values, [key]: v });

  /** Plain text box for ids (and for pickers when the server's choices are not known). */
  const idInput = (f: SchemaField, v: unknown) => (
    <TextInput
      key={f.key}
      label={f.label}
      hint={t('admin.schemaForm.idHint')}
      value={f.multi ? textList(v) : typeof v === 'string' ? v : ''}
      onChange={(e) =>
        set(
          f.key,
          f.multi
            ? e.target.value
                .split(',')
                .map((x) => x.trim())
                .filter(Boolean)
            : e.target.value.trim(),
        )
      }
    />
  );

  return (
    <div className="adm-form">
      {fields.map((f) => {
        const v = values[f.key];
        const hint = f.help;
        switch (f.type) {
          case 'bool':
            return (
              <Checkbox
                key={f.key}
                label={f.label}
                {...(hint ? { description: hint } : {})}
                checked={v === true}
                onChange={(e) => set(f.key, e.target.checked)}
              />
            );
          case 'enum':
            return (
              <Select
                key={f.key}
                aria-label={f.label}
                options={(f.options ?? []).map((o) => ({ value: o.value, label: o.label }))}
                value={typeof v === 'string' ? v : ''}
                onValueChange={(next) => set(f.key, next)}
              />
            );
          case 'int':
          case 'duration': {
            const lo = f.type === 'int' ? f.min : f.min_secs;
            const hi = f.type === 'int' ? f.max : f.max_secs;
            return (
              <TextInput
                key={f.key}
                label={
                  f.type === 'duration'
                    ? t('admin.schemaForm.seconds', { label: f.label })
                    : f.label
                }
                type="number"
                step={1}
                {...(lo !== undefined ? { min: lo } : {})}
                {...(hi !== undefined ? { max: hi } : {})}
                {...(hint ? { hint } : {})}
                value={typeof v === 'number' ? String(v) : ''}
                onChange={(e) => set(f.key, e.target.value === '' ? null : Number(e.target.value))}
              />
            );
          }
          case 'color':
            return (
              <TextInput
                key={f.key}
                label={f.label}
                placeholder="#rrggbb"
                {...(hint ? { hint } : {})}
                value={typeof v === 'string' ? v : ''}
                onChange={(e) => set(f.key, e.target.value)}
              />
            );
          case 'list': {
            if (f.item?.type === 'group') {
              const rows = Array.isArray(v) ? v : [];
              const itemFields = f.item.fields ?? [];
              const full = f.max_items !== undefined && rows.length >= f.max_items;
              return (
                <fieldset key={f.key} className="adm-dt-chooser">
                  <legend className="ss-sub">{f.label}</legend>
                  {hint && <p className="ss-meta">{hint}</p>}
                  {rows.map((row, i) => (
                    <div
                      key={i}
                      className="adm-form-row"
                      role="group"
                      aria-label={t('admin.schemaFormRows.row', { label: f.label, n: i + 1 })}
                    >
                      <SchemaForm
                        fields={itemFields}
                        values={isObj(row) ? row : {}}
                        {...(pickers ? { pickers } : {})}
                        {...(customText ? { customText } : {})}
                        onChange={(inner) =>
                          set(
                            f.key,
                            rows.map((r, j) => (j === i ? inner : r)),
                          )
                        }
                      />
                      <Button
                        size="sm"
                        variant="secondary"
                        onClick={() =>
                          set(
                            f.key,
                            rows.filter((_, j) => j !== i),
                          )
                        }
                      >
                        {t('admin.schemaFormRows.remove', { label: f.label, n: i + 1 })}
                      </Button>
                    </div>
                  ))}
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={full}
                    onClick={() => set(f.key, [...rows, defaultsOf(itemFields)])}
                  >
                    {t('admin.schemaFormRows.add', { label: f.label })}
                  </Button>
                </fieldset>
              );
            }
            if (f.item?.type === 'enum') {
              const chosen = Array.isArray(v) ? (v as string[]) : [];
              return (
                <fieldset key={f.key} className="adm-dt-chooser">
                  <legend className="ss-sub">{f.label}</legend>
                  {(f.item.options ?? []).map((o) => (
                    <Checkbox
                      key={o.value}
                      label={o.label}
                      checked={chosen.includes(o.value)}
                      onChange={(e) =>
                        set(
                          f.key,
                          e.target.checked
                            ? [...chosen, o.value]
                            : chosen.filter((x) => x !== o.value),
                        )
                      }
                    />
                  ))}
                </fieldset>
              );
            }
            return (
              <TextInput
                key={f.key}
                label={f.label}
                hint={
                  hint
                    ? `${hint} ${t('admin.schemaForm.commaHint')}`
                    : t('admin.schemaForm.commaHint')
                }
                value={textList(v)}
                onChange={(e) =>
                  set(
                    f.key,
                    e.target.value
                      .split(',')
                      .map((x) => x.trim())
                      .filter(Boolean)
                      .map((x) => (f.item?.type === 'int' ? Number(x) : x)),
                  )
                }
              />
            );
          }
          case 'group':
            return (
              <fieldset key={f.key} className="adm-dt-chooser">
                <legend className="ss-sub">{f.label}</legend>
                <SchemaForm
                  fields={f.fields ?? []}
                  values={isObj(v) ? v : {}}
                  {...(pickers ? { pickers } : {})}
                  {...(customText ? { customText } : {})}
                  onChange={(inner) => set(f.key, inner)}
                />
              </fieldset>
            );
          case 'role_picker':
          case 'channel_picker': {
            const choices = f.type === 'role_picker' ? pickers?.roles : pickers?.channels;
            if (!choices) return idInput(f, v);
            const options = choices.map((c) => ({ value: c.id, label: c.name }));
            return (
              <div key={f.key}>
                <span className="ss-sub">{f.label}</span>
                {f.multi ? (
                  <MultiSelect
                    aria-label={f.label}
                    options={options}
                    values={Array.isArray(v) ? (v as string[]) : []}
                    onValuesChange={(next) => set(f.key, next)}
                  />
                ) : (
                  <Select
                    aria-label={f.label}
                    options={[{ value: '', label: t('admin.schemaFormRows.none') }, ...options]}
                    value={typeof v === 'string' ? v : ''}
                    onValueChange={(next) => set(f.key, next)}
                  />
                )}
                {hint && <p className="ss-meta">{hint}</p>}
              </div>
            );
          }
          case 'emoji_picker':
            return idInput(f, v);
          default: {
            const custom = customText?.(f, typeof v === 'string' ? v : '', (next) =>
              set(f.key, next),
            );
            if (custom !== undefined) return <div key={f.key}>{custom}</div>;
            return (
              <TextInput
                key={f.key}
                label={f.label}
                {...(f.max_len ? { maxLength: f.max_len } : {})}
                {...(hint ? { hint } : {})}
                value={typeof v === 'string' ? v : ''}
                onChange={(e) => set(f.key, e.target.value)}
              />
            );
          }
        }
      })}
    </div>
  );
}
