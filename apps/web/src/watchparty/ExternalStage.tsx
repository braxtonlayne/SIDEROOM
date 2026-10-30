// The theatre's stage for media outside the Cinema catalog (PRF-UNF-001): a provider's privacy-enhanced iframe or a
// direct file in the browser's own player. Nothing third-party is requested until the viewer allows it (a click, or
// the remembered "Load embeds" setting); then the frame or element is created and registered with the external
// host, and the sync controller drives it like the Cinema player.
import { Button } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { useStore as useZustand } from 'zustand';
import { useT } from '../i18n';
import { setLoadEmbeds, useLoadEmbeds } from '../lounge/msg/EmbedsPrefs';
import { externalHost } from './externalHost';
import { FramePlayer, MediaElementPlayer, frameOrigin } from './externalPlayers';
import { providerOfSrc, syncedFrameSrc, type FrameProvider } from './frameProtocol';
import { partyController } from './runtime';
import type { PartyMedia } from './types';

const hostOf = (url: string | null | undefined): string => {
  try {
    return new URL(url ?? '').host;
  } catch {
    return '';
  }
};

function ProviderFrame({ media, provider }: { media: PartyMedia; provider: FrameProvider }) {
  const t = useT();
  const ref = useRef<HTMLIFrameElement>(null);
  const src = syncedFrameSrc(provider, media.url ?? '', window.location.origin);

  useEffect(() => {
    const frame = ref.current;
    if (!frame) return;
    const origin = frameOrigin(provider);
    const host = externalHost();
    const player = new FramePlayer(provider, {
      post: (m) => frame.contentWindow?.postMessage(m, origin),
      now: () => Date.now(),
      sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
      ready: () => void partyController().reapply(),
    });
    const onMessage = (ev: MessageEvent) => {
      // Only this frame, only its provider's origin.
      if (ev.source !== frame.contentWindow || ev.origin !== origin) return;
      player.onMessage(ev.data);
    };
    window.addEventListener('message', onMessage);
    host.attach(player);
    // The provider answers once it listens: say hello after the load, and again a few times in case it was early.
    let tries = 0;
    const hello = () => {
      player.onLoad();
      if (++tries >= 20) clearInterval(poll);
    };
    const poll = setInterval(hello, 500);
    frame.addEventListener('load', hello);
    return () => {
      clearInterval(poll);
      frame.removeEventListener('load', hello);
      window.removeEventListener('message', onMessage);
      host.detach(player);
    };
  }, [provider, src]);

  return (
    <iframe
      ref={ref}
      className="wp-ext-frame"
      title={t('watchparty.external.frameTitle', { title: media.title })}
      src={src}
      allow="autoplay; fullscreen; picture-in-picture; encrypted-media"
      sandbox="allow-scripts allow-same-origin allow-presentation"
      referrerPolicy="strict-origin-when-cross-origin"
    />
  );
}

function DirectMedia({ media }: { media: PartyMedia }) {
  const t = useT();
  const ref = useRef<HTMLVideoElement>(null);
  const [failed, setFailed] = useState(false);
  const audio = /\.(mp3|ogg|oga|opus|m4a|aac|wav|flac)(\?|$)/i.test(media.url ?? '');

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const host = externalHost();
    const player = new MediaElementPlayer(el, () => void partyController().reapply());
    host.attach(player);
    return () => host.detach(player);
  }, [media.url]);

  return (
    <>
      {/* No controls and no download item: the party bar plays it. The address stays what it is (not DRM). */}
      <video
        ref={ref}
        className={audio ? 'wp-ext-audio' : 'wp-ext-frame'}
        src={media.url ?? undefined}
        preload="auto"
        playsInline
        disablePictureInPicture
        controlsList="nodownload noremoteplayback"
        aria-label={media.title}
        onContextMenu={(e) => e.preventDefault()}
        onError={() => setFailed(true)}
      />
      {audio && <p className="wp-ext-title">{media.title}</p>}
      {failed && (
        <p className="wp-ext-error" role="alert">
          {t('watchparty.external.failed')}
        </p>
      )}
    </>
  );
}

export function ExternalStage() {
  const t = useT();
  const host = externalHost();
  const media = useZustand(host.store, (s) => s.media);
  const allowed = useZustand(host.store, (s) => s.allowed);
  const pref = useLoadEmbeds();
  const [remember, setRemember] = useState(false);

  // The remembered setting is the standing click.
  useEffect(() => {
    if (media && pref === 'always') host.allow();
  }, [media, pref, host]);

  if (!media || (media.kind !== 'embed' && media.kind !== 'direct')) return null;
  const provider = media.kind === 'embed' ? providerOfSrc(media.url) : null;
  if (media.kind === 'embed' && !provider) {
    return (
      <div className="wp-ext-gate" role="alert">
        <p>{t('watchparty.external.unsupported')}</p>
      </div>
    );
  }
  if (!allowed) {
    const from = hostOf(media.url);
    return (
      <div className="wp-ext-gate">
        <h2 className="wp-ext-gate-title">{media.title}</h2>
        <p>
          {media.kind === 'embed'
            ? t('watchparty.external.gateEmbed', { host: from })
            : t('watchparty.external.gateDirect', { host: from })}
        </p>
        <Button
          onClick={() => {
            if (remember) void setLoadEmbeds('always').catch(() => undefined);
            host.allow();
          }}
        >
          {t('watchparty.external.load')}
        </Button>
        <label className="wp-ext-remember">
          <input
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
          />{' '}
          {t('watchparty.external.remember')}
        </label>
      </div>
    );
  }
  return provider ? (
    <ProviderFrame media={media} provider={provider} />
  ) : (
    <DirectMedia media={media} />
  );
}
