import { useT } from '../../i18n';
import { scrapbookOn, useGuildScrapbook } from './store';
import './scrapbook.css';

/**
 * The channel banner of the Scrapbook module: tells members that a reaction can archive a message here. It
 * shows only where reactions do archive (the module on, the channel not ignored) so nobody is surprised that
 * their message was kept.
 */
export function ScrapbookBanner({
  guildId,
  channelId,
}: {
  guildId: string | undefined;
  channelId: string;
}) {
  const t = useT();
  const status = useGuildScrapbook(guildId);
  if (!scrapbookOn(status) || status.ignored_channels.includes(channelId)) return null;
  return (
    <p className="lng-scr-banner" role="note" data-testid="scr-banner">
      {t('lounge.scrapbook.banner', { emoji: status.emojis.join(' '), n: status.threshold })}
    </p>
  );
}
