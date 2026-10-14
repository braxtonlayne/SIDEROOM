// The control bar (CIN-PLY-002): every control is a real button or native input with an accessible name; the
// menus come from the UI kit's DropdownMenu so arrows, Home/End, typeahead and Escape work as everywhere else.
import { DropdownMenu, type MenuEntry } from '@sideroom/ui';
import type { ReactElement } from 'react';
import {
  AudioIcon,
  Back10,
  Captions,
  Cast,
  Cog,
  ExitFullscreen,
  Fullscreen,
  Fwd10,
  Gauge,
  PauseGlyph,
  PlayGlyph,
  Pip,
  SkipNext,
  VolumeGlyph,
} from './icons';
import { SPEEDS, formatClock, nextLine } from './model';
import { Scrubber } from './Scrubber';
import { playerSession, usePlayer } from './session';
import type { SubBackground, SubColor, SubSize } from './model';

export interface ControlsProps {
  fullscreen: boolean;
  canFullscreen: boolean;
  canPip: boolean;
  canRemote: boolean;
  onFullscreen: () => void;
  onPip: () => void;
  onRemote: () => void;
  onNext: () => void;
  onMenuOpenChange: (open: boolean) => void;
}

const SIZES: [SubSize, string][] = [
  ['small', 'Small'],
  ['medium', 'Medium'],
  ['large', 'Large'],
  ['xlarge', 'Extra large'],
];
const COLORS: [SubColor, string][] = [
  ['white', 'White'],
  ['yellow', 'Yellow'],
  ['cyan', 'Cyan'],
  ['green', 'Green'],
];
const BACKGROUNDS: [SubBackground, string][] = [
  ['none', 'None'],
  ['translucent', 'Translucent'],
  ['solid', 'Solid'],
];

function IconButton({
  label,
  onClick,
  pressed,
  children,
  disabled,
}: {
  label: string;
  onClick?: () => void;
  pressed?: boolean;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      className="cpl-btn"
      aria-label={label}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function MenuButton({
  label,
  menu,
  items,
  onOpenChange,
  children,
}: {
  label: string;
  menu: string;
  items: MenuEntry[];
  onOpenChange: (open: boolean) => void;
  children: React.ReactNode;
}) {
  const trigger: ReactElement<{ onClick?: () => void }> = (
    <button type="button" className="cpl-btn" aria-label={label}>
      {children}
    </button>
  );
  return (
    <DropdownMenu
      trigger={trigger}
      items={items}
      label={menu}
      placement="top-end"
      onOpenChange={onOpenChange}
    />
  );
}

export function Controls(p: ControlsProps) {
  const s = playerSession();
  const st = usePlayer((x) => x);
  const info = st.info;
  if (!info) return null;

  const audio: MenuEntry[] = [
    {
      type: 'radio-group',
      label: 'Audio',
      value: st.audioId ?? '',
      onValueChange: (v) => s.setAudioTrack(v),
      options: st.audioTracks.map((t) => ({ value: t.id, label: t.label })),
    },
  ];

  const subtitles: MenuEntry[] = [
    {
      type: 'radio-group',
      label: 'Subtitles',
      value: st.subtitleId ?? 'off',
      onValueChange: (v) => void s.setSubtitle(v === 'off' ? null : v),
      options: [
        { value: 'off', label: 'Off' },
        ...info.subtitle_tracks.map((t) => ({
          value: t.id,
          label: t.forced ? `${t.label} (forced)` : t.label,
        })),
      ],
    },
    { type: 'separator' },
    {
      type: 'radio-group',
      label: 'Text size',
      value: st.subtitleStyle.size,
      onValueChange: (v) => s.setSubtitleStyle({ size: v as SubSize }),
      options: SIZES.map(([value, label]) => ({ value, label })),
    },
    {
      type: 'radio-group',
      label: 'Text color',
      value: st.subtitleStyle.color,
      onValueChange: (v) => s.setSubtitleStyle({ color: v as SubColor }),
      options: COLORS.map(([value, label]) => ({ value, label })),
    },
    {
      type: 'radio-group',
      label: 'Background',
      value: st.subtitleStyle.background,
      onValueChange: (v) => s.setSubtitleStyle({ background: v as SubBackground }),
      options: BACKGROUNDS.map(([value, label]) => ({ value, label })),
    },
  ];

  const speed: MenuEntry[] = [
    {
      type: 'radio-group',
      label: 'Playback speed',
      value: String(st.rate),
      onValueChange: (v) => s.setRate(Number(v)),
      options: SPEEDS.map((r) => ({ value: String(r), label: r === 1 ? 'Normal' : `${r}×` })),
    },
  ];

  const current = st.levels.find((l) => l.index === st.playingLevel);
  const quality: MenuEntry[] = [
    {
      type: 'radio-group',
      label: 'Quality',
      value: String(st.quality),
      onValueChange: (v) => s.setQuality(v === 'auto' ? 'auto' : Number(v)),
      options: [
        { value: 'auto', label: current ? `Auto (${current.height}p)` : 'Auto' },
        ...[...st.levels]
          .sort((a, b) => b.height - a.height)
          .map((l) => ({ value: String(l.index), label: `${l.height}p` })),
      ],
    },
    { type: 'separator' },
    {
      type: 'checkbox',
      label: 'Data saver (480p at most)',
      checked: st.dataSaver,
      onCheckedChange: (on) => s.setDataSaver(on),
    },
  ];

  const level = st.muted ? 0 : st.volume;
  const next = info.next;
  return (
    <div className="cpl-controls">
      <Scrubber
        currentTime={st.currentTime}
        duration={st.duration}
        bufferedEnd={st.bufferedEnd}
        markers={info.markers}
        spritesUrl={info.sprites_url}
        onSeek={(t) => s.seek(t, false)}
      />
      <div className="cpl-row">
        <IconButton label={st.playing ? 'Pause' : 'Play'} onClick={() => s.toggle()}>
          {st.playing ? <PauseGlyph /> : <PlayGlyph />}
        </IconButton>
        <IconButton label="Back 10 seconds" onClick={() => s.seekBy(-10)}>
          <Back10 />
        </IconButton>
        <IconButton label="Forward 10 seconds" onClick={() => s.seekBy(10)}>
          <Fwd10 />
        </IconButton>
        {next && (
          <IconButton label={`Next episode: ${nextLine(next)}`} onClick={p.onNext}>
            <SkipNext />
          </IconButton>
        )}
        <div className="cpl-volume">
          <IconButton label={st.muted ? 'Unmute' : 'Mute'} onClick={() => s.toggleMute()}>
            <VolumeGlyph muted={st.muted || st.volume === 0} level={level} />
          </IconButton>
          <input
            type="range"
            className="cpl-volume-input"
            aria-label="Volume"
            min={0}
            max={100}
            step={5}
            value={Math.round(level * 100)}
            aria-valuetext={`${Math.round(level * 100)} percent`}
            onChange={(e) => s.setVolume(Number(e.target.value) / 100, false)}
          />
        </div>
        <span className="cpl-time" aria-hidden="true">
          {formatClock(st.currentTime)} / {formatClock(st.duration)}
        </span>
        <span className="cpl-spacer" />
        {st.audioTracks.length > 1 && (
          <MenuButton
            label="Audio track"
            menu="Audio track"
            items={audio}
            onOpenChange={p.onMenuOpenChange}
          >
            <AudioIcon />
          </MenuButton>
        )}
        {info.subtitle_tracks.length > 0 && (
          <MenuButton
            label="Subtitles"
            menu="Subtitles"
            items={subtitles}
            onOpenChange={p.onMenuOpenChange}
          >
            <Captions />
          </MenuButton>
        )}
        <MenuButton
          label="Playback speed"
          menu="Playback speed"
          items={speed}
          onOpenChange={p.onMenuOpenChange}
        >
          <Gauge />
        </MenuButton>
        {st.levels.length > 0 && (
          <MenuButton
            label="Quality"
            menu="Quality"
            items={quality}
            onOpenChange={p.onMenuOpenChange}
          >
            <Cog />
          </MenuButton>
        )}
        {p.canRemote && (
          <IconButton label="Cast or AirPlay" onClick={p.onRemote}>
            <Cast />
          </IconButton>
        )}
        {p.canPip && (
          <IconButton label="Picture in picture" onClick={p.onPip}>
            <Pip />
          </IconButton>
        )}
        {p.canFullscreen && (
          <IconButton
            label={p.fullscreen ? 'Exit full screen' : 'Full screen'}
            onClick={p.onFullscreen}
          >
            {p.fullscreen ? <ExitFullscreen /> : <Fullscreen />}
          </IconButton>
        )}
      </div>
    </div>
  );
}
