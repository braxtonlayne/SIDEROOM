import { Avatar, Button, Icon, Notice } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { HOME } from '../channels';
import { useLoungeGoto } from '../nav';
import { useLoungeUi } from '../store';
import { blockUser, respondToMessageRequest, type DmObject } from './api';
import { rememberDm } from './dm';
import { requestReasonText } from './requestReason';
import { useSocial, wireName } from './social';

export type RequestAnswer = 'accepted' | 'ignored' | 'spam';

/** Accept, ignore or report a message request (shared by the request list and the request view). */
export function useAnswerRequest() {
  const t = useT();
  const forget = useSocial((s) => s.forgetMessageRequest);
  const goto = useLoungeGoto();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const answer = async (dm: DmObject, state: RequestAnswer, alsoBlock?: string) => {
    setBusy(dm.id);
    setError(null);
    try {
      await respondToMessageRequest(dm.id, state);
      // The answer is stored now: the request leaves the list even when the block after it fails.
      forget(dm.id);
      if (alsoBlock) {
        try {
          await blockUser(alsoBlock);
        } catch {
          setError(t('lounge.friends.blockError'));
          return false;
        }
      }
      if (state === 'accepted') {
        rememberDm({ ...dm, is_message_request: false });
        goto(HOME, dm.id);
      }
      return true;
    } catch (e) {
      setError(
        e instanceof ApiError && e.message ? e.message : t('lounge.messageRequests.updateError'),
      );
      return false;
    } finally {
      setBusy(null);
    }
  };

  return { answer, error, setError, busy };
}

/**
 * O1: a DM that is still a pending message request, opened by link (`/channels/@me/<id>`). The recipient
 * sees who wrote and why it is a request, and answers it here instead of on a "doesn't exist" page.
 */
export function MessageRequestView({
  dm,
  onBack,
}: {
  dm: DmObject;
  onBack?: (() => void) | undefined;
}) {
  const t = useT();
  const goto = useLoungeGoto();
  const setHomeView = useLoungeUi((s) => s.setHomeView);
  const { answer, error, setError, busy } = useAnswerRequest();
  const from = dm.recipients[0];
  const name = from ? wireName(from) : t('lounge.messageRequests.unknown');
  const reason = requestReasonText(dm, t);

  const toList = () => {
    setHomeView('requests');
    goto(HOME);
  };
  const leave = async (state: RequestAnswer, alsoBlock?: string) => {
    if ((await answer(dm, state, alsoBlock)) && state !== 'accepted') toList();
  };

  return (
    <section className="lng-channel-view" aria-labelledby="mr-view-h">
      <header className="lng-header">
        {onBack && (
          <button
            type="button"
            className="lng-icon-btn lng-back"
            aria-label={t('lounge.channelHeader.backToChannels')}
            onClick={onBack}
          >
            <Icon name="arrow-left" size={20} />
          </button>
        )}
        <Icon name="user" size={24} />
        <span className="lng-header-title">{t('lounge.messageRequests.title')}</span>
      </header>
      <div className="lng-request-view">
        {error && (
          <Notice variant="danger" onDismiss={() => setError(null)}>
            {error}
          </Notice>
        )}
        <Avatar name={name} size={80} />
        <h1 id="mr-view-h" className="lng-request-view-name">
          {name}
        </h1>
        <p className="lng-request-view-sub">{t('lounge.messageRequests.wantsToMessage')}</p>
        {reason && <p className="lng-muted">{reason}</p>}
        <p className="lng-muted lng-request-view-help">{t('lounge.polishLounge.requestHelp')}</p>
        <div className="lng-request-view-actions">
          <Button
            loading={busy === dm.id}
            aria-label={t('lounge.messageRequests.acceptLabel', { name })}
            onClick={() => void leave('accepted')}
          >
            {t('lounge.messageRequests.accept')}
          </Button>
          <Button
            variant="secondary"
            aria-label={t('lounge.messageRequests.ignoreLabel', { name })}
            onClick={() => void leave('ignored')}
          >
            {t('lounge.messageRequests.ignore')}
          </Button>
          <Button
            variant="secondary"
            aria-label={t('lounge.messageRequests.spamLabel', { name })}
            onClick={() => void leave('spam')}
          >
            {t('lounge.messageRequests.spam')}
          </Button>
          {from && (
            <Button
              variant="danger"
              aria-label={t('lounge.messageRequests.blockLabel', { name })}
              onClick={() => void leave('ignored', from.id)}
            >
              {t('lounge.messageRequests.block')}
            </Button>
          )}
        </div>
        <Button variant="link" size="sm" onClick={toList}>
          {t('lounge.polishLounge.allRequests')}
        </Button>
      </div>
    </section>
  );
}
