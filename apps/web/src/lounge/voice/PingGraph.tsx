import { useT } from '../../i18n';

const W = 120;
const H = 32;

/** Points of the sparkline for the readings (oldest first), scaled to the graph box. */
export function pingPoints(history: readonly number[], width = W, height = H): string {
  if (history.length === 0) return '';
  const max = Math.max(100, ...history);
  const step = history.length > 1 ? width / (history.length - 1) : 0;
  return history
    .map(
      (ms, i) =>
        `${Math.round(i * step * 10) / 10},${Math.round((height - (ms / max) * (height - 2) - 1) * 10) / 10}`,
    )
    .join(' ');
}

/** The ping tooltip's graph: recent round-trip times as a sparkline (VOX-UI-001). */
export function PingGraph({ history }: { history: readonly number[] }) {
  const t = useT();
  if (history.length < 2)
    return <span className="vc-ping-empty">{t('lounge.pingGraph.collecting')}</span>;
  const avg = Math.round(history.reduce((a, b) => a + b, 0) / history.length);
  const peak = Math.max(...history);
  return (
    <span className="vc-ping-graph">
      <svg
        viewBox={`0 0 ${W} ${H}`}
        width={W}
        height={H}
        role="img"
        aria-label={t('lounge.pingGraph.label', { avg, peak })}
      >
        <polyline
          points={pingPoints(history)}
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
        />
      </svg>
      <span className="vc-ping-stats">{t('lounge.pingGraph.stats', { avg, peak })}</span>
    </span>
  );
}
