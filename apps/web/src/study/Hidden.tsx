import type { ReactNode } from 'react';
import { useT } from '../i18n';
import { useStudyFlag, type StudyFlag } from './flags';

/** A page that is hidden while its Study switch is off: it shows the plain "not found" text (the server answers 404). */
export function HiddenWhenOff({ flag, children }: { flag: StudyFlag; children: ReactNode }) {
  const t = useT();
  const on = useStudyFlag(flag);
  if (!on) return <p>{t('app.router.notFound')}</p>;
  return <>{children}</>;
}
