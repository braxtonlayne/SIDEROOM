import { useRouterState } from '@tanstack/react-router';
import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { useT } from '../../i18n';
import { DM_GUILD, useChannels, type Channel } from '../../store';
import { onDispatch } from '../../store/events';
import './rooms.css';
import { createRoom, deleteRoom, listRooms, type ToolRoom, type ToolRoomEvent } from './roomApi';
import { useLiveRoom, type LiveRoom } from './useRoom';

/** The channel the member is looking at, from the address (`/channels/<server or @me>/<channel>`). */
export function channelFromPath(pathname: string): { guildId: string; channelId: string } | null {
  const m = /^\/channels\/([^/]+)\/([^/]+)/.exec(pathname);
  return m ? { guildId: m[1]!, channelId: m[2]! } : null;
}

const TEXTY = new Set([0, 5, 1, 3]);

const STATUS_KEYS = {
  live: 'tools.rooms.statusLive',
  saving: 'tools.rooms.statusSaving',
  offline: 'tools.rooms.statusOffline',
  gone: 'tools.rooms.statusGone',
} as const;

function channelLabel(c: Channel): string {
  if (c.type === 1 || c.type === 3)
    return (c.recipients ?? []).map((u) => u.username).join(', ') || c.name || c.id;
  return `#${c.name ?? c.id}`;
}

function ChannelPicker({
  value,
  onChange,
  guildId,
}: {
  value: string;
  onChange: (id: string) => void;
  guildId: string;
}) {
  const t = useT();
  const here = useChannels(guildId === DM_GUILD ? undefined : guildId);
  const dms = useChannels(DM_GUILD);
  const options = useMemo(
    () => [...here, ...dms].filter((c) => TEXTY.has(c.type) && !c.is_message_request),
    [here, dms],
  );
  return (
    <label className="sr-room-field">
      <span>{t('tools.rooms.channel')}</span>
      <select value={value} onChange={(e) => onChange(e.target.value)}>
        <option value="">{t('tools.rooms.pickChannel')}</option>
        {options.map((c) => (
          <option key={c.id} value={c.id}>
            {channelLabel(c)}
          </option>
        ))}
      </select>
    </label>
  );
}

export interface RoomShellProps {
  kind: ToolRoom['kind'];
  /** The state a new room starts with. */
  starter: () => Record<string, unknown>;
  defaultTitle: string;
  children: (live: LiveRoom, ctx: { channelId: string; room: ToolRoom }) => ReactNode;
}

/**
 * The part both collaborative tools share: choose the channel (the one you are in, by default), pick or start
 * a room of this kind there, and show whether your edits have reached everyone.
 */
export function RoomShell({ kind, starter, defaultTitle, children }: RoomShellProps) {
  const t = useT();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const here = channelFromPath(pathname);
  const [channelId, setChannelId] = useState(here?.channelId ?? '');
  const [rooms, setRooms] = useState<ToolRoom[] | null>(null);
  const [open, setOpen] = useState<ToolRoom | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(
    async (cid: string) => {
      setRooms(null);
      setError(null);
      try {
        setRooms((await listRooms(cid)).filter((r) => r.kind === kind));
      } catch {
        setRooms([]);
        setError(t('tools.rooms.errorLoad'));
      }
    },
    [kind, t],
  );

  useEffect(() => {
    setOpen(null);
    if (channelId) void load(channelId);
    else setRooms(null);
  }, [channelId, load]);

  // Rooms other people start or delete in this channel show up in the list.
  useEffect(
    () =>
      onDispatch((event, data) => {
        if (event !== 'TOOL_ROOM_UPDATE' || !channelId) return;
        const ev = data as ToolRoomEvent;
        if (ev.channel_id === channelId && ev.kind === kind && (ev.created || ev.deleted))
          void load(channelId);
      }),
    [channelId, kind, load],
  );

  const start = async () => {
    setError(null);
    try {
      const room = await createRoom(channelId, { kind, title: defaultTitle, state: starter() });
      setRooms((r) => [room, ...(r ?? [])]);
      setOpen(room);
    } catch {
      setError(t('tools.rooms.errorCreate'));
    }
  };

  const remove = async (room: ToolRoom) => {
    try {
      await deleteRoom(channelId, room.id);
      setOpen(null);
      void load(channelId);
    } catch {
      setError(t('tools.rooms.errorDelete'));
    }
  };

  return (
    <div className="sr-room">
      <ChannelPicker
        value={channelId}
        onChange={setChannelId}
        guildId={here?.guildId ?? DM_GUILD}
      />
      {error && (
        <p role="alert" className="sr-room-error">
          {error}
        </p>
      )}
      {!channelId && <p className="sr-tl-muted">{t('tools.rooms.chooseFirst')}</p>}
      {channelId && !open && (
        <div className="sr-room-list">
          <button
            type="button"
            className="sr-room-btn sr-room-primary"
            onClick={() => void start()}
          >
            {t('tools.rooms.new')}
          </button>
          {rooms === null && <p className="sr-tl-muted">{t('tools.rooms.loading')}</p>}
          {rooms?.length === 0 && <p className="sr-tl-muted">{t('tools.rooms.none')}</p>}
          <ul aria-label={t('tools.rooms.listLabel')}>
            {rooms?.map((r) => (
              <li key={r.id}>
                <button type="button" className="sr-room-btn" onClick={() => setOpen(r)}>
                  {r.title || t('tools.rooms.untitled')}
                </button>
              </li>
            ))}
          </ul>
          <p className="sr-tl-muted">{t('tools.rooms.life')}</p>
        </div>
      )}
      {channelId && open && (
        <Open
          key={open.id}
          room={open}
          channelId={channelId}
          onBack={() => {
            setOpen(null);
            void load(channelId);
          }}
          onDelete={() => void remove(open)}
        >
          {children}
        </Open>
      )}
    </div>
  );
}

function Open({
  room,
  channelId,
  onBack,
  onDelete,
  children,
}: {
  room: ToolRoom;
  channelId: string;
  onBack: () => void;
  onDelete: () => void;
  children: RoomShellProps['children'];
}) {
  const t = useT();
  const live = useLiveRoom(room);
  const { status, view, sync } = live;
  return (
    <div className="sr-room-open">
      <div className="sr-room-bar">
        <button type="button" className="sr-room-btn" onClick={onBack}>
          {t('tools.rooms.back')}
        </button>
        <label className="sr-room-title">
          <span className="sr-visually-hidden">{t('tools.rooms.title')}</span>
          <input
            value={view.title}
            maxLength={100}
            placeholder={t('tools.rooms.untitled')}
            onChange={(e) => live.edit({}, e.target.value)}
          />
        </label>
        <span className="sr-room-status" role="status" data-status={status}>
          {t(STATUS_KEYS[status])}
        </span>
        {status === 'offline' && (
          <button type="button" className="sr-room-btn" onClick={() => sync.retry()}>
            {t('tools.rooms.retry')}
          </button>
        )}
        <button type="button" className="sr-room-btn" onClick={onDelete}>
          {t('tools.rooms.delete')}
        </button>
      </div>
      {status === 'gone' ? (
        <p role="alert" className="sr-room-error">
          {t('tools.rooms.gone')}
        </p>
      ) : (
        children(live, { channelId, room })
      )}
    </div>
  );
}
