import { Icon, Popover } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { useStore } from '../../store';
import * as rest from '../../store/rest';
import type * as S from '../../store/types';
import { HOME } from '../channels';
import type { Message } from '../data';
import { toLoungeMessage } from '../liveMap';
import { useLoungeGotoMessage } from '../nav';
import { formatMessageTime } from '../time';
import './header.css';

export interface PinsButtonProps {
  /** Guild id, or '@me' for a DM. */
  guildId: string;
  channelId: string;
  /** The viewer may unpin (Pin Messages / Manage Messages). */
  canUnpin: boolean;
}

/** Pinned-messages popout in the channel header: list, jump, unpin (LNG-CHN-020). */
export function PinsButton({ guildId, channelId, canUnpin }: PinsButtonProps) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [pins, setPins] = useState<Message[] | null>(null);
  const [error, setError] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const jump = useLoungeGotoMessage();

  useEffect(() => {
    if (!open) return;
    let live = true;
    setError(false);
    apiFetch<S.Message[]>(`/channels/${channelId}/pins`, { retries: 0 })
      .then((list) => {
        if (live) setPins(list.map(toLoungeMessage));
      })
      .catch(() => {
        if (live) setError(true);
      });
    return () => {
      live = false;
    };
  }, [open, channelId]);

  const unpin = async (id: string) => {
    setBusy(id);
    try {
      await rest.unpinMessage(channelId, id);
      setPins((p) => p?.filter((m) => m.id !== id) ?? null);
      useStore.setState((s) => {
        const m = s.messages[id];
        return m ? { messages: { ...s.messages, [id]: { ...m, pinned: false } } } : s;
      });
    } catch {
      setError(true);
    } finally {
      setBusy(null);
    }
  };

  return (
    <Popover
      label={t('lounge.pins.title')}
      placement="bottom-end"
      onOpenChange={setOpen}
      trigger={
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.pins.title')}
          title={t('lounge.pins.title')}
        >
          <Icon name="pin" size={20} />
        </button>
      }
    >
      <div className="lng-pins-pop">
        <h2 className="lng-pop-title">{t('lounge.pins.title')}</h2>
        {error && (
          <p className="lng-muted" role="alert">
            {t('lounge.pins.error')}
          </p>
        )}
        {pins === null && !error && (
          <p className="lng-muted" role="status">
            {t('lounge.pins.loading')}
          </p>
        )}
        {pins?.length === 0 && <p className="lng-muted">{t('lounge.pins.empty')}</p>}
        <ul className="lng-pins-list">
          {pins?.map((m) => (
            <li key={m.id} className="lng-pin">
              <div className="lng-pin-head">
                <strong>{m.author.displayName}</strong>
                <span className="lng-muted">{formatMessageTime(m.createdAt)}</span>
              </div>
              <p className="lng-pin-body">
                {m.content ||
                  (m.attachments?.length ? t('lounge.pins.attachment') : t('lounge.pins.noText'))}
              </p>
              <div className="lng-pin-actions">
                <button
                  type="button"
                  className="lng-link-btn"
                  onClick={() => jump(guildId || HOME, channelId, m.id)}
                >
                  {t('lounge.pins.jump')}
                </button>
                {canUnpin && (
                  <button
                    type="button"
                    className="lng-link-btn"
                    disabled={busy === m.id}
                    aria-label={t('lounge.pins.unpinLabel', { name: m.author.displayName })}
                    onClick={() => void unpin(m.id)}
                  >
                    {t('lounge.pins.unpin')}
                  </button>
                )}
              </div>
            </li>
          ))}
        </ul>
      </div>
    </Popover>
  );
}
