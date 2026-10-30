import { Link } from '@tanstack/react-router';
import { Icon } from '@sideroom/ui';
import { useT } from '../../i18n';
import { scrapbookOn, useGuildScrapbook } from './store';

/** The Scrapbook row at the top of the channel list: only where the operator offered the module and the server uses it. */
export function ScrapbookRow({ guildId }: { guildId: string }) {
  const t = useT();
  const status = useGuildScrapbook(guildId);
  if (!scrapbookOn(status)) return null;
  return (
    <Link to="/channels/$guildId/scrapbook" params={{ guildId }} className="lng-events-row">
      <Icon name="image" size={20} />
      <span>{t('lounge.scrapbook.nav')}</span>
    </Link>
  );
}
