// Admin > Moderation (MOD-CASE-011/012, MOD-HOLD-006, MOD-APL-005; docs/specs/moderation-compliance.md section 9).
// The queue, one case at a time, the holds list and the appeals list. The API decides what each viewer may see.
import { Tabs } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { SectionHeader, useAdminViewer } from '../ui';
import { CasePage } from './CasePage';
import { CaseQueue } from './CaseQueue';
import { AppealsTab, HoldsTab } from './HoldsAndAppeals';

export function ModerationSection() {
  const t = useT();
  const manage = useAdminViewer().can('MANAGE_CASES');
  const [tab, setTab] = useState('cases');
  const [caseId, setCaseId] = useState<string | null>(null);

  const open = (id: string) => {
    setTab('cases');
    setCaseId(id);
  };

  return (
    <>
      <SectionHeader
        title={t('admin.moderation.title')}
        description={t('admin.moderation.description')}
      />
      {caseId ? (
        <CasePage id={caseId} onBack={() => setCaseId(null)} />
      ) : !manage ? (
        // Reviewers work the cases routed to them; holds and appeals are platform staff's (D-213, D-215).
        <CaseQueue onOpen={open} />
      ) : (
        <Tabs
          label={t('admin.moderation.tabsLabel')}
          value={tab}
          onValueChange={setTab}
          items={[
            {
              id: 'cases',
              label: t('admin.moderation.tabCases'),
              content: <CaseQueue onOpen={open} />,
            },
            {
              id: 'holds',
              label: t('admin.moderation.tabHolds'),
              content: <HoldsTab onOpenCase={open} />,
            },
            {
              id: 'appeals',
              label: t('admin.moderation.tabAppeals'),
              content: <AppealsTab onOpenCase={open} />,
            },
          ]}
        />
      )}
    </>
  );
}
