// Channel editor (TV-ADM-001): identity, kind, time zone, restrictions, filler policy and, for rule-based channels,
// the rules (TV-ADM-002). Logo upload and the guide import are not on the server yet; the screen says so.
import { Button, Notice, Select, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
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
      ? 'Give the channel a number between 1 and 9999.'
      : !name.trim()
        ? 'Name is required.'
        : !validTimezone(timezone, zones.data)
          ? 'Pick a time zone from the list (an IANA name such as America/New_York).'
          : kind === 'rule_based'
            ? validateRules(rules)
            : null;

  // Kinds the server can run today; an existing imported_guide channel keeps its kind.
  const kinds = [
    { value: 'rule_based', label: 'Rule based' },
    { value: 'manual', label: 'Manual calendar' },
    ...(channel?.kind === 'imported_guide'
      ? [{ value: 'imported_guide', label: 'Imported guide' }]
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
      channel ? 'Saved.' : 'Channel created.',
    );

  return (
    <form
      className="adm-form adm-cin-form"
      aria-label="Channel details"
      onSubmit={(e) => {
        e.preventDefault();
        if (!problem) void save();
      }}
    >
      <div className="adm-cin-grid">
        <TextInput
          label="Channel number"
          type="number"
          min={1}
          max={9999}
          value={number}
          onChange={(e) => setNumber(e.target.value)}
        />
        <TextInput
          label="Call sign"
          hint="The logo text in the guide. Empty uses the name."
          value={callSign}
          onChange={(e) => setCallSign(e.target.value)}
        />
        <TextInput label="Channel name" value={name} onChange={(e) => setName(e.target.value)} />
        <TextInput
          label="Era label"
          hint='Shown under the call sign, for example "Fall 2003".'
          value={era}
          onChange={(e) => setEra(e.target.value)}
        />
        {zones.data && zones.data.length > 0 ? (
          <Select
            aria-label="Time zone"
            searchable
            options={zones.data.map((z) => ({ value: z, label: z }))}
            value={timezone}
            onValueChange={setTimezone}
          />
        ) : (
          <TextInput
            label="Time zone"
            hint="The schedule is written in this zone (D-103)."
            value={timezone}
            onChange={(e) => setTimezone(e.target.value)}
          />
        )}
        <Select aria-label="Channel kind" options={kinds} value={kind} onValueChange={setKind} />
        <Select
          aria-label="Filler policy"
          options={FILLER_POLICIES.map((p) => ({ value: p, label: tv.FILLER_LABELS[p] }))}
          value={filler}
          onValueChange={setFiller}
        />
      </div>
      <Switch
        label="Age-restricted channel"
        checked={restricted}
        onChange={(e) => setRestricted(e.target.checked)}
      />
      <Switch label="Enabled" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />

      {channel?.logo_url ? (
        <figure className="adm-cin-logo">
          <img src={channel.logo_url} alt="Channel logo" height={48} />
          <figcaption className="ss-meta">Logo upload is not available yet.</figcaption>
        </figure>
      ) : (
        <p className="ss-meta">Logo upload is not available yet; the call sign is shown instead.</p>
      )}

      {kind === 'rule_based' && (
        <section aria-label="Schedule rules" className="ss-card">
          <h2 className="ss-sub">Schedule rules</h2>
          <RulesEditor value={rules} onChange={setRules} />
        </section>
      )}
      {kind === 'manual' && (
        <Notice variant="info" title="Weekly grid not available yet">
          A drag-and-drop weekly grid is not part of this build. Place programmes at a time from the
          Now control tab.
        </Notice>
      )}
      {kind === 'imported_guide' && (
        <Notice variant="info" title="Guide import not available yet">
          Importing a CSV or JSON guide with the mapping report is not part of this build.
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
          {channel ? 'Save channel' : 'Create channel'}
        </Button>
      </div>
    </form>
  );
}
