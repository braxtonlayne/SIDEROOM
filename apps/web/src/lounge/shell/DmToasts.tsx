import { useToast } from '@sideroom/ui';
import { useEffect } from 'react';
import { useT } from '../../i18n';
import { streamerState, usePrefsStore } from '../../settings/user/prefs';
import { useStore } from '../../store';
import type { Message } from '../../store';
import { onDispatch } from '../../store/events';
import { HOME } from '../channels';
import { useLoungeRoute } from '../nav';
import { desktopDecision } from './desktopNotify';
import { useNotifSettings } from './notifSettings';

/**
 * In-app toast for a direct message that arrives while the window is focused on another channel (LNG-NTF-009).
 * It follows the same rules as desktop notifications (DND, mutes, per-conversation level, Streamer Mode); with the
 * window in the background the desktop notification covers it, so there is never a toast and a notification for the
 * same message. Off with the "Pop-up for direct messages" switch in Settings > Notifications.
 */
export function DmToasts(): null {
  const t = useT();
  const toast = useToast();
  const active = useLoungeRoute().channelId;
  useEffect(() => {
    return onDispatch((event, data) => {
      if (event !== 'MESSAGE_CREATE') return;
      const msg = data as Message;
      // Server messages and restored ones (D-338, not new) raise no toast.
      if (msg.guild_id || msg.restored) return;
      void (async () => {
        const st = useStore.getState();
        const me = st.currentUserId;
        const prefs = usePrefsStore.getState().prefs;
        if (!me || !prefs.dmToasts || streamerState(prefs).silence) return;
        if (typeof document === 'undefined' || !document.hasFocus()) return;
        await useNotifSettings.getState().load(HOME);
        const kind = desktopDecision(msg, {
          me,
          activeChannelId: active,
          windowFocused: true,
          status: st.presences[me]?.status,
          scope: useNotifSettings.getState().byGuild[HOME],
          now: Date.now(),
        });
        if (!kind) return;
        const who = msg.author.global_name ?? msg.author.username;
        toast.show({
          title: t('lounge.dmToast.title', { name: who }),
          description: msg.content.slice(0, 140),
          duration: 6000,
        });
      })();
    });
  }, [active, t, toast]);
  return null;
}
