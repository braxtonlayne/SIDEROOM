import { UiStringsProvider } from '@sideroom/ui';
import { useMemo, type ReactNode } from 'react';
import { useT } from '../i18n';

/** Hands @sideroom/ui the translated words its primitives speak on their own (notice prefixes, Dismiss). */
export function AppUiStrings({ children }: { children: ReactNode }) {
  const t = useT();
  const value = useMemo(
    () => ({
      noticeInfo: t('a11y.notice.info'),
      noticeSuccess: t('a11y.notice.success'),
      noticeWarning: t('a11y.notice.warning'),
      noticeError: t('a11y.notice.error'),
      dismiss: t('a11y.notice.dismiss'),
      notifications: t('a11y.toast.region'),
    }),
    [t],
  );
  return <UiStringsProvider value={value}>{children}</UiStringsProvider>;
}
