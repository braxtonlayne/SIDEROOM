import { Button, Modal } from '@sideroom/ui';
import { useT } from '../../i18n';
import { groupChannels, isTextLike } from '../channels';
import { useChannels, useGuild } from '../data';
import { useLoungeGoto } from '../nav';

/**
 * Browse Channels (LNG-GLD-032): every channel the viewer can open in a server that has onboarding, grouped by
 * category, each one a jump link. Which channels the viewer sees is already decided by roles and permissions.
 */
export function BrowseChannelsDialog({
  guildId,
  onClose,
}: {
  guildId: string;
  onClose: () => void;
}) {
  const t = useT();
  const guild = useGuild(guildId);
  const channels = useChannels(guildId);
  const goto = useLoungeGoto();
  const groups = groupChannels(channels).filter((g) => g.channels.length > 0);
  return (
    <Modal
      open
      onClose={onClose}
      size="md"
      title={t('lounge.browseChannels.title')}
      description={t('lounge.browseChannels.subtitle', { name: guild?.name ?? '' })}
      footer={<Button onClick={onClose}>{t('lounge.browseChannels.close')}</Button>}
    >
      {groups.length === 0 && <p>{t('lounge.browseChannels.empty')}</p>}
      {groups.map((g) => (
        <section
          key={g.category?.id ?? 'loose'}
          aria-label={g.category?.name ?? t('lounge.browseChannels.noCategory')}
        >
          <h3>{g.category?.name ?? t('lounge.browseChannels.noCategory')}</h3>
          <ul className="lng-guide-list">
            {g.channels.map((c) => (
              <li key={c.id}>
                {isTextLike(c) ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      goto(guildId, c.id);
                      onClose();
                    }}
                  >
                    #{c.name}
                  </Button>
                ) : (
                  <span>{c.name}</span>
                )}
                {c.topic && <span className="lng-muted"> {c.topic}</span>}
              </li>
            ))}
          </ul>
        </section>
      ))}
    </Modal>
  );
}
