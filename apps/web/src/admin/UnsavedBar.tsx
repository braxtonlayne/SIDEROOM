// The unsaved-changes bar under a SchemaForm (FND-FEAT-006). It always holds the form's action buttons; while the
// form differs from what is saved it also announces that and pins itself to the bottom of the scrolling area, so
// Save is never off screen after editing a long form.
import type { ReactNode } from 'react';
import { useT } from '../i18n';

export function UnsavedBar({ dirty, children }: { dirty: boolean; children: ReactNode }) {
  const t = useT();
  return (
    <div className="adm-unsaved" data-dirty={dirty ? 'true' : 'false'}>
      <p className="adm-unsaved-note" role="status">
        {dirty ? t('admin.unsaved.message') : ''}
      </p>
      <div className="adm-actions">{children}</div>
    </div>
  );
}
