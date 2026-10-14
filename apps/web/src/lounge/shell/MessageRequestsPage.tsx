import { Avatar, Button, EmptyState, Icon, Notice } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { HOME } from '../channels';
import { useLoungeGoto } from '../nav';
import { blockUser, respondToMessageRequest, type DmObject } from './api';
import { rememberDm } from './dm';
import { useSocial, wireName } from './social';

/** Message Requests (LNG-DM-008): conversations from people who are not friends yet. */
export function MessageRequestsPage() {
  const t = useT();
  const list = useSocial((s) => s.messageRequests);
  const forget = useSocial((s) => s.forgetMessageRequest);
  const goto = useLoungeGoto();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const answer = async (
    dm: DmObject,
    state: 'accepted' | 'ignored' | 'spam',
    alsoBlock?: string,
  ) => {
    setBusy(dm.id);
    setError(null);
    try {
      await respondToMessageRequest(dm.id, state);
      if (alsoBlock) await blockUser(alsoBlock);
      forget(dm.id);
      if (state === 'accepted') {
        rememberDm({ ...dm, is_message_request: false });
        goto(HOME, dm.id);
      }
    } catch (e) {
      setError(
        e instanceof ApiError && e.message ? e.message : t('lounge.messageRequests.updateError'),
      );
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="lng-channel-view" aria-label={t('lounge.messageRequests.title')}>
      <header className="lng-header">
        <Icon name="user" size={24} />
        <h1 className="lng-header-title">{t('lounge.messageRequests.title')}</h1>
      </header>
      <div className="lng-friends-main">
        {error && (
          <Notice variant="danger" onDismiss={() => setError(null)}>
            {error}
          </Notice>
        )}
        {list.length === 0 && (
          <EmptyState
            icon={<Icon name="user" size={24} />}
            title={t('lounge.messageRequests.emptyTitle')}
            description={t('lounge.messageRequests.emptyBody')}
          />
        )}
        <ul className="lng-friend-list" aria-label={t('lounge.messageRequests.list')}>
          {list.map((dm) => {
            const from = dm.recipients[0];
            const name = from ? wireName(from) : t('lounge.messageRequests.unknown');
            return (
              <li key={dm.id} className="lng-friend-row">
                <Avatar name={name} size={32} />
                <span className="lng-friend-text">
                  <span className="lng-friend-name">{name}</span>
                  <span className="lng-friend-sub">
                    {t('lounge.messageRequests.wantsToMessage')}
                  </span>
                </span>
                <Button
                  size="sm"
                  loading={busy === dm.id}
                  aria-label={t('lounge.messageRequests.acceptLabel', { name })}
                  onClick={() => void answer(dm, 'accepted')}
                >
                  {t('lounge.messageRequests.accept')}
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={t('lounge.messageRequests.ignoreLabel', { name })}
                  onClick={() => void answer(dm, 'ignored')}
                >
                  {t('lounge.messageRequests.ignore')}
                </Button>
                <Button
                  size="sm"
                  variant="danger"
                  aria-label={t('lounge.messageRequests.spamLabel', { name })}
                  onClick={() => void answer(dm, 'spam')}
                >
                  {t('lounge.messageRequests.spam')}
                </Button>
                {from && (
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={t('lounge.messageRequests.blockLabel', { name })}
                    onClick={() => void answer(dm, 'ignored', from.id)}
                  >
                    {t('lounge.messageRequests.block')}
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      </div>
    </section>
  );
}
