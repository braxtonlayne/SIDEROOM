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
import type { Message } from '../../store';
import { streamerState, usePrefsStore } from '../../settings/user/prefs';
import { useT } from '../../i18n';
import { HOME } from '../channels';
import { useLoungeRoute } from '../nav';
import { isMuted, useNotifSettings, type GuildNotif } from './notifSettings';
import { useSocial } from './social';

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
}

export type NotifyKind = 'mention' | 'message';

/** Whether `msg` should raise a desktop notification, and which kind of sound it gets. */
export function desktopDecision(msg: Message, c: NotifyContext): NotifyKind | null {
  if (msg.author.id === c.me) return null;
  // A restored message (D-338) is not new.
  if (msg.restored) return null;
  if ((msg.type ?? 0) !== 0 && msg.type !== 19) return null;
  if (c.status === 'dnd') return null;
  if (c.windowFocused && c.activeChannelId === msg.channel_id) return null;
  const channel = c.scope?.channels[msg.channel_id];
  if (isMuted(channel, c.now) || isMuted(c.scope, c.now)) return null;
  const isDm = !msg.guild_id;
  const direct = (msg.mentions ?? []).some((u) => u.id === c.me);
  const everyone = !!msg.mention_everyone && !c.scope?.suppressEveryone;
  const mention = direct || everyone;
  // Explicit channel choice, then the server choice; unset means DMs notify for all, servers for mentions.
  const level = channel?.level ?? c.scope?.level ?? (isDm ? 0 : 1);
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
  /** Accounts the viewer has blocked. */
  blocked: ReadonlySet<string>;
}

/** D-703: whether a reaction on the viewer's own message raises a desktop notification (no sound, ever). */
export function reactionDecision(ev: ReactionEvent, c: ReactionContext): boolean {
  if (!c.enabled || c.silenced) return false;
  if (!ev.message_author_id || ev.message_author_id !== c.me) return false;
  if (ev.user_id === c.me || c.blocked.has(ev.user_id)) return false;
  if (c.status === 'dnd') return false;
  if (c.windowFocused && c.activeChannelId === ev.channel_id) return false;
  const channel = c.scope?.channels[ev.channel_id];
  return !(isMuted(channel, c.now) || isMuted(c.scope, c.now));
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
            now: Date.now(),
            enabled: prefs.reactionNotifications,
            silenced: streamerState(prefs).silence,
            blocked: new Set(
              useSocial
                .getState()
                .relationships.filter((r) => r.type === 2)
                .map((r) => r.id),
            ),
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
            const n = new Notification(t('lounge.desktopNotify.reaction', { name: who, emoji }), {
              body: (st.messages[ev.message_id]?.content ?? '').slice(0, 140),
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
            { body: msg.content.slice(0, 140), tag: `ch-${msg.channel_id}` },
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
