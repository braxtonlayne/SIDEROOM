// Admin > Moderation > Signals (MOD-SUS-003): the evasion signals waiting for review. They are IP-derived, so the
// tab is for staff with `MANAGE_ACCOUNTS`, and every read is logged. A signal never bans anyone.
import { EmptyState, Select } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import * as api from '../../moderation/api';
import { ApiProblem, useAdminViewer, useResource } from '../ui';
import { SignalRow } from './SignalRow';

export function SignalsTab() {
  const t = useT();
  const viewer = useAdminViewer();
  const [state, setState] = useState<api.SignalState | 'all'>('open');
  const res = useResource(`signals:${state}`, () => api.listEvasionSignals({ state, limit: 100 }));
  return (
    <>
      <p className="ss-sub">{t('admin.suspicion.tabIntro')}</p>
      <div className="ss-toolbar">
        <Select
          aria-label={t('admin.suspicion.stateFilter')}
          options={[
            { value: 'open', label: t('admin.suspicion.signalOpen') },
            { value: 'confirmed', label: t('admin.suspicion.signalConfirmed') },
            { value: 'dismissed', label: t('admin.suspicion.signalDismissed') },
            { value: 'all', label: t('admin.suspicion.stateAll') },
          ]}
          value={state}
          onValueChange={(v) => setState(v as api.SignalState | 'all')}
        />
      </div>
      <ApiProblem error={res.error} />
      {res.data && res.data.length === 0 && (
        <EmptyState
          title={t('admin.suspicion.emptyTitle')}
          description={t('admin.suspicion.emptyDescription')}
        />
      )}
      {res.data && res.data.length > 0 && (
        <ul className="ss-list" aria-label={t('admin.suspicion.signalsTitle')}>
          {res.data.map((s) => (
            <SignalRow
              key={s.id}
              signal={s}
              canReview={viewer.can('MANAGE_ACCOUNTS')}
              onChanged={() => void res.reload()}
            />
          ))}
        </ul>
      )}
    </>
  );
}
