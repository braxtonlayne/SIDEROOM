// "Play here" on a link card (PRF-UNF-001). The card itself renders with nothing from a third party: no frame, no
// media request. The reader's click (or the remembered "Load embeds" setting) creates the provider's
// privacy-enhanced iframe (youtube-nocookie.com, Vimeo with dnt=1) or the browser's own player for a direct file.
// "Watch together" starts a party with the same link, driven through the provider frame's own message API.
import { Button } from '@sideroom/ui';
import { lazy, Suspense, useState } from 'react';
import { useT } from '../../i18n';
import type { Embed } from '../data';
import { cardEmbedSrc, isCardEmbedSrc, providerOfSrc } from '../../watchparty/frameProtocol';
import { setLoadEmbeds, useLoadEmbeds } from './EmbedsPrefs';

const StartPartyDialog = lazy(() =>
  import('../../watchparty/StartPartyDialog').then((m) => ({ default: m.StartPartyDialog })),
);

type Play = NonNullable<Embed['sr_play']>;

/** The offer, or `null` when the server's address is not one this client will ever load. */
export function usablePlay(play: Embed['sr_play']): Play | null {
  if (!play || typeof play.src !== 'string') return null;
  if (play.mode === 'embed') return isCardEmbedSrc(play.src) ? play : null;
  if (play.mode === 'direct') {
    try {
      const u = new URL(play.src);
      return u.protocol === 'https:' && !u.username && !u.password ? play : null;
    } catch {
      return null;
    }
  }
  return null;
}

/** The host a click would contact, for the notice next to the button. */
export function playHost(play: Play): string {
  try {
    return new URL(play.src).host;
  } catch {
    return '';
  }
}

/** Whether the browser can play an HLS playlist itself (Safari). Other browsers would need a script and connect-src. */
function nativeHls(): boolean {
  if (typeof document === 'undefined') return false;
  return document.createElement('video').canPlayType('application/vnd.apple.mpegurl') !== '';
}

function DirectPlayer({ play, title }: { play: Play; title: string }) {
  const t = useT();
  if (play.media === 'hls' && !nativeHls()) {
    return (
      <p className="lng-muted" role="status">
        {t('lounge.embedPlay.hlsUnsupported')}
      </p>
    );
  }
  // No download control in the UI. The address is still the address (a UI promise, not DRM).
  const common = {
    className: 'lng-embed-media',
    src: play.src,
    controls: true,
    controlsList: 'nodownload noremoteplayback',
    preload: 'metadata' as const,
    'aria-label': title,
    onContextMenu: (e: React.MouseEvent) => e.preventDefault(),
  };
  return play.media === 'audio' ? (
    <audio {...common} />
  ) : (
    <video {...common} playsInline disablePictureInPicture />
  );
}

export function PlayHere({
  play,
  title,
  channelId,
}: {
  play: Play;
  title: string;
  channelId?: string | undefined;
}) {
  const t = useT();
  const pref = useLoadEmbeds();
  const [clicked, setClicked] = useState(false);
  const [remember, setRemember] = useState(false);
  const [party, setParty] = useState(false);
  const loaded = clicked || pref === 'always';
  const host = playHost(play);
  const provider =
    play.mode === 'embed'
      ? play.provider === 'youtube'
        ? 'YouTube'
        : play.provider === 'vimeo'
          ? 'Vimeo'
          : host
      : host;
  // Only the two providers whose message API the party drives can be watched together.
  const canParty =
    play.sync === true && (play.mode === 'direct' || providerOfSrc(play.src) !== null);

  return (
    <div className="lng-embed-play">
      {loaded ? (
        play.mode === 'embed' ? (
          <iframe
            className="lng-embed-frame"
            title={title}
            src={cardEmbedSrc(play.src)}
            allow="autoplay; fullscreen; picture-in-picture; encrypted-media"
            sandbox="allow-scripts allow-same-origin allow-presentation allow-popups"
            referrerPolicy="strict-origin-when-cross-origin"
            allowFullScreen
          />
        ) : (
          <DirectPlayer play={play} title={title} />
        )
      ) : (
        <div className="lng-embed-gate">
          <p className="lng-muted">
            {play.mode === 'embed'
              ? t('lounge.embedPlay.embedNotice', { provider, host })
              : t('lounge.embedPlay.directNotice', { host })}
          </p>
          <div className="lng-embed-play-row">
            <Button
              size="sm"
              onClick={() => {
                setClicked(true);
                if (remember) void setLoadEmbeds('always').catch(() => undefined);
              }}
            >
              {t('lounge.embedPlay.playHere')}
            </Button>
            {canParty && (
              <Button size="sm" variant="ghost" onClick={() => setParty(true)}>
                {t('lounge.embedPlay.watchTogether')}
              </Button>
            )}
          </div>
          <label className="lng-embed-remember">
            <input
              type="checkbox"
              checked={remember}
              onChange={(e) => setRemember(e.target.checked)}
            />{' '}
            {t('lounge.embedPlay.remember')}
          </label>
        </div>
      )}
      {loaded && canParty && (
        <Button size="sm" variant="ghost" onClick={() => setParty(true)}>
          {t('lounge.embedPlay.watchTogether')}
        </Button>
      )}
      {party && (
        <Suspense fallback={null}>
          <StartPartyDialog
            open
            onClose={() => setParty(false)}
            external={{ url: play.src, label: title }}
            channelId={channelId ?? null}
          />
        </Suspense>
      )}
    </div>
  );
}
