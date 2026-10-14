import { Button, DropdownMenu, Icon, type MenuEntry } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { apiFetch } from '../../api/client';
import { useT } from '../../i18n';
import { Composer } from '../Composer';
import { useCurrentUser, useGuild, type Message } from '../data';
import { isMuted, MUTE_DURATIONS, useNotifSettings } from '../shell/notifSettings';
import { ThreadMembers } from './ThreadMembers';
import { ThreadSettingsDialog } from './ThreadSettingsDialog';
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
  const t = useT();
  const thread = useThreads((s) => s.byId[threadId]);
  const guild = useGuild(guildId);
  const timedOutUntil = useMyTimeout(guildId);
  const goto = useLoungeGoto();
  const [replyTo, setReplyTo] = useState<Message | null>(null);
  const [editing, setEditing] = useState<Message | null>(null);
  const [missing, setMissing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const me = useCurrentUser();
  const notif = useNotifSettings((s) => s.byGuild[guildId]);
  const muted = isMuted(notif?.channels[threadId]);
  const patchChannel = useNotifSettings((s) => s.patchChannel);

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

  useEffect(() => {
    void useNotifSettings.getState().load(guildId);
  }, [guildId]);

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

  const canManage =
    !!thread &&
    (thread.ownerId === me.id ||
      !!guild?.permissions.manageChannels ||
      !!guild?.permissions.manageMessages);
  const quiet = (p: Promise<void>) => void p.catch(() => undefined);
  const menu: MenuEntry[] = [
    ...(thread
      ? [
          {
            label: thread.joined
              ? t('lounge.threadPane.leaveThread')
              : t('lounge.threadPane.joinThread'),
            onSelect: () => void toggleFollow(),
          } satisfies MenuEntry,
        ]
      : []),
    muted
      ? {
          label: t('lounge.threadPane.unmute'),
          onSelect: () => quiet(patchChannel(guildId, threadId, { mute: { muted: false } })),
        }
      : {
          type: 'submenu',
          label: t('lounge.threadPane.mute'),
          items: MUTE_DURATIONS.map((d) => ({
            label: d.label,
            onSelect: () =>
              quiet(patchChannel(guildId, threadId, { mute: { muted: true, seconds: d.seconds } })),
          })),
        },
    {
      label: expanded ? t('lounge.threadPane.sidePane') : t('lounge.threadPane.fullView'),
      onSelect: () => setExpanded((v) => !v),
    },
    ...(canManage
      ? [
          { type: 'separator' } satisfies MenuEntry,
          {
            label: t('lounge.threadPane.settings'),
            onSelect: () => setSettingsOpen(true),
          } satisfies MenuEntry,
        ]
      : []),
  ];
  const name = thread?.name ?? t('lounge.threadPane.defaultName');
  const canSend = !thread?.locked;
  return (
    <aside
      className="lng-thread-pane"
      data-expanded={expanded || undefined}
      aria-label={t('lounge.threadPane.label', { name })}
    >
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
            {thread.joined ? t('lounge.threadPane.following') : t('lounge.threadPane.follow')}
          </Button>
        )}
        {thread && <ThreadMembers threadId={threadId} count={thread.memberCount} />}
        {thread && (
          <DropdownMenu
            label={t('lounge.threadPane.menu')}
            placement="bottom-end"
            items={menu}
            trigger={
              <button
                type="button"
                className="lng-icon-btn"
                aria-label={t('lounge.threadPane.menu')}
                title={t('lounge.threadPane.menu')}
                data-muted={muted || undefined}
              >
                <Icon name="bell" size={20} />
              </button>
            }
          />
        )}
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('lounge.threadPane.close')}
          onClick={close}
        >
          <Icon name="close" size={20} />
        </button>
      </header>
      {missing && !thread ? (
        <p className="lng-muted lng-thread-note" role="alert">
          {t('lounge.threadPane.missing')}
        </p>
      ) : (
        <>
          {thread?.archived && (
            <p className="lng-muted lng-thread-note" role="status">
              {t('lounge.threadPane.archived')}
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
      {settingsOpen && thread && (
        <ThreadSettingsDialog
          thread={thread}
          onClose={() => setSettingsOpen(false)}
          onDeleted={() => {
            setSettingsOpen(false);
            close();
          }}
        />
      )}
    </aside>
  );
}
