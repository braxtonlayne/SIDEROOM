import { Avatar, Button, EmptyState, Icon, Notice } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { HOME } from '../channels';
import { useLoungeGoto } from '../nav';
import { blockUser, respondToMessageRequest, type DmObject } from './api';
import { rememberDm } from './dm';
import { useSocial, wireName } from './social';

/** Message Requests (LNG-DM-008): conversations from people who are not friends yet. */
export function MessageRequestsPage() {
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
      setError(e instanceof ApiError && e.message ? e.message : 'Could not update that request.');
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="lng-channel-view" aria-label="Message Requests">
      <header className="lng-header">
        <Icon name="user" size={24} />
        <h1 className="lng-header-title">Message Requests</h1>
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
            title="No message requests"
            description="When someone you don't share a friendship with messages you, it shows up here first."
          />
        )}
        <ul className="lng-friend-list" aria-label="Message requests">
          {list.map((dm) => {
            const from = dm.recipients[0];
            const name = from ? wireName(from) : 'Unknown';
            return (
              <li key={dm.id} className="lng-friend-row">
                <Avatar name={name} size={32} />
                <span className="lng-friend-text">
                  <span className="lng-friend-name">{name}</span>
                  <span className="lng-friend-sub">wants to send you a message</span>
                </span>
                <Button
                  size="sm"
                  loading={busy === dm.id}
                  aria-label={`Accept request from ${name}`}
                  onClick={() => void answer(dm, 'accepted')}
                >
                  Accept
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  aria-label={`Ignore request from ${name}`}
                  onClick={() => void answer(dm, 'ignored')}
                >
                  Ignore
                </Button>
                <Button
                  size="sm"
                  variant="danger"
                  aria-label={`Report ${name} as spam`}
                  onClick={() => void answer(dm, 'spam')}
                >
                  Spam
                </Button>
                {from && (
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={`Block ${name}`}
                    onClick={() => void answer(dm, 'ignored', from.id)}
                  >
                    Block
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
