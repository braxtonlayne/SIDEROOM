// Apply a slowmode preset on demand (FTR-SLOW-001, G-74): pick a preset and how long it lasts, then apply it to the
// channel(s). The server (Slowmode schedules module) sets each channel's slowmode, audits it, and puts the old value
// back after the chosen time. Used by the channel menu dialog (one channel) and by the module's card in Server
// Settings (any channels).
import { Button, Checkbox, Notice, Select } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError } from '../../api/client';
import { formatDateTime, useT } from '../../i18n';
import { useChannels } from '../../store';
import * as api from '../../settings/server/featuresApi';

/** How long a preset stays: `0` keeps it until somebody changes the channel. */
const DURATIONS = [
  { minutes: 0, key: 'lounge.slowmode.keep' },
  { minutes: 15, key: 'lounge.slowmode.min15' },
  { minutes: 60, key: 'lounge.slowmode.hour1' },
  { minutes: 240, key: 'lounge.slowmode.hour4' },
  { minutes: 1440, key: 'lounge.slowmode.day1' },
] as const;

/** Channel types a slowmode applies to (Discord codes): text, voice chat, announcement, forum and media. */
const SLOWMODE_TYPES = new Set([0, 2, 5, 15, 16]);

export function SlowmodeForm({
  guildId,
  channelIds,
  onApplied,
}: {
  guildId: string;
  /** Fixed channels (the channel menu). Without it the person ticks channels. */
  channelIds?: string[] | undefined;
  onApplied?: (() => void) | undefined;
}) {
  const t = useT();
  const channels = useChannels(guildId);
  const [presets, setPresets] = useState<api.SlowmodePresets | null>(null);
  const [failed, setFailed] = useState(false);
  const [preset, setPreset] = useState('');
  const [minutes, setMinutes] = useState('0');
  const [picked, setPicked] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<api.AppliedSlowmode[] | null>(null);

  useEffect(() => {
    let live = true;
    api
      .getSlowmodePresets(guildId)
      .then((p) => {
        if (!live) return;
        setPresets(p);
        setPreset((cur) => cur || p.presets[0]?.id || '');
      })
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [guildId]);

  const targets = channelIds ?? picked;
  const name = (id: string) => channels.find((c) => c.id === id)?.name ?? id;
  const seconds = (n: number) =>
    n === 0 ? t('lounge.slowmode.off') : t('lounge.slowmode.secs', { n });

  const apply = async () => {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const out = await api.applySlowmode(guildId, {
        preset_id: preset,
        channel_ids: targets,
        ...(Number(minutes) > 0 ? { minutes: Number(minutes) } : {}),
      });
      setDone(out.applied);
      onApplied?.();
    } catch (e) {
      setError(
        e instanceof ApiError && e.message && e.status !== 500
          ? e.message
          : t('lounge.slowmode.failed'),
      );
    } finally {
      setBusy(false);
    }
  };

  if (failed)
    return (
      <Notice variant="danger" role="alert">
        {t('lounge.slowmode.loadFailed')}
      </Notice>
    );
  if (!presets)
    return (
      <p className="lng-muted" role="status">
        {t('lounge.slowmode.loading')}
      </p>
    );
  if (!presets.enabled) return <Notice variant="info">{t('lounge.slowmode.moduleOff')}</Notice>;

  return (
    <form
      className="lng-comm-stack"
      onSubmit={(e) => {
        e.preventDefault();
        void apply();
      }}
    >
      <Select
        aria-label={t('lounge.slowmode.preset')}
        options={presets.presets.map((p) => ({
          value: p.id,
          label: `${p.name} (${seconds(p.seconds)})`,
        }))}
        value={preset}
        onValueChange={setPreset}
      />
      <Select
        aria-label={t('lounge.slowmode.duration')}
        options={DURATIONS.map((d) => ({ value: String(d.minutes), label: t(d.key) }))}
        value={minutes}
        onValueChange={setMinutes}
      />
      {!channelIds && (
        <fieldset className="lng-comm-stack">
          <legend>{t('lounge.slowmode.channels')}</legend>
          {channels
            .filter((c) => SLOWMODE_TYPES.has(c.type))
            .map((c) => (
              <Checkbox
                key={c.id}
                label={`#${c.name}`}
                checked={picked.includes(c.id)}
                onChange={(e) =>
                  setPicked((cur) =>
                    e.target.checked ? [...cur, c.id] : cur.filter((x) => x !== c.id),
                  )
                }
              />
            ))}
        </fieldset>
      )}
      {error && (
        <p className="lng-failed" role="alert">
          {error}
        </p>
      )}
      <div>
        <Button type="submit" size="sm" disabled={busy || !preset || targets.length === 0}>
          {t('lounge.slowmode.apply')}
        </Button>
      </div>
      {done && (
        <ul className="lng-saved-list" aria-label={t('lounge.slowmode.result')}>
          {done.map((a) => (
            <li key={a.channel_id} className="lng-saved-item">
              {a.revert_at
                ? t('lounge.slowmode.appliedUntil', {
                    channel: name(a.channel_id),
                    now: seconds(a.seconds),
                    before: seconds(a.before),
                    when: formatDateTime(a.revert_at),
                  })
                : t('lounge.slowmode.applied', {
                    channel: name(a.channel_id),
                    now: seconds(a.seconds),
                  })}
            </li>
          ))}
          {done.length === 0 && <li className="lng-saved-item">{t('lounge.slowmode.nothing')}</li>}
        </ul>
      )}
    </form>
  );
}
