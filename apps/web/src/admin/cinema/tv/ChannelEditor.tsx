// Channel editor (TV-ADM-001): identity, kind, time zone, restrictions, filler policy, logo and, for rule-based
// channels, the rules (TV-ADM-002).
import { Button, Notice, Select, Switch, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { putFile } from '../../../lounge/msg/upload';
import { ArtworkPicker } from '../ArtworkPicker';
import { Banner, intOrNull, useRun } from '../hooks';
import { RulesEditor, validateRules } from './RulesEditor';
import * as tv from './tvApi';
import { CHANNEL_KINDS, FILLER_POLICIES } from '../../../cinema/tv/types';

export const validTimezone = (tz: string): boolean => {
  if (!tz.trim()) return false;
  try {
    new Intl.DateTimeFormat(undefined, { timeZone: tz });
    return true;
  } catch {
    return false;
  }
};

const CATEGORIES = [
  { value: '', label: 'Automatic (from what airs)' },
  { value: 'anime', label: 'Anime & animation' },
  { value: 'movies', label: 'Movies' },
  { value: 'series', label: 'Series' },
  { value: 'custom', label: 'Custom' },
];

export interface ChannelEditorProps {
  channel: tv.AdminChannel | undefined;
  onSaved: (c: tv.AdminChannel) => void;
}

export function ChannelEditor({ channel, onSaved }: ChannelEditorProps) {
  const [number, setNumber] = useState(channel ? String(channel.number) : '');
  const [callSign, setCallSign] = useState(channel?.call_sign ?? '');
  const [name, setName] = useState(channel?.name ?? '');
  const [network, setNetwork] = useState(channel?.network ?? '');
  const [era, setEra] = useState(channel?.era_label ?? '');
  const [kind, setKind] = useState<string>(channel?.kind ?? 'rule_based');
  const [category, setCategory] = useState<string>(channel?.category ?? '');
  const [timezone, setTimezone] = useState(
    channel?.timezone ?? Intl.DateTimeFormat().resolvedOptions().timeZone ?? 'UTC',
  );
  const [restricted, setRestricted] = useState(channel?.age_restricted ?? false);
  const [enabled, setEnabled] = useState(channel?.enabled ?? true);
  const [filler, setFiller] = useState<string>(channel?.filler_policy ?? 'slate');
  const [rules, setRules] = useState<tv.ScheduleRules>(channel?.rules ?? tv.EMPTY_RULES);
  const { busy, error, notice, run } = useRun();

  const n = intOrNull(number);
  const problem =
    n === null || n < 1 || n > 9999
      ? 'Give the channel a number between 1 and 9999.'
      : !callSign.trim()
        ? 'Call sign is required.'
        : !name.trim()
          ? 'Name is required.'
          : !validTimezone(timezone)
            ? 'Time zone must be an IANA name such as America/New_York.'
            : kind === 'rule_based'
              ? validateRules(rules, restricted)
              : null;

  const input = (logoKey?: string | null): tv.ChannelInput => ({
    number: n ?? 0,
    call_sign: callSign.trim(),
    name: name.trim(),
    network: network.trim() || null,
    era_label: era.trim() || null,
    ...(logoKey !== undefined ? { logo_key: logoKey } : {}),
    kind: kind as tv.ChannelKind,
    category: (category || null) as tv.ChannelCategory | null,
    timezone: timezone.trim(),
    age_restricted: restricted,
    enabled,
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

  const uploadLogo = async (file: File, onProgress: (f: number) => void) => {
    if (!channel) return;
    const slot = await tv.reserveLogo(channel.id, file.type);
    await putFile(slot.upload_url, file, onProgress, new AbortController().signal);
    onSaved(await tv.updateChannel(channel.id, input(slot.key)));
  };

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
          value={callSign}
          onChange={(e) => setCallSign(e.target.value)}
        />
        <TextInput label="Channel name" value={name} onChange={(e) => setName(e.target.value)} />
        <TextInput
          label="Network"
          hint='Shown under the call sign. Use "CUSTOM" for your own channels.'
          value={network}
          onChange={(e) => setNetwork(e.target.value)}
        />
        <TextInput
          label="Era label"
          hint='For example "Fall 2003".'
          value={era}
          onChange={(e) => setEra(e.target.value)}
        />
        <TextInput
          label="Time zone"
          hint="The schedule is written in this zone (D-103)."
          value={timezone}
          onChange={(e) => setTimezone(e.target.value)}
        />
        <Select
          aria-label="Channel kind"
          options={CHANNEL_KINDS}
          value={kind}
          onValueChange={setKind}
        />
        <Select
          aria-label="Guide category"
          options={CATEGORIES}
          value={category}
          onValueChange={setCategory}
        />
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

      {channel ? (
        <ArtworkPicker
          label="Channel logo"
          slot="logo"
          currentUrl={channel.logo_url}
          onUpload={uploadLogo}
        />
      ) : (
        <p className="ss-meta">Save the channel first, then add a logo.</p>
      )}

      {kind === 'rule_based' && (
        <section aria-label="Schedule rules" className="ss-card">
          <h2 className="ss-sub">Schedule rules</h2>
          <RulesEditor value={rules} onChange={setRules} channelRestricted={restricted} />
        </section>
      )}
      {kind === 'manual' && (
        <Notice variant="info" title="Weekly grid not available yet">
          Placing programmes on the weekly calendar is not part of this build. Use Now control to
          put a title on air.
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
