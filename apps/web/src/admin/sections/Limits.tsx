import { Button, Notice, Switch, TextInput } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import * as api from '../api';
import { useT, type MessageKey } from '../../i18n';
import { Async, ReasonDialog, SectionHeader, fmtMs, useResource } from '../ui';

const GROUP_LABEL: Record<string, MessageKey> = {
  limits: 'admin.limits.groupLimits',
  invites: 'admin.limits.groupInvites',
  retention: 'admin.limits.groupRetention',
  abuse: 'admin.limits.groupAbuse',
};
const groupLabel = (g: string, t: (key: MessageKey) => string) =>
  GROUP_LABEL[g] ? t(GROUP_LABEL[g]) : g.charAt(0).toUpperCase() + g.slice(1);

/** Edit-box text for a setting value. */
const asText = (v: unknown) => (typeof v === 'string' ? v : String(v ?? ''));

/** Parse the edit box back to the JSON value the API validates; null when not a number/bool yet. */
function parseValue(s: api.SettingView, text: string): unknown {
  if (s.kind === 'boolean') return text === 'true';
  if (s.kind === 'text') return text;
  if (text.trim() === '') return NaN;
  return Number(text);
}

export function LimitsSection() {
  const t = useT();
  const res = useResource('admin:settings', api.listSettings);
  return (
    <>
      <SectionHeader title={t('admin.limits.title')} description={t('admin.limits.description')} />
      <Async res={res} label={t('admin.limits.loading')}>
        {(list) => <LimitsForm list={list} reload={res.reload} />}
      </Async>
    </>
  );
}

function LimitsForm({ list, reload }: { list: api.SettingView[]; reload: () => Promise<void> }) {
  const t = useT();
  const [edits, setEdits] = useState<Record<string, string>>({});
  const [confirm, setConfirm] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const groups = useMemo(() => {
    const m = new Map<string, api.SettingView[]>();
    for (const s of list) m.set(s.group, [...(m.get(s.group) ?? []), s]);
    return [...m.entries()];
  }, [list]);

  const changed = useMemo(() => {
    const out: Record<string, unknown> = {};
    for (const s of list) {
      const txt = edits[s.key];
      if (txt === undefined || txt === asText(s.value)) continue;
      out[s.key] = parseValue(s, txt);
    }
    return out;
  }, [edits, list]);
  const errors = useMemo(() => {
    const out: Record<string, string> = {};
    for (const s of list) {
      if (!(s.key in changed)) continue;
      const v = changed[s.key];
      if (s.kind === 'integer' || s.kind === 'number') {
        if (typeof v !== 'number' || !Number.isFinite(v))
          out[s.key] = t('admin.limits.enterNumber');
        else if (s.kind === 'integer' && !Number.isInteger(v))
          out[s.key] = t('admin.limits.enterWholeNumber');
        else if (v < s.min || v > s.max)
          out[s.key] = t('admin.limits.mustBeBetween', { min: s.min, max: s.max });
      }
    }
    return out;
  }, [changed, list, t]);
  const count = Object.keys(changed).length;
  const bad = Object.keys(errors).length > 0;

  return (
    <>
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <form
        className="ss-form"
        style={{ maxWidth: 720 }}
        onSubmit={(e) => {
          e.preventDefault();
          if (count > 0 && !bad) setConfirm(true);
        }}
      >
        {groups.map(([group, items]) => (
          <section key={group} className="ss-card" aria-labelledby={`lim-${group}`}>
            <h2 id={`lim-${group}`}>{groupLabel(group, t)}</h2>
            {items.map((s) => {
              const text = edits[s.key] ?? asText(s.value);
              const hint = (
                <>
                  {s.description}
                  {s.kind !== 'boolean' && s.kind !== 'text'
                    ? t('admin.limits.range', { min: s.min, max: s.max })
                    : ''}
                  {s.is_default
                    ? t('admin.limits.usingDefault')
                    : s.updated_at_ms
                      ? t('admin.limits.defaultChanged', {
                          value: asText(s.default),
                          when: fmtMs(s.updated_at_ms),
                        })
                      : t('admin.limits.defaultValue', { value: asText(s.default) })}
                </>
              );
              return s.kind === 'boolean' ? (
                <Switch
                  key={s.key}
                  label={s.key}
                  description={hint}
                  checked={text === 'true'}
                  onChange={(e) => setEdits({ ...edits, [s.key]: String(e.target.checked) })}
                />
              ) : (
                <TextInput
                  key={s.key}
                  label={s.key}
                  hint={hint}
                  error={errors[s.key]}
                  type={s.kind === 'text' ? 'text' : 'number'}
                  step={s.kind === 'integer' ? 1 : 'any'}
                  value={text}
                  onChange={(e) => setEdits({ ...edits, [s.key]: e.target.value })}
                />
              );
            })}
          </section>
        ))}
        <div className="adm-actions">
          <Button type="submit" disabled={count === 0 || bad}>
            {count > 0 ? t('admin.limits.saveChanges', { count }) : t('admin.limits.saveNone')}
          </Button>
          <Button variant="ghost" disabled={count === 0} onClick={() => setEdits({})}>
            {t('admin.limits.discard')}
          </Button>
        </div>
      </form>
      <ReasonDialog
        open={confirm}
        title={t('admin.limits.confirmTitle', { count })}
        description={Object.keys(changed).join(', ')}
        confirmLabel={t('admin.limits.confirmLabel')}
        onCancel={() => setConfirm(false)}
        onConfirm={async (reason) => {
          await api.patchSettings(changed, reason);
          setConfirm(false);
          setEdits({});
          setNotice(t('admin.limits.saved'));
          await reload();
        }}
      />
    </>
  );
}
