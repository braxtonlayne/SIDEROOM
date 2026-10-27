// The progress indicator of a conversion (BYOS-CNV-001): a bar with the phase, the percentage and an estimate.
// Indeterminate while the length of the source is unknown (and while the core loads).
import { useT } from '../i18n';
import type { ConvertProgress as Progress } from './types';

const PHASE_KEY = {
  loading: 'convert.progress.loading',
  probing: 'convert.progress.probing',
  remuxing: 'convert.progress.remuxing',
  extracting: 'convert.progress.extracting',
  transcoding: 'convert.progress.transcoding',
  uploading: 'convert.progress.uploading',
} as const;

export function ConvertProgressBar({ name, progress }: { name: string; progress: Progress }) {
  const t = useT();
  const phase = t(PHASE_KEY[progress.phase]);
  const percent = progress.fraction === null ? null : Math.round(progress.fraction * 100);
  const label =
    percent === null
      ? t('convert.progress.working', { name, phase })
      : t('convert.progress.percent', { name, phase, percent });
  const eta =
    progress.etaSec === null
      ? null
      : progress.etaSec < 60
        ? t('convert.progress.etaSeconds')
        : t('convert.progress.eta', { minutes: Math.round(progress.etaSec / 60) });
  return (
    <span className="cnv-progress">
      {/* No value attribute = an indeterminate bar. */}
      <progress {...(percent === null ? {} : { value: percent, max: 100 })} aria-label={label} />
      <span className="cnv-progress-text" role="status">
        {phase}
        {percent === null ? '' : ` ${percent}%`}
        {eta ? `, ${eta}` : ''}
      </span>
    </span>
  );
}
