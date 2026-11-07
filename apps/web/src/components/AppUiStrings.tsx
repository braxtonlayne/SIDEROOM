import { UiStringsProvider } from '@sideroom/ui';
import { useMemo, type ReactNode } from 'react';
import { useT } from '../i18n';

/** Hands @sideroom/ui the translated words its primitives speak on their own (notice prefixes, Dismiss, the colour picker). */
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
      colorHex: t('a11y.colorPicker.hex'),
      colorPick: t('a11y.colorPicker.pick'),
      // SEC-AUD-040: the rest of the kit's own words (core fragment p54-webb-2, so public pages have them).
      close: t('a11y.kit.close'),
      confirm: t('a11y.kit.confirm'),
      cancel: t('a11y.kit.cancel'),
      typeToConfirm: (text: string) => t('a11y.kit.typeToConfirm', { text }),
      showPassword: t('a11y.kit.showPassword'),
      hidePassword: t('a11y.kit.hidePassword'),
      show: t('a11y.kit.show'),
      hide: t('a11y.kit.hide'),
      passwordStrength: t('a11y.kit.passwordStrength'),
      strengthLevels: [
        t('a11y.kit.strength0'),
        t('a11y.kit.strength1'),
        t('a11y.kit.strength2'),
        t('a11y.kit.strength3'),
        t('a11y.kit.strength4'),
      ] as const,
      errorTitle: t('a11y.kit.errorTitle'),
      errorDescription: t('a11y.kit.errorDescription'),
      technicalDetails: t('a11y.kit.technicalDetails'),
      tryAgain: t('a11y.kit.tryAgain'),
      noResults: t('a11y.kit.noResults'),
      removeOption: (label: string) => t('a11y.kit.removeOption', { label }),
      selectedCount: (count: number) => t('a11y.kit.selectedCount', { count }),
      loading: t('a11y.kit.loading'),
      submenu: t('a11y.kit.submenu'),
    }),
    [t],
  );
  return <UiStringsProvider value={value}>{children}</UiStringsProvider>;
}
