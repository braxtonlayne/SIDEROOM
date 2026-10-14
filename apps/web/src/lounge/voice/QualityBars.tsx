import { t } from '../../i18n';
import { barsFor, QUALITY_LABEL, type ConnectionQuality } from '../../voice/quality';

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
