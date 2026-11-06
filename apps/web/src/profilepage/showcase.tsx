// The media showcase of a profile page in the app (PRF-PRO-001, D-502, D-503): the items the server resolved for
// this viewer (title, length, caption), each with a Play button. The address is minted on the click, never on
// render, and the media element plays inline with no download control (a UI promise, not DRM). One item plays at a
// time. The public page never fills this slot, so none of it reaches a visitor without an account.
import { Button } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../i18n';
import { showcaseUrl } from './api';
import type { Block, ProfilePage, ShowcaseView } from './types';

const text = (c: Record<string, unknown>, k: string): string => {
  const v = c[k];
  return typeof v === 'string' ? v : '';
};

/** `m:ss` or `h:mm:ss`. */
export function fmtLength(ms: number | null | undefined): string {
  if (typeof ms !== 'number' || ms <= 0) return '';
  const s = Math.round(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

function Media({ item, src }: { item: ShowcaseView; src: string }) {
  const common = {
    className: 'pp-showcase-media',
    src,
    controls: true,
    autoPlay: true,
    controlsList: 'nodownload noremoteplayback',
    preload: 'metadata' as const,
    'aria-label': item.title,
    onContextMenu: (e: React.MouseEvent) => e.preventDefault(),
  };
  return item.media === 'audio' ? (
    <audio {...common} />
  ) : (
    <video {...common} playsInline disablePictureInPicture />
  );
}

export function Showcase({ page, block }: { page: ProfilePage; block: Block }) {
  const t = useT();
  const items = page.showcase ?? [];
  const [playing, setPlaying] = useState<{ hash: string; src: string } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  const play = async (it: ShowcaseView) => {
    setFailed(null);
    setBusy(it.object_hash);
    try {
      const { url } = await showcaseUrl(page.user.username, it.object_hash);
      setPlaying({ hash: it.object_hash, src: url });
    } catch {
      setPlaying(null);
      setFailed(it.object_hash);
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      <h3 className="pp-title">{text(block.config, 'title') || t('profile.showcase.title')}</h3>
      {items.length === 0 ? (
        <p className="pp-muted">{t('profile.showcase.empty')}</p>
      ) : (
        <ul className="pp-showcase" aria-label={t('profile.showcase.title')}>
          {items.map((it) => {
            const on = playing?.hash === it.object_hash;
            const length = fmtLength(it.duration_ms);
            return (
              <li key={it.object_hash}>
                <div className="pp-showcase-row">
                  <div className="pp-showcase-text">
                    <strong className="pp-showcase-name">{it.title}</strong>
                    <span className="pp-muted">
                      {t(
                        it.media === 'audio' ? 'profile.showcase.audio' : 'profile.showcase.video',
                      )}
                      {length ? ` · ${length}` : ''}
                    </span>
                    {it.caption && <span className="pp-text">{it.caption}</span>}
                  </div>
                  <Button
                    size="sm"
                    variant={on ? 'ghost' : 'secondary'}
                    loading={busy === it.object_hash}
                    aria-label={
                      on
                        ? t('profile.showcase.stop', { title: it.title })
                        : t('profile.showcase.play', { title: it.title })
                    }
                    onClick={() => (on ? setPlaying(null) : void play(it))}
                  >
                    {on ? t('profile.showcase.stopShort') : t('profile.showcase.playShort')}
                  </Button>
                </div>
                {failed === it.object_hash && (
                  <p role="alert" className="pp-error">
                    {t('profile.showcase.failed')}
                  </p>
                )}
                {on && playing && <Media item={it} src={playing.src} />}
              </li>
            );
          })}
        </ul>
      )}
    </>
  );
}
