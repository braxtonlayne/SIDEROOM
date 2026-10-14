// The now-playing panel (TV-UI-002): CRT preview frame, channel eyebrow, programme, actions, reception dot.
import { Suspense, lazy, useState } from 'react';
import { useT } from '../../i18n';
import { kindYear, watchChannelHref } from './model';
import { useBlockInfo } from './api';
import type { ChannelNow, TvBlock, TvChannel } from './types';

const LivePreview = lazy(() => import('./LivePreview').then((m) => ({ default: m.LivePreview })));

export interface NowPanelProps {
  channel: TvChannel;
  now: TvBlock | null;
  next: TvBlock | null;
  /** Reception: true good, false no signal, null unknown (still asking). */
  healthy: ChannelNow['healthy'] | null;
  canManage: boolean;
  onDetails: (b: TvBlock) => void;
}

export function NowPanel({
  channel,
  now: nowBlock,
  next,
  healthy,
  canManage,
  onDetails,
}: NowPanelProps) {
  const t = useT();
  // Synopsis and artwork come from the catalog (the schedule block carries neither).
  const now = useBlockInfo(nowBlock);
  const [hover, setHover] = useState(false);
  const [preview, setPreview] = useState<'idle' | 'live' | 'error'>('idle');
  // A restricted channel needs the age gate before any picture: no preview, only Tune in.
  const canPreview = !channel.age_restricted && healthy !== false;
  const showVideo = hover && canPreview;
  const picture = now?.backdrop_url ?? now?.poster_url ?? null;
  const reception = healthy === null ? null : healthy && preview !== 'error';
  return (
    <section className="tv-now" aria-label={t('cinema.nowPanel.onNow', { n: channel.number })}>
      <div
        className="tv-crt"
        data-live={preview === 'live' || undefined}
        onMouseEnter={() => setHover(true)}
        onMouseLeave={() => {
          setHover(false);
          setPreview('idle');
        }}
        onFocus={() => setHover(true)}
        onBlur={() => {
          setHover(false);
          setPreview('idle');
        }}
      >
        <div className="tv-crt-screen">
          {picture && <img className="tv-crt-img" src={picture} alt="" />}
          {showVideo && (
            <Suspense fallback={null}>
              <LivePreview key={channel.id} channelId={channel.id} onState={(s) => setPreview(s)} />
            </Suspense>
          )}
          <span className="tv-crt-num" aria-hidden="true">
            {channel.number}
          </span>
          <span className="tv-crt-bug" aria-hidden="true">
            {channel.logo_url ? <img src={channel.logo_url} alt="" /> : channel.call_sign}
          </span>
          <span className="tv-crt-label" aria-hidden="true">
            {t('cinema.nowPanel.preview')}
          </span>
          <span className="tv-scan" aria-hidden="true" />
        </div>
      </div>
      <div className="tv-now-body">
        <p className="tv-eyebrow">
          <span aria-hidden="true" className="tv-rule" />
          {channel.name}
          {channel.era_label ? ` · ${channel.era_label}` : ''}
        </p>
        {reception !== null && (
          <p className="tv-reception" data-ok={reception || undefined} role="status">
            <span aria-hidden="true">●</span>{' '}
            {reception ? t('cinema.nowPanel.goodReception') : t('cinema.nowPanel.noSignal')}
          </p>
        )}
        {now ? (
          <>
            <h2 className="tv-now-title">{now.title}</h2>
            <p className="tv-now-meta">
              {[now.year, kindYear({ kind: now.kind, year: null })].filter(Boolean).join('  ')}
              {now.episode_label ? ` · ${now.episode_label}` : ''}
            </p>
            <p className="tv-now-syn">{now.synopsis ?? ''}</p>
          </>
        ) : (
          <>
            <h2 className="tv-now-title">{t('cinema.nowPanel.offAir')}</h2>
            <p className="tv-now-syn">
              {next
                ? t('cinema.nowPanel.offAirNext', { title: next.title })
                : t('cinema.nowPanel.offAirBody')}
            </p>
          </>
        )}
        <div className="tv-now-actions">
          {now && (
            <button type="button" className="tv-btn" onClick={() => onDetails(now)}>
              <span aria-hidden="true">ⓘ</span> {t('cinema.nowPanel.details')}
            </button>
          )}
          {now && (
            <a className="tv-btn tv-btn-primary" href={watchChannelHref(channel.id)}>
              <span aria-hidden="true">▶</span> {t('cinema.nowPanel.tuneIn')}
            </a>
          )}
          {canManage && (
            <a className="tv-btn" href="/admin/cinema-live-tv">
              {t('cinema.nowPanel.editLineup')}
            </a>
          )}
        </div>
      </div>
    </section>
  );
}
