import { Markdown, type Resolvers } from '@sideroom/markdown-react';
import { Icon, Popover } from '@sideroom/ui';
import { useEffect, useMemo, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { useSavedPrefs } from '../../settings/user/prefs';
import { useStore } from '../../store';
import * as rest from '../../store/rest';
import type * as S from '../../store/types';
import { HOME } from '../channels';
import type { Message } from '../data';
import { toLoungeMessage } from '../liveMap';
import { currentMarkdownEngine, isJumbo, parseMarkdown, useMarkdownReady } from '../markdown';
import { useLoungeGotoMessage } from '../nav';
import { CustomEmoji } from '../msg/CustomEmoji';
import { useHiddenKind } from '../msg/lineText';
import { richCardResolvers } from '../msg/RichCards';
import { storeResolvers } from '../shell/notifyText';
import { formatMessageTime } from '../time';
import './header.css';

export interface PinsButtonProps {
  /** Guild id, or '@me' for a DM. */
  guildId: string;
  channelId: string;
  /** The viewer may unpin (Pin Messages / Manage Messages). */
  canUnpin: boolean;
}

/**
 * A pin's text. A blocked or ignored author's pin is the list's hidden message of one with its "Show" (D-730):
 * the reveal is this pin's own, and a block made while the panel is open takes effect at once.
 */
function PinText({ m, guildId }: { m: Message; guildId: string }) {
  const t = useT();
  useMarkdownReady();
  const { spoilers } = useSavedPrefs();
  const kind = useHiddenKind(m.author.id);
  const [shown, setShown] = useState(false);
  const hidden = kind && !shown;
  // The message list's resolvers for this server (names from the store, a custom emoji, the rich cards).
  const resolvers: Resolvers = useMemo(
    () => ({
      ...storeResolvers(guildId === HOME ? null : guildId),
      ...richCardResolvers,
      customEmoji: (e) => <CustomEmoji id={e.id} name={e.name} animated={e.animated} />,
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the names are read when the pin is drawn
    [guildId, m.id],
  );
  const fallback = m.attachments?.length ? t('lounge.pins.attachment') : t('lounge.pins.noText');
  return (
    <>
      <div className="lng-pin-head">
        {!hidden && <strong>{m.author.displayName}</strong>}
        <span className="lng-muted">{formatMessageTime(m.createdAt)}</span>
      </div>
      {hidden ? (
        <p className="lng-pin-body lng-muted" data-hidden-line={kind}>
          {t('lounge.messageList.hiddenRun', { count: 1, kind })}{' '}
          <button type="button" className="lng-link-btn" onClick={() => setShown(true)}>
            {t('lounge.messageList.showHidden', { count: 1 })}
          </button>
        </p>
      ) : (
        <div className="lng-pin-body">
          {/* D-739: a pin is a message, drawn by the message list's renderer (no embeds, stickers or reactions).
              Until the parser is loaded there is no text to draw: never the typed text. */}
          {m.content && currentMarkdownEngine() ? (
            <Markdown
              nodes={parseMarkdown(m.content)}
              jumbo={isJumbo(m.content)}
              revealSpoilers={spoilers === 'always'}
              resolvers={resolvers}
            />
          ) : (
            fallback
          )}
        </div>
      )}
    </>
  );
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
              <PinText m={m} guildId={guildId} />
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
