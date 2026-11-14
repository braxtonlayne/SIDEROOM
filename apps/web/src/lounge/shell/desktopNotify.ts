/**
 * Desktop notifications and sounds for new messages (LNG-NTF-007). The server decides push; this is the
 * in-browser counterpart: it reads the same notification settings (level, mutes, suppress toggles) from the
 * notif store and honours Do Not Disturb, the viewer's notification preferences and the channel they are
 * looking at.
 */
import { useEffect } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { onDispatch } from '../../store/events';
import { useStore } from '../../store';
import { memberKey } from '../../store/reducer';
import type { Message } from '../../store';
import { streamerState, usePrefsStore } from '../../settings/user/prefs';
import { useT } from '../../i18n';
import { HOME } from '../channels';
import { useLoungeRoute } from '../nav';
import { isMuted, useNotifSettings, type GuildNotif } from './notifSettings';
import { hiddenAuthorsNow } from './hiddenAuthors';
import { useNotifDefaults } from './notifDefaults';
import { notificationText, reactionBody, storeResolvers } from './notifyText';
import type { HiddenAuthors } from '../messageRows';

export interface NotifyContext {
  me: string;
  /** Channel the viewer has open in a focused window; its messages never notify. */
  activeChannelId: string | undefined;
  windowFocused: boolean;
  /** The viewer's own presence; Do Not Disturb silences everything. */
  status: string | undefined;
  /** Settings of the message's guild (or `@me` scope for DMs), when loaded. */
  scope: GuildNotif | undefined;
  now: number;
  /** Accounts the viewer blocked or ignores: nothing from them notifies or sounds (D-717). */
  hidden: HiddenAuthors;
  /** The viewer's role ids in the message's server; a pinged role among them is a mention (D-727). Absent: none. */
  roles?: readonly string[];
  /** The channel's category, or for a thread its parent channel (what the push engine passes): its mute and level count. */
  categoryId?: string;
  /** The server's suggested level (`default_message_notifications`); absent: 1 in a server, 0 in a DM. */
  guildDefault?: number;
  /** The person's own default level (`/users/@me/notification-defaults`), when known; null or absent: none. */
  userDefault?: number | null;
}

export type NotifyKind = 'mention' | 'message';

/** Whether `msg` should raise a desktop notification, and which kind of sound it gets. */
export function desktopDecision(msg: Message, c: NotifyContext): NotifyKind | null {
  if (msg.author.id === c.me) return null;
  if (c.hidden.blocked.has(msg.author.id) || c.hidden.ignored.has(msg.author.id)) return null;
  // A restored message (D-338) is not new.
  if (msg.restored) return null;
  if ((msg.type ?? 0) !== 0 && msg.type !== 19) return null;
  if (c.status === 'dnd') return null;
  if (c.windowFocused && c.activeChannelId === msg.channel_id) return null;
  const channel = c.scope?.channels[msg.channel_id];
  const category = c.categoryId ? c.scope?.channels[c.categoryId] : undefined;
  if (isMuted(channel, c.now) || isMuted(category, c.now) || isMuted(c.scope, c.now)) return null;
  const isDm = !msg.guild_id;
  const direct = (msg.mentions ?? []).some((u) => u.id === c.me);
  const everyone = !!msg.mention_everyone && !c.scope?.suppressEveryone;
  const role =
    !c.scope?.suppressRoles && (msg.mention_roles ?? []).some((r) => c.roles?.includes(r));
  const mention = direct || everyone || role;
  // The engine's order (D-727): the channel, its category, the person's choice for the server, the person's own
  // default, the server's suggestion; with none, DMs notify for all and servers for mentions.
  const level =
    channel?.level ??
    category?.level ??
    c.scope?.level ??
    c.userDefault ??
    c.guildDefault ??
    (isDm ? 0 : 1);
  if (level === 2) return null;
  if (level === 1) return mention ? 'mention' : null;
  return mention ? 'mention' : 'message';
}

/** A `MESSAGE_REACTION_ADD` payload, the fields the decision reads. */
export interface ReactionEvent {
  user_id: string;
  channel_id: string;
  message_id: string;
  guild_id?: string | null;
  emoji?: { id?: string | null; name?: string | null };
  /** Discord's field (D-703); absent when the sender did not know the author. */
  message_author_id?: string;
}

export interface ReactionContext extends NotifyContext {
  /** The "Reaction notifications" preference. */
  enabled: boolean;
  /** Streamer Mode silences notifications (LNG-USET-052). */
  silenced: boolean;
}

/** D-703: whether a reaction on the viewer's own message raises a desktop notification (no sound, ever). */
export function reactionDecision(ev: ReactionEvent, c: ReactionContext): boolean {
  if (!c.enabled || c.silenced) return false;
  if (!ev.message_author_id || ev.message_author_id !== c.me) return false;
  if (ev.user_id === c.me) return false;
  if (c.hidden.blocked.has(ev.user_id) || c.hidden.ignored.has(ev.user_id)) return false;
  if (c.status === 'dnd') return false;
  if (c.windowFocused && c.activeChannelId === ev.channel_id) return false;
  const channel = c.scope?.channels[ev.channel_id];
  const category = c.categoryId ? c.scope?.channels[c.categoryId] : undefined;
  return !(isMuted(channel, c.now) || isMuted(category, c.now) || isMuted(c.scope, c.now));
}

let audio: AudioContext | null = null;

/** A short two-tone blip made with Web Audio, so no sound files ship. */
export function playBlip(kind: NotifyKind): void {
  try {
    const Ctor =
      window.AudioContext ??
      (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctor) return;
    audio ??= new Ctor();
    const ctx = audio;
    const tones = kind === 'mention' ? [880, 1175] : [660];
    tones.forEach((hz, i) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.frequency.value = hz;
      gain.gain.setValueAtTime(0.0001, ctx.currentTime + i * 0.12);
      gain.gain.exponentialRampToValueAtTime(0.08, ctx.currentTime + i * 0.12 + 0.01);
      gain.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + i * 0.12 + 0.11);
      osc.connect(gain).connect(ctx.destination);
      osc.start(ctx.currentTime + i * 0.12);
      osc.stop(ctx.currentTime + i * 0.12 + 0.12);
    });
  } catch {
    /* audio blocked or unsupported */
  }
}

/** Subscribes to new messages and raises notifications and sounds per the viewer's preferences. */
export function useDesktopNotify(): void {
  const t = useT();
  const navigate = useNavigate();
  const route = useLoungeRoute();
  const active = route.channelId;
  useEffect(() => {
    return onDispatch((event, data) => {
      if (event === 'MESSAGE_REACTION_ADD') {
        void (async () => {
          const ev = data as ReactionEvent;
          const st = useStore.getState();
          const me = st.currentUserId;
          if (!me || ev.message_author_id !== me) return;
          const prefs = usePrefsStore.getState().prefs;
          const scopeKey = ev.guild_id ?? HOME;
          await useNotifSettings.getState().load(scopeKey);
          const raise = reactionDecision(ev, {
            me,
            activeChannelId: active,
            windowFocused: typeof document !== 'undefined' && document.hasFocus(),
            status: st.presences[me]?.status,
            scope: useNotifSettings.getState().byGuild[scopeKey],
            categoryId: st.channels[ev.channel_id]?.parent_id ?? undefined,
            now: Date.now(),
            enabled: prefs.reactionNotifications,
            silenced: streamerState(prefs).silence,
            hidden: hiddenAuthorsNow(),
          });
          if (
            !raise ||
            !prefs.desktopNotifications ||
            typeof Notification === 'undefined' ||
            Notification.permission !== 'granted'
          )
            return;
          try {
            const u = st.users[ev.user_id];
            const who = u ? (u.global_name ?? u.username) : t('lounge.desktopNotify.someone');
            const e = ev.emoji;
            const emoji = e?.id ? `:${e.name ?? ''}:` : (e?.name ?? '');
            // D-712: the text of the message, read once when it is not loaded (waits 2 seconds at most).
            const body = await reactionBody(
              ev.channel_id,
              ev.message_id,
              ev.guild_id,
              t('lounge.preview.spoiler'),
            );
            const n = new Notification(t('lounge.desktopNotify.reaction', { name: who, emoji }), {
              body,
              tag: `react-${ev.message_id}`,
            });
            n.onclick = () => {
              window.focus();
              void navigate({
                to: '/channels/$guildId/$channelId/$messageId',
                params: {
                  guildId: ev.guild_id ?? HOME,
                  channelId: ev.channel_id,
                  messageId: ev.message_id,
                },
              });
              n.close();
            };
          } catch {
            /* constructing a notification can throw on some mobile browsers */
          }
        })();
        return;
      }
      if (event !== 'MESSAGE_CREATE') return;
      void (async () => {
        const msg = data as Message;
        const st = useStore.getState();
        const me = st.currentUserId;
        if (!me) return;
        const prefs = usePrefsStore.getState().prefs;
        // Streamer Mode (LNG-USET-052) silences sounds and notifications alike.
        if (streamerState(prefs).silence) return;
        const scopeKey = msg.guild_id ?? HOME;
        await useNotifSettings.getState().load(scopeKey);
        const kind = desktopDecision(msg, {
          me,
          activeChannelId: active,
          windowFocused: typeof document !== 'undefined' && document.hasFocus(),
          status: st.presences[me]?.status,
          scope: useNotifSettings.getState().byGuild[scopeKey],
          now: Date.now(),
          hidden: hiddenAuthorsNow(),
          roles: msg.guild_id ? st.members[memberKey(msg.guild_id, me)]?.roles : undefined,
          categoryId: st.channels[msg.channel_id]?.parent_id ?? undefined,
          guildDefault: msg.guild_id
            ? (st.guilds[msg.guild_id] as { default_message_notifications?: number } | undefined)
                ?.default_message_notifications
            : undefined,
          // The person's own default (D-732): undefined until it has been read, as before.
          userDefault: useNotifDefaults.getState().level,
        });
        if (!kind) return;
        if (kind === 'mention' ? prefs.soundMention : prefs.soundMessage) playBlip(kind);
        if (
          !prefs.desktopNotifications ||
          typeof Notification === 'undefined' ||
          Notification.permission !== 'granted'
        )
          return;
        try {
          const who = msg.author.global_name ?? msg.author.username;
          const n = new Notification(
            kind === 'mention' ? t('lounge.desktopNotify.mention', { name: who }) : who,
            {
              body: notificationText(
                msg.content,
                storeResolvers(msg.guild_id),
                t('lounge.preview.spoiler'),
              ),
              tag: `ch-${msg.channel_id}`,
            },
          );
          n.onclick = () => {
            window.focus();
            void navigate({
              to: '/channels/$guildId/$channelId',
              params: { guildId: msg.guild_id ?? HOME, channelId: msg.channel_id },
            });
            n.close();
          };
        } catch {
          /* constructing a notification can throw on some mobile browsers */
        }
      })();
    });
  }, [active, navigate, t]);
}
