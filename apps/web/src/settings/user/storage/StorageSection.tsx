import { useT } from '../../../i18n';
import { SectionTitle } from '../parts';
import { MountsPanel } from './MountsPanel';

/** User Settings > Storage (BYOS-UI-001): the caller's own Media Shelf mounts. */
export function StorageSection() {
  const t = useT();
  return (
    <>
      <SectionTitle>{t('settings.storage.title')}</SectionTitle>
      <MountsPanel scope={{ kind: 'user' }} />
    </>
  );
}
