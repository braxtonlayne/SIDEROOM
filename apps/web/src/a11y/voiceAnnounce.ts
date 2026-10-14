import { useEffect } from 'react';
import { t } from '../i18n';
import { useVoiceSession, type VoiceSession } from '../voice/session';
import { announce } from './announcer';

type Snap = Pick<VoiceSession, 'status' | 'channelName' | 'selfMute' | 'selfDeaf' | 'error'>;

export interface VoiceAnnouncement {
  text: string;
  politeness: 'polite' | 'assertive';
}

/** Pure: what, if anything, a screen reader should hear when the call goes from `prev` to `next`. */
export function voiceAnnouncements(prev: Snap, next: Snap): VoiceAnnouncement[] {
  const out: VoiceAnnouncement[] = [];
  const polite = (text: string) => out.push({ text, politeness: 'polite' });
  const assertive = (text: string) => out.push({ text, politeness: 'assertive' });
  const was = prev.status;
  const now = next.status;
  const live = (s: Snap['status']) => s === 'connected' || s === 'reconnecting';

  if (now !== was) {
    if (now === 'connected' && was === 'reconnecting') polite(t('a11y.announce.voiceReconnected'));
    else if (now === 'connected')
      polite(t('a11y.announce.voiceJoined', { channel: next.channelName ?? '' }));
    else if (now === 'reconnecting') assertive(t('a11y.announce.voiceReconnecting'));
    else if (now === 'error')
      assertive(t(live(was) ? 'a11y.announce.voiceDropped' : 'a11y.announce.voiceFailed'));
    else if (now === 'idle' && live(was)) {
      // A dropped call lands in 'error' first; a plain hang up goes straight to idle.
      polite(t('a11y.announce.voiceLeft'));
    }
  }
  if (now === 'connected' && was === 'connected') {
    if (next.selfMute !== prev.selfMute)
      polite(t(next.selfMute ? 'a11y.announce.micMuted' : 'a11y.announce.micUnmuted'));
    if (next.selfDeaf !== prev.selfDeaf)
      polite(t(next.selfDeaf ? 'a11y.announce.deafened' : 'a11y.announce.undeafened'));
  }
  return out;
}

/** Subscribes to the voice session and announces call events (XC-A11Y-002). Mount once at the app root. */
export function useVoiceAnnouncements(): void {
  useEffect(() => {
    let prev: Snap = useVoiceSession.getState();
    return useVoiceSession.subscribe((next) => {
      for (const a of voiceAnnouncements(prev, next)) announce(a.text, a.politeness);
      prev = next;
    });
  }, []);
}
