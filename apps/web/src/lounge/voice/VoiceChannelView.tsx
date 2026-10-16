import { useRef, useState, type ReactNode } from 'react';
import { Button, EmptyState } from '@sideroom/ui';
import { useT } from '../../i18n';
import { useVoiceSession } from '../../voice/session';
import { useLocalVideo, useRemoteVideo } from '../../voice/video';
import { ChannelHeader } from '../ChannelHeader';
import { CHANNEL_ICON } from '../ChannelSidebar';
import { useCurrentUser, useUsers, useVoiceMembers, type Channel } from '../data';
import { useJoinVoice } from './actions';
import { CallControls, type ViewLayout } from './CallControls';
import { JoinPreview } from './JoinPreview';
import { ParticipantTile, StreamTile } from './Tiles';
import { useVoicePerms } from './perms';
import { usePopOut } from './PopOut';
import { useSwipeDown } from './swipe';
import { VoiceChat } from './VoiceChat';
import { VoiceStatusBar } from './VoiceStatusBar';
import { WatchPartyStrip } from '../../watchparty/WatchPartyStrip';
import { ActivityPanel } from '../../activities/ActivityPanel';
import './voice-p10.css';

interface TileItem {
  key: string;
  userId: string;
  kind: 'person' | 'stream';
}

/**
 * The view of a server voice channel: join screen with a camera preview, the tile stage (grid, focus mode with
 * a strip of the other tiles, hide non-video, full screen), the text-in-voice panel and the call controls
 * (VOX-UI-002/003/004/005/007).
 */
export function VoiceChannelView({
  channel,
  onBack,
}: {
  channel: Channel;
  onBack?: (() => void) | undefined;
}) {
  const t = useT();
  const join = useJoinVoice();
  const session = useVoiceSession();
  const me = useCurrentUser();
  const users = useUsers();
  const participants = useVoiceMembers(channel.id);
  const perms = useVoicePerms(channel.guildId, channel.id);
  const localCamera = useLocalVideo((s) => s.camera);
  const remoteCamera = useRemoteVideo((s) => s.camera);
  const here =
    session.channelId === channel.id && session.status !== 'idle' && session.status !== 'error';
  const full = !!channel.userLimit && participants.length >= channel.userLimit && !here;
  const [error, setError] = useState<string | null>(null);
  const [layout, setLayout] = useState<ViewLayout>({
    mode: 'grid',
    hideNonVideo: false,
    chatOpen: false,
  });
  const [focusKey, setFocusKey] = useState<string | null>(null);
  const stage = useRef<HTMLDivElement | null>(null);
  const popOut = usePopOut(channel.id);
  // Mobile: swipe down on the call to minimise it (the call keeps running in the voice panel).
  useSwipeDown(stage, here ? onBack : undefined);

  const patchLayout = (patch: Partial<ViewLayout>) => setLayout((l) => ({ ...l, ...patch }));
  const fullscreen = () => {
    const el = stage.current;
    if (!el?.requestFullscreen) return setError(t('lounge.voiceStream.noFullscreen'));
    void el.requestFullscreen().catch(() => setError(t('lounge.voiceStream.noFullscreen')));
  };

  const live = (userId: string, streaming?: boolean) =>
    userId === me.id ? session.screen : !!streaming;
  const hasVideo = (userId: string, video?: boolean, streaming?: boolean) =>
    userId === me.id
      ? !!(session.camera && localCamera) || session.screen
      : !!(video || streaming || remoteCamera[userId]);

  // Streams first, then people; hiding non-video keeps everyone who has a camera or a share.
  const people = participants.filter(
    (p) => !layout.hideNonVideo || !here || hasVideo(p.userId, p.video, p.streaming),
  );
  const items: TileItem[] = [
    ...participants
      .filter((p) => here && live(p.userId, p.streaming))
      .map((p): TileItem => ({ key: `stream:${p.userId}`, userId: p.userId, kind: 'stream' })),
    ...people.map((p): TileItem => ({ key: p.userId, userId: p.userId, kind: 'person' })),
  ];
  const spotlight =
    layout.mode === 'focus' && items.length > 1
      ? (items.find((i) => i.key === focusKey) ?? items[0])
      : undefined;

  const nameOf = (userId: string) => users[userId]?.displayName ?? t('lounge.voiceChannel.unknown');
  const render = (item: TileItem): ReactNode => {
    if (item.kind === 'stream')
      return (
        <StreamTile
          key={item.key}
          guildId={channel.guildId}
          channelId={channel.id}
          userId={item.userId}
          name={nameOf(item.userId)}
          onError={setError}
        />
      );
    const p = participants.find((x) => x.userId === item.userId);
    if (!p) return null;
    const isFocus = spotlight?.key === item.key;
    return (
      <ParticipantTile
        key={item.key}
        guildId={channel.guildId}
        channelId={channel.id}
        participant={p}
        onError={setError}
        focused={isFocus}
        onToggleFocus={
          here
            ? () => {
                if (isFocus) patchLayout({ mode: 'grid' });
                else {
                  setFocusKey(item.key);
                  patchLayout({ mode: 'focus' });
                }
              }
            : undefined
        }
      />
    );
  };
  const gridLabel = here
    ? t('lounge.voiceChannel.inChannel', { name: channel.name })
    : t('lounge.voiceChannel.inThisChannel');

  return (
    <section className="lng-channel-view vc-view" aria-label={channel.name}>
      <ChannelHeader
        icon={CHANNEL_ICON[channel.type]}
        title={channel.name}
        isDm={false}
        status={undefined}
        topic={channel.topic}
        membersOpen={false}
        onToggleMembers={() => undefined}
        onBack={onBack}
        searchLabel={t('lounge.voiceChannel.search', { name: channel.name })}
        guildId={channel.guildId}
        channelId={channel.id}
      />
      <VoiceStatusBar channel={channel} here={here} />
      <WatchPartyStrip channelId={channel.id} />
      <ActivityPanel channelId={channel.id} channelName={channel.name} here={here} />
      <div className="vc-body" data-chat={layout.chatOpen && here ? 'open' : undefined}>
        <div className="vc-main">
          <div className="vc-stage" ref={stage}>
            {error && (
              <p role="alert" className="vc-inline-error">
                {error}
              </p>
            )}
            {!here ? (
              <div className="vc-join">
                <EmptyState
                  title={channel.name}
                  description={
                    participants.length
                      ? t('lounge.voiceChannel.peopleHere', { n: participants.length })
                      : t('lounge.voiceChannel.nobodyHere')
                  }
                  action={
                    <Button
                      onClick={() => join(channel)}
                      disabled={full || perms.connect === false}
                    >
                      {full ? t('lounge.voiceChannel.full') : t('lounge.voiceChannel.join')}
                    </Button>
                  }
                />
                <JoinPreview />
                {!perms.useVad && <p className="vc-note">{t('lounge.voiceChannel.pttRequired')}</p>}
                {participants.length > 0 && (
                  <ul className="vc-grid" aria-label={gridLabel}>
                    {items.map(render)}
                  </ul>
                )}
              </div>
            ) : spotlight ? (
              <div className="vc-focus">
                <ul className="vc-spotlight" aria-label={gridLabel}>
                  {render(spotlight)}
                </ul>
                <ul className="vc-strip" aria-label={t('lounge.voiceChannel.otherTiles')}>
                  {items.filter((i) => i.key !== spotlight.key).map(render)}
                </ul>
              </div>
            ) : items.length ? (
              <ul className="vc-grid" aria-label={gridLabel}>
                {items.map(render)}
              </ul>
            ) : (
              <p className="vc-note" role="status">
                {t('lounge.voiceChannel.noVideo')}
              </p>
            )}
          </div>
          {here && (
            <CallControls
              channelId={channel.id}
              guildId={channel.guildId}
              layout={layout}
              onLayout={patchLayout}
              onFullscreen={fullscreen}
              onPopOut={
                popOut.supported && !popOut.isOpen
                  ? () => void popOut.open().catch(() => setError(t('lounge.popOut.failed')))
                  : undefined
              }
            />
          )}
        </div>
        {here && layout.chatOpen && (
          <VoiceChat channel={channel} onClose={() => patchLayout({ chatOpen: false })} />
        )}
      </div>
      {popOut.portal}
    </section>
  );
}
