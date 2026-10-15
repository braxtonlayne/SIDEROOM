import { useEffect } from 'react';
import { t } from '../i18n';
import { streamerState, usePrefsStore } from '../settings/user/prefs';
import { useStore } from '../store';
import { onDispatch } from '../store/events';
import type { Message } from '../store/types';

/** A message worth interrupting for: a direct message (no server) or one that names the viewer. */
export function shouldAlert(m: Message, me: string | null): boolean {
  if (!me || m.author?.id === me) return false;
  if (m.type !== undefined && m.type !== 0 && m.type !== 19) return false;
  return !m.guild_id || !!m.mention_everyone || !!m.mentions?.some((u) => u.id === me);
}

/**
 * The blinking tab title from the Notifications settings (LNG-USET-052): DMs and mentions blink the title
 * while the window is in the background, until it is focused again. Streamer Mode silences it. Browser
 * notifications and sounds are raised by the Lounge's `useDesktopNotify` (LNG-NTF-007), which applies the
 * per-server levels and mutes, so a message never produces two notifications.
 */
export function useDesktopAlerts(): void {
  useEffect(() => {
    const original = document.title;
    let timer: ReturnType<typeof setInterval> | null = null;
    const stopBlink = () => {
      if (timer) clearInterval(timer);
      timer = null;
      document.title = original;
    };
    const blink = (text: string) => {
      if (timer) return;
      let on = false;
      timer = setInterval(() => {
        on = !on;
        document.title = on ? text : original;
      }, 1000);
    };
    const onFocus = () => stopBlink();
    window.addEventListener('focus', onFocus);
    const off = onDispatch((event, data) => {
      if (event !== 'MESSAGE_CREATE') return;
      if (typeof document.hasFocus === 'function' && document.hasFocus()) return;
      const m = data as Message;
      if (!shouldAlert(m, useStore.getState().currentUserId)) return;
      const prefs = usePrefsStore.getState().prefs;
      if (streamerState(prefs).silence) return;
      const who = m.author?.global_name || m.author?.username || '';
      if (prefs.titleBlink) blink(t('pwa.alerts.blink', { name: who }));
    });
    return () => {
      off();
      window.removeEventListener('focus', onFocus);
      stopBlink();
    };
  }, []);
}
