// Renders a feature's settings schema (crates/sr-features/src/schema.rs) as a form (ADM-PLT-001). Values are
// plain JSON: bool, integer, string, whole seconds, `#rrggbb`, snowflake strings, arrays and groups.
// The server validates every value; this form only keeps the shape right and shows the schema's own limits.
import { Checkbox, Select, TextInput } from '@sideroom/ui';
import { useT } from '../i18n';

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
  item?: { type: string };
  fields?: SchemaField[];
}
export type SettingsValues = Record<string, unknown>;

/** Fields of a schema value as the API sends it; anything malformed is an empty form. */
export function schemaFields(schema: unknown): SchemaField[] {
  const f = (schema as { fields?: unknown } | null)?.fields;
  return Array.isArray(f) ? (f as SchemaField[]) : [];
}

const isObj = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/**
 * The platform overrides to save: every value that differs from the schema default. Groups recurse and only
 * appear when something inside differs. A field the schema leaves unset with an empty value is not an override.
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
    if (JSON.stringify(v) === JSON.stringify(f.default)) continue;
    if (Array.isArray(v) && v.length === 0 && f.default === undefined) continue;
    out[f.key] = v;
  }
  return out;
}

const textList = (v: unknown) => (Array.isArray(v) ? v.join(', ') : '');

export function SchemaForm({
  fields,
  values,
  onChange,
}: {
  fields: SchemaField[];
  values: SettingsValues;
  onChange: (next: SettingsValues) => void;
}) {
  const t = useT();
  const set = (key: string, v: unknown) => onChange({ ...values, [key]: v });
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
          case 'list':
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
          case 'group':
            return (
              <fieldset key={f.key} className="adm-dt-chooser">
                <legend className="ss-sub">{f.label}</legend>
                <SchemaForm
                  fields={f.fields ?? []}
                  values={isObj(v) ? v : {}}
                  onChange={(inner) => set(f.key, inner)}
                />
              </fieldset>
            );
          case 'role_picker':
          case 'channel_picker':
          case 'emoji_picker':
            return (
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
          default:
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
      })}
    </div>
  );
}
