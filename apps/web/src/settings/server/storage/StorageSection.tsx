import { useT } from '../../../i18n';
import { ByosOffNotice, useByosOn } from '../../byos';
import { MountsPanel } from '../../user/storage/MountsPanel';
import type { SectionProps } from '../ServerSettings';
import { SectionHeader } from '../ui';

/**
 * Server Settings > Storage (BYOS-UI-001): the server's Guild Shelf mounts. Same panel as the user's,
 * scoped to `/guilds/{guild_id}/storage/mounts`. The server enforces the `manage` storage grant.
 */
export function ServerStorageSection({ guildId }: SectionProps) {
  const t = useT();
  const on = useByosOn();
  return (
    <>
      <SectionHeader
        title={t('settings.serverStorage.title')}
        description={t('settings.serverStorage.description')}
      />
      {on ? <MountsPanel scope={{ kind: 'guild', guildId }} /> : <ByosOffNotice />}
    </>
  );
}
