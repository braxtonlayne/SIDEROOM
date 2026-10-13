import { Button, Icon } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { Composer } from '../Composer';
import { useGuild, type Message } from '../data';
import { MessageList } from '../MessageList';
import { useLoungeGoto } from '../nav';
import { useMyTimeout } from '../timeout';
import { useThreads } from './threadStore';
import './threads.css';

/** A thread opened beside its parent channel: header, messages and composer (LNG-THR-001 UI). */
export function ThreadPane({
  guildId,
  parentId,
  threadId,
}: {
  guildId: string;
  parentId: string;
  threadId: string;
}) {
  const thread = useThreads((s) => s.byId[threadId]);
  const guild = useGuild(guildId);
  const timedOutUntil = useMyTimeout(guildId);
  const goto = useLoungeGoto();
  const [replyTo, setReplyTo] = useState<Message | null>(null);
  const [editing, setEditing] = useState<Message | null>(null);
  const [missing, setMissing] = useState(false);
  const [busy, setBusy] = useState(false);

  // A link to a thread the store has not seen (archived, or opened from a bookmark): fetch it once.
  useEffect(() => {
    if (thread) return;
    let live = true;
    apiFetch<Parameters<ReturnType<typeof useThreads.getState>['upsert']>[0][number]>(
      `/channels/${threadId}`,
      { retries: 0 },
    )
      .then((w) => live && useThreads.getState().upsert([w]))
      .catch(() => live && setMissing(true));
    return () => {
      live = false;
    };
  }, [thread, threadId]);

  const close = () => goto(guildId, parentId);
  const toggleFollow = async () => {
    if (!thread || busy) return;
    setBusy(true);
    try {
      if (thread.joined) await useThreads.getState().leave(threadId);
      else await useThreads.getState().join(threadId);
    } catch {
      /* the button state simply does not change */
    } finally {
      setBusy(false);
    }
  };

  const name = thread?.name ?? 'Thread';
  const canSend = !thread?.locked;
  return (
    <aside className="lng-thread-pane" aria-label={`Thread: ${name}`}>
      <header className="lng-thread-head">
        <Icon name="list" size={20} />
        <h2 className="lng-thread-title">{name}</h2>
        {thread && (
          <Button
            size="sm"
            variant={thread.joined ? 'secondary' : 'primary'}
            onClick={() => void toggleFollow()}
            disabled={busy}
          >
            {thread.joined ? 'Following' : 'Follow'}
          </Button>
        )}
        <button type="button" className="lng-icon-btn" aria-label="Close thread" onClick={close}>
          <Icon name="close" size={20} />
        </button>
      </header>
      {missing && !thread ? (
        <p className="lng-muted lng-thread-note" role="alert">
          This thread no longer exists, or you cannot see it.
        </p>
      ) : (
        <>
          {thread?.archived && (
            <p className="lng-muted lng-thread-note" role="status">
              This thread is archived. Sending a message brings it back.
            </p>
          )}
          <MessageList
            channelId={threadId}
            guildId={guildId}
            title={name}
            inThread
            onReply={(m) => {
              setEditing(null);
              setReplyTo(m);
            }}
            onEdit={(m) => {
              setReplyTo(null);
              setEditing(m);
            }}
            editingId={editing?.id}
            onCancelEdit={() => setEditing(null)}
          />
          <Composer
            channelId={threadId}
            guildId={guildId}
            target={name}
            canSend={canSend}
            timedOutUntil={timedOutUntil}
            bypassSlowmode={!!guild?.permissions.manageMessages}
            replyTo={replyTo}
            onCancelReply={() => setReplyTo(null)}
            onEditLast={() => undefined}
          />
        </>
      )}
    </aside>
  );
}
