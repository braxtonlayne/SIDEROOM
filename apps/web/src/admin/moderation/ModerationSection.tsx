// Admin > Moderation (MOD-CASE-011/012, MOD-HOLD-006, MOD-APL-005, MOD-TKD-010, MOD-DASH-*; docs/specs/
// moderation-compliance.md section 9). The queue, one case at a time, the holds list, the appeals list, the takedown
// register and the dashboard. The API decides what each viewer may see; the tabs follow the platform bits (cases,
// takedowns; PLATFORM_BAN adds the ban figures to the dashboard).
import { Tabs } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { SectionHeader, useAdminViewer } from '../ui';
import { CasePage } from './CasePage';
import { CaseQueue } from './CaseQueue';
import { DashboardTab } from './DashboardTab';
import { AppealsTab, HoldsTab } from './HoldsAndAppeals';
import { TakedownsTab } from './TakedownsTab';

export function ModerationSection() {
  const t = useT();
  const viewer = useAdminViewer();
  const manage = viewer.can('MANAGE_CASES');
  const takedowns = viewer.can('MANAGE_TAKEDOWNS');
  // Reviewers (no MANAGE_CASES) still work the cases routed to them (D-331).
  const cases =
    manage ||
    (['REVIEW_READING', 'REVIEW_MEDIA', 'DELETE_CONTENT'] as const).some((p) => viewer.can(p));
  // The Dashboard tab shows whatever the viewer's bits allow: MANAGE_CASES, MANAGE_TAKEDOWNS and, for the ban
  // figures, PLATFORM_BAN on top of one of them (the section itself stays hidden from PLATFORM_BAN alone).
  const dashboard = manage || takedowns;
  const [tab, setTab] = useState(cases ? 'cases' : takedowns ? 'takedowns' : 'dashboard');
  const [caseId, setCaseId] = useState<string | null>(null);
  const [noticeId, setNoticeId] = useState<string | null>(null);

  const open = (id: string) => {
    setTab('cases');
    setCaseId(id);
  };

  // The badge on a case opens its notice in the Takedowns tab (MANAGE_TAKEDOWNS).
  const openNotice = takedowns
    ? (id: string) => {
        setCaseId(null);
        setNoticeId(id);
        setTab('takedowns');
      }
    : undefined;

  const items = [
    ...(dashboard
      ? [
          {
            id: 'dashboard',
            label: t('admin.moderation.tabDashboard'),
            content: (
              <DashboardTab
                onOpenCase={open}
                onOpenRegister={takedowns ? () => setTab('takedowns') : undefined}
              />
            ),
          },
        ]
      : []),
    ...(cases
      ? [
          {
            id: 'cases',
            label: t('admin.moderation.tabCases'),
            content: <CaseQueue onOpen={open} onOpenNotice={openNotice} />,
          },
        ]
      : []),
    ...(manage
      ? [
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
        ]
      : []),
    ...(takedowns
      ? [
          {
            id: 'takedowns',
            label: t('admin.moderation.tabTakedowns'),
            content: <TakedownsTab onOpenCase={open} noticeId={noticeId} />,
          },
        ]
      : []),
  ];

  return (
    <>
      <SectionHeader
        title={t('admin.moderation.title')}
        description={t('admin.moderation.description')}
      />
      {caseId && cases ? (
        <CasePage id={caseId} onBack={() => setCaseId(null)} onOpenNotice={openNotice} />
      ) : !dashboard ? (
        // Reviewers work the cases routed to them; holds and appeals are platform staff's (D-213, D-215).
        <CaseQueue onOpen={open} />
      ) : (
        <Tabs
          label={t('admin.moderation.tabsLabel')}
          value={tab}
          onValueChange={setTab}
          items={items}
        />
      )}
    </>
  );
}
