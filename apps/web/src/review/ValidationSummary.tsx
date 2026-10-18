// The checks a person can fix before sending a submission (REV-ED-004): errors would be refused or are surely a
// mistake, warnings are hints. Live region so a fix is announced as the list shrinks.
import { Notice } from '@sideroom/ui';
import { useT, type MessageKey } from '../i18n';
import type { Issue } from './chapterEdit';

const TEXT: Record<Issue['key'], MessageKey> = {
  noTitle: 'review.validation.noTitle',
  noChapters: 'review.validation.noChapters',
  duplicateNumber: 'review.validation.duplicateNumber',
  noFile: 'review.validation.noFile',
  gap: 'review.validation.gap',
  noSynopsis: 'review.validation.noSynopsis',
};

export function ValidationSummary({ issues }: { issues: Issue[] }) {
  const t = useT();
  const errors = issues.filter((i) => i.level === 'error');
  const warnings = issues.filter((i) => i.level === 'warning');
  if (issues.length === 0)
    return (
      <Notice variant="success" title={t('review.validation.title')}>
        {t('review.validation.clean')}
      </Notice>
    );
  return (
    <Notice
      variant={errors.length > 0 ? 'danger' : 'warning'}
      title={t('review.validation.title')}
      role="status"
    >
      <p className="rv-muted" style={{ margin: 0 }}>
        {t('review.validation.counts', { errors: errors.length, warnings: warnings.length })}
      </p>
      <ul style={{ margin: 0, paddingInlineStart: 18 }}>
        {[...errors, ...warnings].map((i) => (
          <li key={`${i.key}-${JSON.stringify(i.vars ?? {})}`}>
            <strong>
              {i.level === 'error' ? t('review.validation.error') : t('review.validation.warning')}
              {': '}
            </strong>
            {t(TEXT[i.key], i.vars)}
          </li>
        ))}
      </ul>
    </Notice>
  );
}
