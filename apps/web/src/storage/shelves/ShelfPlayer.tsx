import { Notice, Spinner } from '@sideroom/ui';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { presignShelfFile } from './api';
import './shelves.css';

/** Ask for a new link this long before the old one stops working. */
const REFRESH_BEFORE_MS = 60_000;

/**
 * Plays a web-native file from a mount (BYOS-CAT-002): the browser streams it straight from the member's bucket
 * through a presigned URL, with range requests (seeking works without the server in the middle). The URL is held in
 * state only, never stored; a minute before it expires, or when the media element errors, the player asks for a
 * new one and resumes at the same position.
 */
export function ShelfPlayer({
  mountId,
  objectKey,
  kind,
  title,
}: {
  mountId: string;
  objectKey: string;
  kind: 'video' | 'audio';
  title: string;
}) {
  const t = useT();
  const el = useRef<HTMLVideoElement & HTMLAudioElement>(null);
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const resume = useRef<{ at: number; playing: boolean } | null>(null);

  const load = useCallback(
    async (live: () => boolean) => {
      try {
        const p = await presignShelfFile(mountId, objectKey);
        if (!live()) return;
        const media = el.current;
        if (media && !media.paused) resume.current = { at: media.currentTime, playing: true };
        else if (media && media.currentTime > 0)
          resume.current = { at: media.currentTime, playing: false };
        setUrl(p.url);
        setError(null);
        const wait = Date.parse(p.expires_at) - Date.now() - REFRESH_BEFORE_MS;
        window.clearTimeout(timer.current);
        timer.current = window.setTimeout(
          () => void load(live),
          Number.isFinite(wait) ? Math.max(wait, 5_000) : 600_000,
        );
      } catch (e) {
        if (!live()) return;
        const code = e instanceof ApiError ? String(e.body?.code ?? '') : '';
        setError(code === 'CORS_NOT_READY' ? t('shelves.cors') : t('shelves.playFailed'));
      }
    },
    [mountId, objectKey, t],
  );

  useEffect(() => {
    let alive = true;
    void load(() => alive);
    return () => {
      alive = false;
      window.clearTimeout(timer.current);
    };
  }, [load]);

  const restore = () => {
    const media = el.current;
    const r = resume.current;
    if (!media || !r) return;
    resume.current = null;
    media.currentTime = r.at;
    if (r.playing) void media.play().catch(() => undefined);
  };

  if (error) return <Notice variant="danger">{error}</Notice>;
  if (!url) return <Spinner label={t('shelves.loading')} />;
  const common = {
    ref: el,
    src: url,
    controls: true,
    preload: 'metadata' as const,
    className: 'shv-player',
    'aria-label': title,
    onLoadedMetadata: restore,
    // An expired or dropped link: ask again once, the effect's timer handles the normal case.
    onError: () => {
      void load(() => true);
    },
  };
  return kind === 'video' ? <video {...common} playsInline /> : <audio {...common} />;
}
