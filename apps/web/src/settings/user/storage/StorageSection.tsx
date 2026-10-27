import { useT } from '../../../i18n';
import { ByosOffNotice, useByosOn } from '../../byos';
import { SectionTitle } from '../parts';
import { MountsPanel } from './MountsPanel';

/** User Settings > Storage (BYOS-UI-001): the caller's own Media Shelf mounts. */
export function StorageSection() {
  const t = useT();
  const on = useByosOn();
  return (
    <>
      <SectionTitle>{t('settings.storage.title')}</SectionTitle>
      {on ? <MountsPanel scope={{ kind: 'user' }} /> : <ByosOffNotice />}
    </>
  );
}
