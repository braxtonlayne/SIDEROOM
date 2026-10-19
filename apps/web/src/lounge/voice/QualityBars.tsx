import { t } from '../../i18n';
import {
  barsFor,
  QUALITY_LABEL,
  type ConnectionQuality,
  type RemoteQuality,
} from '../../voice/quality';

/** Three signal bars for the connection quality (VOX-UI-001/003). */
export function QualityBars({ quality }: { quality: ConnectionQuality }) {
  const lit = barsFor(quality.level);
  const label =
    quality.pingMs !== null
      ? t('lounge.qualityBars.withPing', {
          label: QUALITY_LABEL[quality.level],
          ms: quality.pingMs,
        })
      : QUALITY_LABEL[quality.level];
  return (
    <span className="vc-bars" role="img" aria-label={label} data-level={quality.level}>
      {[1, 2, 3].map((n) => (
        <span
          key={n}
          className="vc-bar"
          data-on={n <= lit || undefined}
          style={{ height: 4 + n * 3 }}
        />
      ))}
    </span>
  );
}

/**
 * The same bars for someone else's microphone line (VOX-UI-003): packet loss, jitter and received bitrate from
 * the receiver's stats, spelled out in the label for screen readers and the tooltip.
 */
export function RemoteQualityBars({ quality }: { quality: RemoteQuality }) {
  const lit = barsFor(quality.level);
  const parts = [
    quality.lossPct !== null && t('lounge.qualityBars.loss', { pct: quality.lossPct }),
    quality.jitterMs !== null && t('lounge.qualityBars.jitter', { ms: quality.jitterMs }),
    quality.kbps !== null && t('lounge.qualityBars.bitrate', { kbps: quality.kbps }),
  ].filter((p): p is string => typeof p === 'string');
  const label =
    parts.length > 0
      ? t('lounge.qualityBars.remote', {
          label: QUALITY_LABEL[quality.level],
          detail: parts.join(', '),
        })
      : QUALITY_LABEL[quality.level];
  return (
    <span
      className="vc-bars"
      role="img"
      aria-label={label}
      title={label}
      data-level={quality.level}
      data-testid="remote-quality"
    >
      {[1, 2, 3].map((n) => (
        <span
          key={n}
          className="vc-bar"
          data-on={n <= lit || undefined}
          style={{ height: 4 + n * 3 }}
        />
      ))}
    </span>
  );
}
