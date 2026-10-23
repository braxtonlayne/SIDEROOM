// Admin > Moderation (MOD-CASE-011/012, MOD-HOLD-006, MOD-APL-005, MOD-TKD-010, MOD-DASH-*, MOD-SUS-003; docs/specs/
// moderation-compliance.md section 9). The queue, one case at a time, the holds list, the appeals list, the takedown
// register, the dashboard and the evasion signals. The API decides what each viewer may see; every tab follows one
// platform bit: Dashboard MANAGE_CASES or MANAGE_TAKEDOWNS (PLATFORM_BAN adds the ban figures), Cases MANAGE_CASES and
// the reviewer bits, Holds and Appeals MANAGE_CASES, Signals MANAGE_ACCOUNTS, Takedowns MANAGE_TAKEDOWNS. A viewer
// with a single tab gets its content without a tab bar.
import { Tabs } from '@sideroom/ui';
import { useLocation } from '@tanstack/react-router';
import { useState } from 'react';
import { useT } from '../../i18n';
import { SectionHeader, useAdminViewer } from '../ui';
import { CasePage } from './CasePage';
import { CaseQueue } from './CaseQueue';
import { DashboardTab } from './DashboardTab';
import { AppealsTab, HoldsTab } from './HoldsAndAppeals';
import { SignalsTab } from './SignalsTab';
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
  // Signals are IP-derived: MANAGE_ACCOUNTS (P-16). A deep link `?tab=signals` opens them (the Review hub tile).
  const signals = viewer.can('MANAGE_ACCOUNTS');
  const search = useLocation({ select: (l) => l.searchStr });
  const wantSignals = signals && /(^|[?&])tab=signals/.test(search);
  const [tab, setTab] = useState(
    wantSignals
      ? 'signals'
      : cases
        ? 'cases'
        : takedowns
          ? 'takedowns'
          : signals
            ? 'signals'
            : 'dashboard',
  );
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
    ...(signals
      ? [
          {
            id: 'signals',
            label: t('admin.suspicion.tabSignals'),
            content: <SignalsTab />,
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
      ) : items.length <= 1 ? (
        // One tab only (a reviewer's queue, D-331; MANAGE_ACCOUNTS alone sees the signals, P-16): no tab bar.
        (items[0]?.content ?? null)
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
