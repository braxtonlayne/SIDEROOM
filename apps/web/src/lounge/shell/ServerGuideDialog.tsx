import { Button, Modal, Spinner } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { useChannels, useGuild } from '../data';
import { useLoungeGoto } from '../nav';
import { Md } from '../msg/Md';
import { getGuide, type Guide } from './joinPipeline';

/**
 * Server Guide (LNG-GLD-033): the welcome message, the to-dos that point at channels, and the resource pages the
 * managers wrote in Server Settings. Read-only; the editor lives in Settings, Community.
 */
export function ServerGuideDialog({ guildId, onClose }: { guildId: string; onClose: () => void }) {
  const t = useT();
  const guild = useGuild(guildId);
  const channels = useChannels(guildId);
  const goto = useLoungeGoto();
  const [guide, setGuide] = useState<Guide | null>(null);
  const [failed, setFailed] = useState(false);
  const [open, setOpen] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    getGuide(guildId).then(
      (g) => live && setGuide(g),
      () => live && setFailed(true),
    );
    return () => {
      live = false;
    };
  }, [guildId]);

  const empty = guide && !guide.welcome_message && !guide.todos.length && !guide.resources.length;
  return (
    <Modal
      open
      onClose={onClose}
      size="md"
      title={t('lounge.serverGuide.title', { name: guild?.name ?? '' })}
      footer={<Button onClick={onClose}>{t('lounge.serverGuide.close')}</Button>}
    >
      {failed && <p role="alert">{t('lounge.serverGuide.error')}</p>}
      {!guide && !failed && <Spinner label={t('lounge.serverGuide.loading')} />}
      {empty && <p>{t('lounge.serverGuide.empty')}</p>}
      {guide?.welcome_message && <p className="lng-guide-welcome">{guide.welcome_message}</p>}
      {guide && guide.todos.length > 0 && (
        <section aria-label={t('lounge.serverGuide.todos')}>
          <h3>{t('lounge.serverGuide.todos')}</h3>
          <ul className="lng-guide-list">
            {guide.todos.map((todo, i) => {
              const channel = channels.find((c) => c.id === todo.channel_id);
              return (
                <li key={todo.id ?? i}>
                  <strong>{todo.title}</strong>
                  {todo.description && <p className="lng-muted">{todo.description}</p>}
                  {channel && (
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => {
                        goto(guildId, channel.id);
                        onClose();
                      }}
                    >
                      {t('lounge.serverGuide.goTo', { name: channel.name ?? '' })}
                    </Button>
                  )}
                </li>
              );
            })}
          </ul>
        </section>
      )}
      {guide && guide.resources.length > 0 && (
        <section aria-label={t('lounge.serverGuide.resources')}>
          <h3>{t('lounge.serverGuide.resources')}</h3>
          <ul className="lng-guide-list">
            {guide.resources.map((r, i) => {
              const key = r.id ?? String(i);
              const shown = open === key;
              return (
                <li key={key}>
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-expanded={shown}
                    onClick={() => setOpen(shown ? null : key)}
                  >
                    {r.title}
                  </Button>
                  {shown && (
                    <div className="lng-guide-resource">
                      <Md text={r.content} />
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        </section>
      )}
    </Modal>
  );
}
