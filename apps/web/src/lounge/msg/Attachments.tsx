import { Icon, Modal, Tooltip, type IconName } from '@sideroom/ui';
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from 'react';
import type { Attachment } from '../data';
import { useT } from '../../i18n';
import { isGif, useGifAutoplay } from '../../settings/user/gifAutoplay';
import { useSavedPrefs } from '../../settings/user/prefs';
import { decodeBlurhash } from './blurhash';
import { formatBytes, formatDuration, isHttpUrl } from './format';
import { useAnimatedSrc } from './staticFrame';

// UTL-IMG-001: "Open in studio" in the media viewer; the studio edits a copy and loads only when opened.
const StudioModal = lazy(() => import('../../tools/studio/StudioModal'));

const isImage = (a: Attachment) => a.contentType.startsWith('image/');
const isVideo = (a: Attachment) => a.contentType.startsWith('video/');
const isAudio = (a: Attachment) => a.contentType.startsWith('audio/');
const isText = (a: Attachment) =>
  a.contentType.startsWith('text/') || /\.(txt|md|log|json|csv)$/i.test(a.filename);

/** Icon for a file card, chosen from its type. */
export function fileIcon(a: Pick<Attachment, 'contentType' | 'filename'>): IconName {
  const t = a.contentType;
  if (t.startsWith('image/')) return 'image';
  if (t.startsWith('video/')) return 'film';
  if (t.startsWith('audio/')) return 'headphones';
  if (t === 'application/pdf' || t.startsWith('text/')) return 'book';
  return 'download';
}

export interface AttachmentListProps {
  attachments: Attachment[];
  /** Voice message: the single audio attachment renders as a waveform player. */
  voice?: boolean;
}

/** All attachments of a message: media mosaic first, then players and file cards. */
export function AttachmentList({ attachments, voice }: AttachmentListProps) {
  const [lightbox, setLightbox] = useState<number | null>(null);
  // "Display attachments directly in chat" off: pictures and videos become file cards (voice messages stay).
  const inline = useSavedPrefs().inlineAttachments;
  const media = attachments.filter(
    (a) => inline && !a.expired && !a.filtered && (isImage(a) || isVideo(a)),
  );
  const mosaic = media.filter(isImage);
  const rest = attachments.filter((a) => !mosaic.includes(a));
  return (
    <div className="lng-attachments">
      {mosaic.length > 0 && (
        <div className="lng-mosaic" data-count={Math.min(mosaic.length, 10)}>
          {mosaic.slice(0, 10).map((a, i) => (
            <MosaicTile key={a.id} a={a} onOpen={() => setLightbox(i)} />
          ))}
        </div>
      )}
      {rest.map((a) => {
        if (a.filtered) return <FilteredChip key={a.id} a={a} />;
        if (a.expired) return <ExpiredChip key={a.id} a={a} />;
        if (isVideo(a) && inline) return <VideoPlayer key={a.id} a={a} />;
        if (isAudio(a) && (voice || a.waveform)) return <VoiceMessage key={a.id} a={a} />;
        if (isAudio(a)) return <AudioPlayer key={a.id} a={a} />;
        return <FileCard key={a.id} a={a} />;
      })}
      {lightbox !== null && (
        <Lightbox
          items={mosaic}
          index={lightbox}
          onIndex={setLightbox}
          onClose={() => setLightbox(null)}
        />
      )}
    </div>
  );
}

/** The BlurHash of an image drawn small and stretched behind it until the real picture arrives. */
export function BlurhashPlaceholder({ hash }: { hash: string }) {
  const ref = useRef<HTMLCanvasElement | null>(null);
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    try {
      const ctx = canvas.getContext('2d');
      if (!ctx) return;
      const pixels = decodeBlurhash(hash, 32, 32);
      ctx.putImageData(new ImageData(pixels, 32, 32), 0, 0);
    } catch {
      /* a bad hash or no canvas support: the tile simply has no placeholder */
    }
  }, [hash]);
  return <canvas ref={ref} className="lng-blurhash" width={32} height={32} aria-hidden="true" />;
}

function MosaicTile({ a, onOpen }: { a: Attachment; onOpen: () => void }) {
  const t = useT();
  const [revealed, setRevealed] = useState(!a.spoiler);
  const [loaded, setLoaded] = useState(false);
  const hidden = !revealed;
  // RDY-PAR-006: a GIF plays on its own only when autoplay is on, the client is focused and motion is not reduced.
  const gif = isGif(a.contentType, a.filename);
  const autoplay = useGifAutoplay();
  const [pointed, setPointed] = useState(false);
  const src = useAnimatedSrc(gif ? a.url : null, autoplay ? 'always' : 'hover', pointed) ?? a.url;
  return (
    <div className="lng-tile" data-spoiler={hidden || undefined}>
      <button
        type="button"
        className="lng-tile-btn"
        aria-label={
          hidden
            ? t('lounge.attachments.revealSpoiler')
            : t('lounge.attachments.openImage', { name: a.filename })
        }
        onClick={() => (hidden ? setRevealed(true) : onOpen())}
        onMouseEnter={() => setPointed(true)}
        onMouseLeave={() => setPointed(false)}
        onFocus={() => setPointed(true)}
        onBlur={() => setPointed(false)}
      >
        {a.blurhash && !loaded && <BlurhashPlaceholder hash={a.blurhash} />}
        <img
          src={src}
          alt={hidden ? '' : (a.description ?? a.filename)}
          width={a.width}
          height={a.height}
          loading="lazy"
          onLoad={() => setLoaded(true)}
          style={a.width && a.height ? { aspectRatio: `${a.width} / ${a.height}` } : undefined}
        />
        {hidden && <span className="lng-spoiler-chip">{t('lounge.attachments.spoilerChip')}</span>}
      </button>
      {a.description && !hidden && (
        <Tooltip content={a.description}>
          <span
            className="lng-alt-chip"
            tabIndex={0}
            aria-label={t('lounge.attachments.altTextLabel', { text: a.description })}
          >
            {t('lounge.attachments.altChip')}
          </span>
        </Tooltip>
      )}
    </div>
  );
}

/** A file the viewer's own safety settings hid (D-230); it is not deleted. */
export function FilteredChip({ a }: { a: Attachment }) {
  const t = useT();
  return (
    <div className="lng-attach-file lng-expired" role="note">
      <Icon name="eye-off" size={20} />
      <span>
        <strong>{a.filename}</strong>
        <br />
        <span className="lng-muted">{t('lounge.attachments.filtered')}</span>
      </span>
    </div>
  );
}

export function ExpiredChip({ a }: { a: Attachment }) {
  const t = useT();
  return (
    <div className="lng-attach-file lng-expired" role="note">
      <Icon name="clock" size={20} />
      <span>
        <strong>{a.filename}</strong>
        <br />
        <span className="lng-muted">{t('lounge.attachments.expired')}</span>
      </span>
    </div>
  );
}

const TEXT_PREVIEW_MAX = 64 * 1024;

export function FileCard({ a }: { a: Attachment }) {
  const t = useT();
  const [preview, setPreview] = useState<string | null>(null);
  const [state, setState] = useState<'idle' | 'loading' | 'error'>('idle');
  const canPreview = isText(a) && a.size <= TEXT_PREVIEW_MAX;
  const togglePreview = async () => {
    if (preview !== null) return setPreview(null);
    setState('loading');
    try {
      const res = await fetch(a.url);
      if (!res.ok) throw new Error(String(res.status));
      setPreview((await res.text()).slice(0, 4000));
      setState('idle');
    } catch {
      setState('error');
    }
  };
  return (
    <div className="lng-file-wrap">
      <div className="lng-attach-file">
        <Icon name={fileIcon(a)} size={24} />
        <span className="lng-file-meta">
          <a href={a.url} download={a.filename} className="lng-file-name">
            {a.filename}
          </a>
          <span className="lng-muted">{formatBytes(a.size)}</span>
        </span>
        {canPreview && (
          <button
            type="button"
            className="lng-link-btn"
            aria-expanded={preview !== null}
            onClick={() => void togglePreview()}
          >
            {preview !== null
              ? t('lounge.attachments.hidePreview')
              : t('lounge.attachments.preview')}
          </button>
        )}
        <a
          className="lng-icon-btn"
          href={a.url}
          download={a.filename}
          aria-label={t('lounge.attachments.downloadFile', { name: a.filename })}
        >
          <Icon name="download" size={20} />
        </a>
      </div>
      {state === 'loading' && <p className="lng-muted">{t('lounge.attachments.loadingPreview')}</p>}
      {state === 'error' && <p className="lng-failed">{t('lounge.attachments.previewError')}</p>}
      {preview !== null && (
        <pre
          className="lng-file-preview"
          tabIndex={0}
          aria-label={t('lounge.attachments.previewOf', { name: a.filename })}
        >
          {preview}
        </pre>
      )}
    </div>
  );
}

export function AudioPlayer({ a }: { a: Attachment }) {
  const t = useT();
  return (
    <div className="lng-attach-file lng-media-card">
      <span className="lng-file-name">{a.filename}</span>
      <audio
        controls
        preload="none"
        src={a.url}
        aria-label={t('lounge.attachments.audio', { name: a.filename })}
      />
    </div>
  );
}

export function VideoPlayer({ a }: { a: Attachment }) {
  const t = useT();
  return (
    <div className="lng-video">
      <video
        controls
        preload="metadata"
        src={a.url}
        aria-label={a.description ?? t('lounge.attachments.video', { name: a.filename })}
        width={a.width}
        height={a.height}
      />
    </div>
  );
}

const SPEEDS = [1, 1.5, 2] as const;
const BARS = 48;

/** Resample amplitude bytes to a fixed bar count (0..1). */
export function waveformBars(wave: number[] | null | undefined, n = BARS): number[] {
  if (!wave || wave.length === 0) return Array.from({ length: n }, () => 0.3);
  return Array.from({ length: n }, (_, i) => {
    const from = Math.floor((i * wave.length) / n);
    const to = Math.max(from + 1, Math.floor(((i + 1) * wave.length) / n));
    let max = 0;
    for (let j = from; j < to; j++) max = Math.max(max, wave[j] ?? 0);
    return Math.max(0.08, max / 255);
  });
}

/** Voice message: waveform, play/pause, playback speed (LNG-MSGUI-014). */
export function VoiceMessage({ a }: { a: Attachment }) {
  const t = useT();
  const audio = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [pos, setPos] = useState(0);
  const [speed, setSpeed] = useState<(typeof SPEEDS)[number]>(1);
  const total = a.durationSecs ?? 0;
  const bars = waveformBars(a.waveform);

  const toggle = useCallback(() => {
    const el = audio.current;
    if (!el) return;
    if (el.paused) void el.play().catch(() => setPlaying(false));
    else el.pause();
  }, []);

  useEffect(() => {
    if (audio.current) audio.current.playbackRate = speed;
  }, [speed]);

  const progress = total > 0 ? Math.min(1, pos / total) : 0;
  return (
    <div className="lng-voice" role="group" aria-label={t('lounge.attachments.voiceMessage')}>
      <audio
        ref={audio}
        src={a.url}
        preload="none"
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => {
          setPlaying(false);
          setPos(0);
        }}
        onTimeUpdate={(e) => setPos(e.currentTarget.currentTime)}
      />
      <button
        type="button"
        className="lng-voice-play"
        aria-label={
          playing ? t('lounge.attachments.pauseVoice') : t('lounge.attachments.playVoice')
        }
        onClick={toggle}
      >
        <Icon name={playing ? 'pause' : 'play'} size={20} />
      </button>
      <div
        className="lng-wave"
        role="progressbar"
        aria-label={t('lounge.attachments.playbackPosition')}
        aria-valuemin={0}
        aria-valuemax={Math.round(total)}
        aria-valuenow={Math.round(pos)}
      >
        {bars.map((h, i) => (
          <span
            key={i}
            className="lng-wave-bar"
            data-played={i / bars.length < progress || undefined}
            style={{ height: `${Math.round(h * 100)}%` }}
          />
        ))}
      </div>
      <span className="lng-voice-time">{formatDuration(playing || pos > 0 ? pos : total)}</span>
      <button
        type="button"
        className="lng-voice-speed"
        aria-label={t('lounge.attachments.playbackSpeed', { speed })}
        onClick={() => setSpeed((s) => SPEEDS[(SPEEDS.indexOf(s) + 1) % SPEEDS.length]!)}
      >
        {speed}x
      </button>
    </div>
  );
}

/** Media viewer (LNG-MSGUI-010): zoom, prev/next within the message, download, open original, copy link. */
export function Lightbox({
  items,
  index,
  onIndex,
  onClose,
}: {
  items: Attachment[];
  index: number;
  onIndex: (i: number) => void;
  onClose: () => void;
}) {
  const t = useT();
  const [zoom, setZoom] = useState(false);
  const [copied, setCopied] = useState(false);
  const [studio, setStudio] = useState(false);
  const a = items[index];
  useEffect(() => setZoom(false), [index]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'ArrowRight' && index < items.length - 1) onIndex(index + 1);
      if (e.key === 'ArrowLeft' && index > 0) onIndex(index - 1);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [index, items.length, onIndex]);
  if (!a) return null;
  const abs = isHttpUrl(a.url) ? a.url : new URL(a.url, window.location.origin).toString();
  return (
    <Modal open onClose={onClose} title={a.filename} size="lg" className="lng-lightbox">
      <div className="lng-lightbox-stage" data-zoom={zoom || undefined}>
        <img
          src={a.url}
          alt={a.description ?? a.filename}
          onClick={() => setZoom((z) => !z)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') setZoom((z) => !z);
          }}
          tabIndex={0}
          role="button"
          aria-pressed={zoom}
          aria-label={zoom ? t('lounge.attachments.zoomOut') : t('lounge.attachments.zoomIn')}
        />
      </div>
      <div className="lng-lightbox-bar">
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.attachments.previousImage')}
          disabled={index === 0}
          onClick={() => onIndex(index - 1)}
        >
          <Icon name="chevron-left" size={20} />
        </button>
        <span aria-live="polite">
          {index + 1} / {items.length}
        </span>
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.attachments.nextImage')}
          disabled={index >= items.length - 1}
          onClick={() => onIndex(index + 1)}
        >
          <Icon name="chevron-right" size={20} />
        </button>
        <span className="lng-spacer" />
        <a className="lng-link-btn" href={a.url} download={a.filename}>
          {t('lounge.attachments.download')}
        </a>
        <a className="lng-link-btn" href={abs} target="_blank" rel="noreferrer noopener">
          {t('lounge.attachments.openOriginal')}
        </a>
        <button
          type="button"
          className="lng-link-btn"
          onClick={() => {
            void navigator.clipboard?.writeText(abs);
            setCopied(true);
          }}
        >
          {copied ? t('lounge.attachments.copied') : t('lounge.attachments.copyLink')}
        </button>
        {isImage(a) && !/^image\/(gif|svg)/.test(a.contentType) && (
          <button
            type="button"
            className="lng-link-btn"
            data-testid="open-in-studio"
            onClick={() => setStudio(true)}
          >
            {t('studio.open.button')}
          </button>
        )}
      </div>
      {studio && (
        <Suspense fallback={null}>
          <StudioModal
            mode="copy"
            sourceUrl={a.url}
            name={a.filename.replace(/\.[^.]+$/, '')}
            onClose={() => setStudio(false)}
            onUse={(file) => {
              const url = URL.createObjectURL(file);
              const link = document.createElement('a');
              link.href = url;
              link.download = file.name;
              link.click();
              setTimeout(() => URL.revokeObjectURL(url), 2000);
            }}
          />
        </Suspense>
      )}
    </Modal>
  );
}
