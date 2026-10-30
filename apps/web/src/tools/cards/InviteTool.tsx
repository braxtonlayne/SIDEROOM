import { useRouterState } from '@tanstack/react-router';
import { useEffect, useMemo, useState, type FormEvent } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { DM_GUILD, useChannels, type Channel } from '../../store';
import type { ToolProps } from '../registry';
import { createCard, type InviteCard } from './api';
import './cards.css';
import { deviceZone, zoneNames, zonedToUtcMs } from './time';
import { localParts, pickedTime } from '../timezone/zones';

const KINDS = ['game_night', 'cinema', 'live_tv'] as const;
const KIND_KEYS = {
  game_night: 'tools.invite.kindGameNight',
  cinema: 'tools.invite.kindCinema',
  live_tv: 'tools.invite.kindLiveTv',
} as const;
const TEXTY = new Set([0, 5, 1, 3]);
/** The title page of the Cinema section (`/cinema/t/<id>`). */
const TITLE_PATH = /^\/cinema\/t\/(\d{1,20})(?:[/?#].*)?$/;

/** The channel the member is looking at, from the address (`/channels/<server or @me>/<channel>`). */
export function channelFromPath(pathname: string): { guildId: string; channelId: string } | null {
  const m = /^\/channels\/([^/]+)\/([^/]+)/.exec(pathname);
  return m ? { guildId: m[1]!, channelId: m[2]! } : null;
}

/**
 * Reads what a host pasted into the link field: an app path, or a full address of this app. Returns the path
 * (`/cinema/t/12`) and, for a Cinema title page, the title id the watch-party hand-off needs. `null` when it is not
 * an address inside this app.
 */
export function parseCardLink(
  raw: string,
  origin: string,
): { path: string; titleId: string | null } | null {
  const text = raw.trim();
  if (!text) return { path: '', titleId: null };
  let path = text;
  if (/^[a-z][a-z0-9+.-]*:/i.test(text)) {
    try {
      const u = new URL(text);
      if (u.origin !== origin) return null;
      path = `${u.pathname}${u.search}`;
    } catch {
      return null;
    }
  }
  if (!path.startsWith('/') || path.startsWith('//') || /[\s\\]/.test(path) || path.length > 200)
    return null;
  const title = TITLE_PATH.exec(path);
  return { path, titleId: title ? title[1]! : null };
}

function channelLabel(c: Channel): string {
  if (c.type === 1 || c.type === 3)
    return (c.recipients ?? []).map((u) => u.username).join(', ') || c.name || c.id;
  return `#${c.name ?? c.id}`;
}

/** The invite card maker: a Cinema ticket, a Live TV slot or a game night, posted as a card with claimable seats. */
export default function InviteTool({ host }: ToolProps) {
  const t = useT();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const here = channelFromPath(pathname);
  const guildChannels = useChannels(here && here.guildId !== DM_GUILD ? here.guildId : undefined);
  const dms = useChannels(DM_GUILD);
  const options = useMemo(
    () => [...guildChannels, ...dms].filter((c) => TEXTY.has(c.type) && !c.is_message_request),
    [guildChannels, dms],
  );
  const zones = useMemo(() => zoneNames(), []);

  const [channelId, setChannelId] = useState(here?.channelId ?? '');
  const [kind, setKind] = useState<InviteCard['kind']>('game_night');
  const [title, setTitle] = useState('');
  const [note, setNote] = useState('');
  const [when, setWhen] = useState('');
  const [zone, setZone] = useState(deviceZone());
  const [seats, setSeats] = useState('6');
  const [minutes, setMinutes] = useState('120');
  const [link, setLink] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [posted, setPosted] = useState<InviteCard | null>(null);
  // UTL-CALC-001 hand-off (wave-32 integration): the instant last picked in the time zone tool, if any.
  const [picked, setPicked] = useState<number | null>(() => pickedTime.get());
  useEffect(() => pickedTime.subscribe(setPicked), []);
  const applyPicked = () => {
    if (picked === null) return;
    const p = localParts(zone, picked);
    if (!p) return;
    const two = (n: number) => String(n).padStart(2, '0');
    setWhen(`${p.year}-${two(p.month)}-${two(p.day)}T${two(p.hour)}:${two(p.minute)}`);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!channelId) return setError(t('tools.invite.needChannel'));
    if (!title.trim()) return setError(t('tools.invite.needTitle'));
    const startsAt = zonedToUtcMs(when, zone);
    if (startsAt === null) return setError(t('tools.invite.needTime'));
    if (startsAt < Date.now() - 5 * 60_000) return setError(t('tools.invite.timePast'));
    const n = Number(seats);
    if (!Number.isInteger(n) || n < 1 || n > 250) return setError(t('tools.invite.needSeats'));
    const parsed = parseCardLink(link, window.location.origin);
    if (!parsed) return setError(t('tools.invite.badLink'));
    const duration = Number(minutes);
    if (!Number.isInteger(duration) || duration < 15 || duration > 720)
      return setError(t('tools.invite.needLength'));
    setBusy(true);
    try {
      const card = await createCard(channelId, {
        kind,
        title: title.trim(),
        ...(note.trim() ? { note: note.trim() } : {}),
        starts_at: startsAt,
        duration_min: duration,
        timezone: zone,
        seats: n,
        ...(parsed.path ? { link_path: parsed.path } : {}),
        ...(kind === 'cinema' && parsed.titleId ? { title_id: parsed.titleId } : {}),
      });
      setPosted(card);
    } catch (err) {
      setError(
        err instanceof ApiError && err.status === 403
          ? t('tools.invite.errorDenied')
          : err instanceof ApiError && err.body?.message
            ? err.body.message
            : t('tools.invite.errorCreate'),
      );
    } finally {
      setBusy(false);
    }
  };

  if (posted) {
    return (
      <div className="sr-card">
        <p className="sr-card-done" role="status">
          {t('tools.invite.posted', { title: posted.title })}
        </p>
        <button type="button" className="sr-card-btn" onClick={() => setPosted(null)}>
          {t('tools.invite.another')}
        </button>
      </div>
    );
  }

  return (
    <form className="sr-card" data-host={host} onSubmit={(e) => void submit(e)} noValidate>
      <label className="sr-card-field">
        <span>{t('tools.invite.channel')}</span>
        <select value={channelId} onChange={(e) => setChannelId(e.target.value)}>
          <option value="">{t('tools.invite.pickChannel')}</option>
          {options.map((c) => (
            <option key={c.id} value={c.id}>
              {channelLabel(c)}
            </option>
          ))}
        </select>
      </label>
      <label className="sr-card-field">
        <span>{t('tools.invite.kind')}</span>
        <select value={kind} onChange={(e) => setKind(e.target.value as InviteCard['kind'])}>
          {KINDS.map((k) => (
            <option key={k} value={k}>
              {t(KIND_KEYS[k])}
            </option>
          ))}
        </select>
      </label>
      <label className="sr-card-field">
        <span>{t('tools.invite.title')}</span>
        <input value={title} maxLength={100} onChange={(e) => setTitle(e.target.value)} />
      </label>
      <label className="sr-card-field">
        <span>{t('tools.invite.note')}</span>
        <textarea value={note} maxLength={500} onChange={(e) => setNote(e.target.value)} />
      </label>
      <div className="sr-card-row">
        <label className="sr-card-field">
          <span>{t('tools.invite.when')}</span>
          <input type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} />
        </label>
        {picked !== null && (
          <button type="button" className="sr-card-btn" onClick={applyPicked}>
            {t('tools.invite.usePicked')}
          </button>
        )}
        <label className="sr-card-field">
          <span>{t('tools.invite.zone')}</span>
          <select value={zone} onChange={(e) => setZone(e.target.value)}>
            {zones.map((z) => (
              <option key={z} value={z}>
                {z}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="sr-card-row">
        <label className="sr-card-field">
          <span>{t('tools.invite.seats')}</span>
          <input
            type="number"
            inputMode="numeric"
            min={1}
            max={250}
            value={seats}
            onChange={(e) => setSeats(e.target.value)}
          />
        </label>
        <label className="sr-card-field">
          <span>{t('tools.invite.length')}</span>
          <input
            type="number"
            inputMode="numeric"
            min={15}
            max={720}
            step={5}
            value={minutes}
            onChange={(e) => setMinutes(e.target.value)}
          />
        </label>
      </div>
      <label className="sr-card-field">
        <span>{t('tools.invite.link')}</span>
        <input
          value={link}
          placeholder={kind === 'live_tv' ? '/cinema/tv' : '/cinema/t/123'}
          onChange={(e) => setLink(e.target.value)}
        />
      </label>
      <p className="sr-card-hint">
        {kind === 'cinema' ? t('tools.invite.linkHintCinema') : t('tools.invite.linkHint')}
      </p>
      {error && (
        <p className="sr-card-error" role="alert">
          {error}
        </p>
      )}
      <button type="submit" className="sr-card-btn" disabled={busy}>
        {t('tools.invite.post')}
      </button>
    </form>
  );
}
