import { Button, Modal, TextArea, TextInput } from '@sideroom/ui';
import { useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { useT, type MessageKey } from '../../i18n';
import { EventCoverField } from './EventCoverField';
import { useChannels, useGuild } from '../data';
import { createEvent, EVENT_ENTITY } from './api';
import './events.css';

type Kind = 'voice' | 'stage' | 'external';

const KIND_TEXT: Record<Kind, { label: MessageKey; blurb: MessageKey }> = {
  voice: { label: 'lounge.createEvent.kindVoice', blurb: 'lounge.createEvent.kindVoiceBlurb' },
  stage: { label: 'lounge.createEvent.kindStage', blurb: 'lounge.createEvent.kindStageBlurb' },
  external: {
    label: 'lounge.createEvent.kindExternal',
    blurb: 'lounge.createEvent.kindExternalBlurb',
  },
};

const KIND_ENTITY: Record<Kind, number> = {
  voice: EVENT_ENTITY.voice,
  stage: EVENT_ENTITY.stage,
  external: EVENT_ENTITY.external,
};

/** `YYYY-MM-DDTHH:mm` in local time, the value format of `datetime-local`. */
export const toLocalInput = (d: Date): string => {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
};

/** The next whole hour, `hoursLater` hours on. */
const defaultTime = (hoursLater: number) => {
  const d = new Date();
  d.setMinutes(0, 0, 0);
  d.setHours(d.getHours() + hoursLater);
  return toLocalInput(d);
};

/**
 * Create Event (`POST /guilds/{id}/scheduled-events`): where it happens (a voice channel, a stage or
 * somewhere else with a location), when, and what it is. Anyone with Create Events may.
 */
export function CreateEventDialog({
  guildId,
  channelId,
  onClose,
  onCreated,
}: {
  guildId: string;
  channelId?: string | undefined;
  onClose: () => void;
  onCreated?: () => void;
}) {
  const t = useT();
  const guild = useGuild(guildId);
  const channels = useChannels(guildId);
  const voiceChannels = channels.filter((c) => c.type === 'voice');
  const stageChannels = channels.filter((c) => c.type === 'stage');
  const preset = channels.find((c) => c.id === channelId);
  const [kind, setKind] = useState<Kind>(
    preset?.type === 'stage' ? 'stage' : preset?.type === 'voice' ? 'voice' : 'voice',
  );
  const [channel, setChannel] = useState(preset?.id ?? '');
  const [location, setLocation] = useState('');
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [start, setStart] = useState(defaultTime(1));
  const [end, setEnd] = useState(defaultTime(2));
  const [cover, setCover] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const p = guild?.permissions;
  // Client-side hiding is a convenience; the server enforces CREATE_EVENTS or MANAGE_EVENTS.
  if (p && p.createEvents === false && p.manageEvents === false) return null;

  const pickable = kind === 'stage' ? stageChannels : voiceChannels;
  const chosen = pickable.find((c) => c.id === channel)?.id ?? pickable[0]?.id ?? '';

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const clean = name.trim();
    if (!clean) return setError(t('lounge.createEvent.nameRequired'));
    const startAt = new Date(start);
    if (Number.isNaN(startAt.getTime())) return setError(t('lounge.createEvent.startRequired'));
    if (startAt.getTime() <= Date.now()) return setError(t('lounge.createEvent.startInPast'));
    const endAt = end ? new Date(end) : null;
    if (kind === 'external' && (!endAt || Number.isNaN(endAt.getTime())))
      return setError(t('lounge.createEvent.endRequired'));
    if (endAt && endAt.getTime() <= startAt.getTime())
      return setError(t('lounge.createEvent.endBeforeStart'));
    if (kind === 'external' && !location.trim())
      return setError(t('lounge.createEvent.locationRequired'));
    if (kind !== 'external' && !chosen) return setError(t('lounge.createEvent.channelRequired'));
    setBusy(true);
    setError(null);
    try {
      await createEvent(guildId, {
        name: clean,
        ...(description.trim() ? { description: description.trim() } : {}),
        entity_type: KIND_ENTITY[kind],
        ...(kind === 'external'
          ? { entity_metadata: { location: location.trim() } }
          : { channel_id: chosen }),
        scheduled_start_time: startAt.toISOString(),
        ...(endAt ? { scheduled_end_time: endAt.toISOString() } : {}),
        privacy_level: 2,
        ...(cover ? { image: cover } : {}),
      });
      onCreated?.();
      onClose();
    } catch (err) {
      setError(
        err instanceof ApiError && err.status === 403
          ? t('lounge.createEvent.forbidden')
          : err instanceof ApiError && err.message
            ? err.message
            : t('lounge.createEvent.failed'),
      );
      setBusy(false);
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={t('lounge.createEvent.title')}
      description={t('lounge.createEvent.description')}
    >
      <form className="lng-form" onSubmit={(e) => void submit(e)} noValidate>
        <fieldset className="lng-kind">
          <legend>{t('lounge.createEvent.whereLegend')}</legend>
          {(['voice', 'stage', 'external'] as const).map((k) => (
            <label key={k} className="lng-kind-opt" data-selected={kind === k || undefined}>
              <input
                type="radio"
                name="event-kind"
                value={k}
                checked={kind === k}
                onChange={() => setKind(k)}
              />
              <span>
                <strong>{t(KIND_TEXT[k].label)}</strong>
                <small>{t(KIND_TEXT[k].blurb)}</small>
              </span>
            </label>
          ))}
        </fieldset>
        {kind === 'external' ? (
          <TextInput
            label={t('lounge.createEvent.location')}
            value={location}
            maxLength={100}
            onChange={(e) => setLocation(e.target.value)}
          />
        ) : (
          <div>
            <label className="lng-field-label" htmlFor="event-channel">
              {t('lounge.createEvent.channel')}
            </label>
            <select
              id="event-channel"
              className="lng-select"
              value={chosen}
              onChange={(e) => setChannel(e.target.value)}
            >
              {pickable.length === 0 && (
                <option value="">{t('lounge.createEvent.noChannels')}</option>
              )}
              {pickable.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </div>
        )}
        <TextInput
          label={t('lounge.createEvent.name')}
          value={name}
          maxLength={100}
          autoFocus
          onChange={(e) => setName(e.target.value)}
        />
        <div className="lng-event-times">
          <TextInput
            type="datetime-local"
            label={t('lounge.createEvent.start')}
            value={start}
            onChange={(e) => setStart(e.target.value)}
          />
          <TextInput
            type="datetime-local"
            label={
              kind === 'external'
                ? t('lounge.createEvent.end')
                : t('lounge.createEvent.endOptional')
            }
            value={end}
            onChange={(e) => setEnd(e.target.value)}
          />
        </div>
        <TextArea
          label={t('lounge.createEvent.descriptionLabel')}
          value={description}
          maxLength={1000}
          onChange={(e) => setDescription(e.target.value)}
        />
        <EventCoverField value={cover} onChange={setCover} disabled={busy} />
        {error && (
          <p role="alert" className="lng-failed">
            {error}
          </p>
        )}
        <div className="lng-form-actions">
          <Button type="button" variant="ghost" onClick={onClose}>
            {t('lounge.createEvent.cancel')}
          </Button>
          <Button type="submit" loading={busy}>
            {t('lounge.createEvent.create')}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
