import { useT } from '../i18n';
import { useTestDeployment } from './flag';
import './testdeploy.css';

/**
 * OPS-TDEP-001: a small, persistent notice for signed-in people while the server runs as a test deployment
 * (same-host Activities). It never takes clicks: whatever is underneath stays usable.
 */
export function TestDeploymentNotice() {
  const t = useT();
  const on = useTestDeployment((s) => s.on);
  if (!on) return null;
  return (
    <div className="tdep-notice" role="note" data-testid="test-deployment-notice">
      <strong>{t('testdeploy.notice.label')}</strong> <span>{t('testdeploy.notice.body')}</span>
    </div>
  );
}
