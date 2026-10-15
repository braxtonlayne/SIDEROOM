import { useEffect } from 'react';
import { Avatar, Button, Icon } from '@sideroom/ui';
import { useT } from '../../i18n';
import { onDispatch } from '../../store/events';
import {
  applyCallEvent,
  RING_INTERVAL_MS,
  RING_TIMEOUT_MS,
  useCalls,
  type IncomingCall as Call,
} from '../../voice/calls';
import { DM_GUILD_ID, voiceController } from '../../voice/controller';
import { useVoiceSession } from '../../voice/session';
import { FREE_PROFILES } from '../../voice/video';
import { dmTitle } from '../channels';
import { useDms, useUsers } from '../data';
import './voice-p10.css';

/** Accept a ringing call: join it (as a DM call) and, for "Accept with video", turn the camera on once connected. */
export function acceptCall(call: Call, name: string, withVideo: boolean): void {
  const calls = useCalls.getState();
  calls.dismiss(call.channelId);
  const controller = voiceController();
  controller.join({ guildId: DM_GUILD_ID, channelId: call.channelId, name });
  if (!withVideo) return;
  const off = useVoiceSession.subscribe((s) => {
    if (s.channelId !== call.channelId) return off();
    if (s.status === 'connected') {
      off();
      void controller.startCamera(FREE_PROFILES);
    }
  });
}

export function declineCall(call: Call): void {
  useCalls.getState().dismiss(call.channelId);
}

/**
 * The incoming call overlay (VOX-UI-010): the caller's avatar and name, Accept (voice), Accept with video and
 * Decline, with a repeating ringtone. Mounted once at the app root next to the voice bridge. It stops ringing
 * by itself after 30 seconds, and when the server says the call ended.
 */
export function IncomingCallOverlay() {
  const t = useT();
  const incoming = useCalls((s) => s.incoming);
  const users = useUsers();
  const dms = useDms();
  const call = incoming[0];
  const busy = useVoiceSession((s) => s.status !== 'idle');

  useEffect(() => onDispatch((event, data) => applyCallEvent(event, data)), []);

  const channelId = call?.channelId;
  useEffect(() => {
    if (!channelId) return;
    voiceController().playRing();
    const ring = setInterval(() => voiceController().playRing(), RING_INTERVAL_MS);
    const stop = setTimeout(() => useCalls.getState().dismiss(channelId), RING_TIMEOUT_MS);
    return () => {
      clearInterval(ring);
      clearTimeout(stop);
    };
  }, [channelId]);

  if (!call) return null;
  const caller = call.callerId ? users[call.callerId] : undefined;
  const callerName = caller?.displayName ?? t('lounge.incomingCall.someone');
  const dm = dms.find((d) => d.id === call.channelId);
  const title = call.group
    ? dm
      ? dmTitle(dm, (id) => users[id]?.displayName ?? t('lounge.incomingCall.someone'))
      : callerName
    : callerName;
  return (
    <div
      className="vc-incoming"
      role="alertdialog"
      aria-label={t('lounge.incomingCall.label', { name: title })}
      data-testid="incoming-call"
    >
      <Avatar name={callerName} src={caller?.avatarUrl ?? null} size={56} />
      <div className="vc-incoming-text">
        <strong>{title}</strong>
        <span>
          {call.video
            ? t('lounge.incomingCall.videoCalling')
            : call.group
              ? t('lounge.incomingCall.groupCalling', { name: callerName })
              : t('lounge.incomingCall.calling')}
        </span>
        {incoming.length > 1 && (
          <span className="vc-note">
            {t('lounge.incomingCall.more', { n: incoming.length - 1 })}
          </span>
        )}
        {busy && <span className="vc-note">{t('lounge.incomingCall.switchNote')}</span>}
      </div>
      <div className="vc-incoming-actions">
        {/* A video call preselects "Accept with video" (G-40): it is the primary button and comes first. */}
        <Button
          size="sm"
          variant={call.video ? 'secondary' : undefined}
          leadingIcon={<Icon name="phone" size={16} />}
          onClick={() => acceptCall(call, title, false)}
          style={call.video ? { order: 2 } : undefined}
        >
          {t('lounge.incomingCall.accept')}
        </Button>
        <Button
          size="sm"
          variant={call.video ? undefined : 'secondary'}
          leadingIcon={<Icon name="video" size={16} />}
          onClick={() => acceptCall(call, title, true)}
          style={call.video ? { order: 1 } : undefined}
          autoFocus={call.video}
        >
          {t('lounge.incomingCall.acceptVideo')}
        </Button>
        <Button size="sm" variant="danger" onClick={() => declineCall(call)}>
          {t('lounge.incomingCall.decline')}
        </Button>
      </div>
    </div>
  );
}
