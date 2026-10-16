import { useEffect, useMemo, useState } from 'react';
import { Icon, Popover, Slider, TextInput } from '@sideroom/ui';
import { useT } from '../../i18n';
import { onDispatch } from '../../store/events';
import { voiceController } from '../../voice/controller';
import { useVoiceSession } from '../../voice/session';
import {
  applySoundboardEvent,
  loadSounds,
  soundKey,
  startFavoritesSync,
  useSoundboard,
  type Sound,
} from '../../voice/soundboard';
import { useVoicePerms } from './perms';
import './voice-p13.css';

interface SectionProps {
  title: string;
  sounds: Sound[];
  disabled: boolean;
  favorites: string[];
  onPlay(s: Sound): void;
  onPreview(s: Sound): void;
  onFavorite(s: Sound): void;
}

function Section({
  title,
  sounds,
  disabled,
  favorites,
  onPlay,
  onPreview,
  onFavorite,
}: SectionProps) {
  const t = useT();
  if (!sounds.length) return null;
  return (
    <section className="vc-sb-section">
      <h3 className="vc-sb-heading">{title}</h3>
      <ul className="vc-sb-list">
        {sounds.map((s) => {
          const fav = favorites.includes(soundKey(s));
          return (
            <li key={soundKey(s)} className="vc-sb-row">
              <button
                type="button"
                className="vc-sb-play"
                disabled={disabled || !s.available}
                aria-label={t('lounge.soundboard.play', { name: s.name })}
                onClick={() => onPlay(s)}
              >
                <span className="vc-sb-emoji" aria-hidden="true">
                  {s.emoji ?? '\u{1F50A}'}
                </span>
                <span className="vc-sb-name">{s.name}</span>
              </button>
              <button
                type="button"
                className="lng-icon-btn"
                aria-label={t('lounge.soundboard.preview', { name: s.name })}
                onClick={() => onPreview(s)}
              >
                <Icon name="headphones" size={16} />
              </button>
              <button
                type="button"
                className="lng-icon-btn"
                aria-label={t('lounge.soundboard.favorite', { name: s.name })}
                aria-pressed={fav}
                data-active={fav || undefined}
                onClick={() => onFavorite(s)}
              >
                <Icon name="star" size={16} />
              </button>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function Body({ guildId, channelId }: { guildId: string; channelId: string }) {
  const t = useT();
  const session = useVoiceSession();
  const perms = useVoicePerms(guildId, channelId);
  const defaults = useSoundboard((s) => s.defaults);
  const server = useSoundboard((s) => s.guilds[guildId]);
  const favorites = useSoundboard((s) => s.favorites);
  const volume = useSoundboard((s) => s.volume);
  const toggleFavorite = useSoundboard((s) => s.toggleFavorite);
  const setVolume = useSoundboard((s) => s.setVolume);
  const [query, setQuery] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const silenced = session.selfMute || session.selfDeaf || session.serverMute || session.serverDeaf;
  const blocked = !perms.useSoundboard || silenced || session.status !== 'connected';

  useEffect(() => startFavoritesSync(), []);

  useEffect(() => {
    let dead = false;
    loadSounds(guildId).catch(() => {
      if (!dead) setFailed(true);
    });
    const off = onDispatch((event, data) => applySoundboardEvent(event, data));
    return () => {
      dead = true;
      off();
    };
  }, [guildId]);

  const q = query.trim().toLowerCase();
  const match = (s: Sound) => !q || s.name.toLowerCase().includes(q);
  const all = useMemo(() => [...(server ?? []), ...(defaults ?? [])], [server, defaults]);
  const favSounds = all.filter((s) => favorites.includes(soundKey(s)) && match(s));
  const serverSounds = (server ?? []).filter(match);
  const defaultSounds = (defaults ?? []).filter(match);
  const nothing = !favSounds.length && !serverSounds.length && !defaultSounds.length;

  const play = (s: Sound) => {
    setMessage(null);
    void voiceController()
      .playSoundboard(s)
      .then((r) => setMessage(r.ok ? null : r.message));
  };
  const preview = (s: Sound) => {
    setMessage(null);
    void voiceController()
      .previewSound(s)
      .then((ok) => setMessage(ok ? null : t('lounge.soundboard.previewFailed')));
  };
  const rowProps = {
    disabled: blocked,
    favorites,
    onPlay: play,
    onPreview: preview,
    onFavorite: (s: Sound) => toggleFavorite(soundKey(s)),
  };

  return (
    <div className="vc-sb" data-testid="soundboard">
      <TextInput
        label={t('lounge.soundboard.search')}
        type="search"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <Slider
        label={t('lounge.soundboard.volume')}
        min={0}
        max={100}
        step={5}
        value={volume}
        showValue
        formatValue={(v) => `${v}%`}
        onValueChange={setVolume}
      />
      {!perms.useSoundboard ? (
        <p className="vc-note" role="status">
          {t('lounge.soundboard.noPermission')}
        </p>
      ) : silenced ? (
        <p className="vc-note" role="status">
          {t('lounge.soundboard.muted')}
        </p>
      ) : null}
      {message && (
        <p className="vc-inline-error" role="alert">
          {message}
        </p>
      )}
      {failed && (
        <p className="vc-inline-error" role="alert">
          {t('lounge.soundboard.loadFailed')}
        </p>
      )}
      <div className="vc-sb-scroll">
        <Section title={t('lounge.soundboard.favorites')} sounds={favSounds} {...rowProps} />
        <Section title={t('lounge.soundboard.server')} sounds={serverSounds} {...rowProps} />
        <Section title={t('lounge.soundboard.defaults')} sounds={defaultSounds} {...rowProps} />
        {nothing && (defaults || failed) && (
          <p className="vc-note" role="status">
            {t('lounge.soundboard.empty')}
          </p>
        )}
      </div>
    </div>
  );
}

/**
 * The soundboard popover of the call controls (VOX-SB-002): search, favorites, this server's sounds and the
 * default ones, a preview that only you hear, and a volume for what you play. Playing a sound announces it to
 * the room and mixes the file into your microphone stream (VOX-SB-003).
 */
export function SoundboardButton({
  guildId,
  channelId,
  compact,
}: {
  guildId: string;
  channelId: string;
  /** The small icon button of the voice panel. */
  compact?: boolean;
}) {
  const t = useT();
  return (
    <Popover
      label={t('lounge.soundboard.title')}
      placement="top"
      className="vc-sb-popover"
      trigger={
        <button
          type="button"
          className={compact ? 'lng-icon-btn' : 'vc-ctl'}
          aria-label={t('lounge.soundboard.title')}
        >
          <Icon name="soundboard" size={compact ? 20 : 24} />
        </button>
      }
    >
      <Body guildId={guildId} channelId={channelId} />
    </Popover>
  );
}
