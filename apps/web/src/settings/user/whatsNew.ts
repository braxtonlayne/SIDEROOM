import { t } from '../../i18n';

/** Release notes shown by Settings > What's New. Newest first; plain text only. */
export interface ReleaseNote {
  title: string;
  items: string[];
}

/** Notes resolve when read, so they follow the active language. */
export const WHATS_NEW: ReleaseNote[] = [
  {
    get title() {
      return t('settings.whatsNew.filledIn.title');
    },
    get items() {
      return [
        t('settings.whatsNew.filledIn.item1'),
        t('settings.whatsNew.filledIn.item2'),
        t('settings.whatsNew.filledIn.item3'),
        t('settings.whatsNew.filledIn.item4'),
      ];
    },
  },
];
