// Channel editor (TV-ADM-001): identity, kind, time zone, restrictions, filler policy and, for rule-based channels,
// the rules (TV-ADM-002). Logo upload and the guide import are not on the server yet; the screen says so.
import { Button, Notice, Select, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { Banner, intOrNull, useLoad, useRun } from '../hooks';
import { RulesEditor, validateRules } from './RulesEditor';
import * as tv from './tvApi';
import { FILLER_POLICIES } from '../../../cinema/tv/types';

/** Intl is only the fallback: the server accepts the names in `GET /tv/timezones`. */
export const validTimezone = (tz: string, allowed?: readonly string[] | null): boolean => {
  if (!tz.trim()) return false;
  if (allowed && allowed.length > 0) return allowed.includes(tz.trim());
  try {
    new Intl.DateTimeFormat(undefined, { timeZone: tz });
    return true;
  } catch {
    return false;
  }
};

export interface ChannelEditorProps {
  channel: tv.AdminChannel | undefined;
  onSaved: (c: tv.AdminChannel) => void;
}

export function ChannelEditor({ channel, onSaved }: ChannelEditorProps) {
  const t = useT();
  const [number, setNumber] = useState(channel ? String(channel.number) : '');
  const [callSign, setCallSign] = useState(channel?.call_sign ?? '');
  const [name, setName] = useState(channel?.name ?? '');
  const [era, setEra] = useState(channel?.era_label ?? '');
  const [kind, setKind] = useState<string>(channel?.kind ?? 'rule_based');
  const [timezone, setTimezone] = useState(
    channel?.timezone ?? Intl.DateTimeFormat().resolvedOptions().timeZone ?? 'UTC',
  );
  const [restricted, setRestricted] = useState(channel?.age_restricted ?? false);
  const [enabled, setEnabled] = useState(channel?.enabled ?? true);
  const [filler, setFiller] = useState<string>(channel?.filler_policy ?? 'slate');
  const [rules, setRules] = useState<tv.ScheduleRules>(channel?.rules ?? tv.EMPTY_RULES);
  const { busy, error, notice, run } = useRun();
  const zones = useLoad<string[] | null>('tv-timezones', () =>
    tv.listTimezones().catch(() => null),
  );

  const n = intOrNull(number);
  const problem =
    n === null || n < 1 || n > 9999
      ? t('admin.channelEditor.problem.number')
      : !name.trim()
        ? t('admin.channelEditor.problem.name')
        : !validTimezone(timezone, zones.data)
          ? t('admin.channelEditor.problem.timezone')
          : kind === 'rule_based'
            ? validateRules(rules)
            : null;

  // Kinds the server can run today; an existing imported_guide channel keeps its kind.
  const kinds = [
    { value: 'rule_based', label: t('admin.channelEditor.kind.ruleBased') },
    { value: 'manual', label: t('admin.channelEditor.kind.manual') },
    ...(channel?.kind === 'imported_guide'
      ? [{ value: 'imported_guide', label: t('admin.channelEditor.kind.importedGuide') }]
      : []),
  ];

  const input = (): tv.ChannelInput => ({
    number: n ?? 0,
    name: name.trim(),
    call_sign: callSign.trim() || null,
    era_label: era.trim() || null,
    theme: channel?.theme ?? null,
    // The API hands out the logo as a signed URL only: send its key back so a save keeps the logo.
    ...(channel?.logo_key ? { logo_key: channel.logo_key } : {}),
    enabled,
    kind: kind as tv.AdminKind,
    timezone: timezone.trim(),
    age_restricted: restricted,
    filler_policy: filler as tv.FillerPolicy,
    rules: kind === 'rule_based' ? rules : null,
  });

  const save = () =>
    run(
      async () => {
        const saved = channel
          ? await tv.updateChannel(channel.id, input())
          : await tv.createChannel(input());
        onSaved(saved);
        return saved;
      },
      channel ? t('admin.channelEditor.saved') : t('admin.channelEditor.created'),
    );

  return (
    <form
      className="adm-form adm-cin-form"
      aria-label={t('admin.channelEditor.aria')}
      onSubmit={(e) => {
        e.preventDefault();
        if (!problem) void save();
      }}
    >
      <div className="adm-cin-grid">
        <TextInput
          label={t('admin.channelEditor.number')}
          type="number"
          min={1}
          max={9999}
          value={number}
          onChange={(e) => setNumber(e.target.value)}
        />
        <TextInput
          label={t('admin.channelEditor.callSign')}
          hint={t('admin.channelEditor.callSignHint')}
          value={callSign}
          onChange={(e) => setCallSign(e.target.value)}
        />
        <TextInput
          label={t('admin.channelEditor.name')}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <TextInput
          label={t('admin.channelEditor.era')}
          hint={t('admin.channelEditor.eraHint')}
          value={era}
          onChange={(e) => setEra(e.target.value)}
        />
        {zones.data && zones.data.length > 0 ? (
          <Select
            aria-label={t('admin.channelEditor.timezone')}
            searchable
            options={zones.data.map((z) => ({ value: z, label: z }))}
            value={timezone}
            onValueChange={setTimezone}
          />
        ) : (
          <TextInput
            label={t('admin.channelEditor.timezone')}
            hint={t('admin.channelEditor.timezoneHint')}
            value={timezone}
            onChange={(e) => setTimezone(e.target.value)}
          />
        )}
        <Select
          aria-label={t('admin.channelEditor.kind.label')}
          options={kinds}
          value={kind}
          onValueChange={setKind}
        />
        <Select
          aria-label={t('admin.channelEditor.filler')}
          options={FILLER_POLICIES.map((p) => ({ value: p, label: tv.FILLER_LABELS[p] }))}
          value={filler}
          onValueChange={setFiller}
        />
      </div>
      <Switch
        label={t('admin.channelEditor.ageRestricted')}
        checked={restricted}
        onChange={(e) => setRestricted(e.target.checked)}
      />
      <Switch
        label={t('admin.channelEditor.enabled')}
        checked={enabled}
        onChange={(e) => setEnabled(e.target.checked)}
      />

      {channel?.logo_url ? (
        <figure className="adm-cin-logo">
          <img src={channel.logo_url} alt={t('admin.channelEditor.logoAlt')} height={48} />
          <figcaption className="ss-meta">{t('admin.channelEditor.logoNotAvailable')}</figcaption>
        </figure>
      ) : (
        <p className="ss-meta">{t('admin.channelEditor.logoNotAvailableCallSign')}</p>
      )}

      {kind === 'rule_based' && (
        <section aria-label={t('admin.channelEditor.rules')} className="ss-card">
          <h2 className="ss-sub">{t('admin.channelEditor.rules')}</h2>
          <RulesEditor value={rules} onChange={setRules} />
        </section>
      )}
      {kind === 'manual' && (
        <Notice variant="info" title={t('admin.channelEditor.manual.title')}>
          {t('admin.channelEditor.manual.body')}
        </Notice>
      )}
      {kind === 'imported_guide' && (
        <Notice variant="info" title={t('admin.channelEditor.guide.title')}>
          {t('admin.channelEditor.guide.body')}
        </Notice>
      )}

      <Banner error={error} notice={notice} />
      {problem && n !== null && (
        <p className="ss-meta" role="status">
          {problem}
        </p>
      )}
      <div className="adm-actions">
        <Button type="submit" loading={busy} disabled={!!problem}>
          {channel ? t('admin.channelEditor.save') : t('admin.channelEditor.create')}
        </Button>
      </div>
    </form>
  );
}
