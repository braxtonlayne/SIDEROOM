import { useEffect, useRef, useState } from 'react';
import { Avatar, Button, Icon, Modal, TextInput } from '@sideroom/ui';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { voiceController } from '../../voice/controller';
import { useVoiceSession } from '../../voice/session';
import {
  becomeSpeaker,
  endStage,
  loadStages,
  raiseHand,
  setSpeaker,
  setStageTopic,
  startStage,
  stepDown,
  useStage,
} from '../../voice/stage';
import { useCurrentUser, useUsers, useVoiceMembers, type Channel, type VoiceMember } from '../data';
import { ChannelHeader } from '../ChannelHeader';
import { CHANNEL_ICON } from '../ChannelSidebar';
import { useJoinVoice } from './actions';
import { useVoicePerms } from './perms';
import { ParticipantTile } from './Tiles';
import { VoiceStatusBar } from './VoiceStatusBar';
import './voice-p10.css';
import './voice-p13.css';

/** The longest topic the server accepts. */
export const MAX_TOPIC = 120;

/** Audience members who raised a hand come first, longest waiting first; the rest keep their order. */
export function audienceOrder(members: VoiceMember[]): VoiceMember[] {
  const hands = members
    .filter((m) => m.handRaisedAt != null)
    .sort((a, b) => (a.handRaisedAt ?? 0) - (b.handRaisedAt ?? 0));
  return [...hands, ...members.filter((m) => m.handRaisedAt == null)];
}

function useAction(onError: (m: string) => void) {
  const t = useT();
  return (job: () => Promise<unknown>) => {
    onError('');
    job().catch((e: unknown) => {
      const status = e instanceof ApiError ? e.status : 0;
      onError(
        status === 403
          ? t('lounge.stage.errPermission')
          : e instanceof ApiError && e.message
            ? e.message
            : t('lounge.stage.errOther'),
      );
    });
  };
}

function TopicDialog({
  open,
  initial,
  starting,
  onClose,
  onSubmit,
}: {
  open: boolean;
  initial: string;
  starting: boolean;
  onClose: () => void;
  onSubmit: (topic: string) => void;
}) {
  const t = useT();
  const [topic, setTopic] = useState(initial);
  const ref = useRef<HTMLInputElement | null>(null);
  useEffect(() => {
    if (open) setTopic(initial);
  }, [open, initial]);
  const valid = topic.trim().length > 0 && topic.trim().length <= MAX_TOPIC;
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={starting ? t('lounge.stage.startTitle') : t('lounge.stage.editTitle')}
      description={t('lounge.stage.topicHelp')}
      initialFocusRef={ref as React.RefObject<HTMLElement>}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('lounge.stage.cancel')}
          </Button>
          <Button
            disabled={!valid}
            onClick={() => {
              onSubmit(topic.trim());
              onClose();
            }}
          >
            {starting ? t('lounge.stage.start') : t('lounge.stage.save')}
          </Button>
        </>
      }
    >
      <TextInput
        ref={ref}
        label={t('lounge.stage.topic')}
        value={topic}
        maxLength={MAX_TOPIC}
        onChange={(e) => setTopic(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && valid) {
            onSubmit(topic.trim());
            onClose();
          }
        }}
      />
    </Modal>
  );
}

/**
 * The view of a stage channel (VOX-STG-002): the topic and the live badge, the speakers as tiles, the audience
 * as a list with raised hands first, raise hand / step down, and the moderator's start, edit and end of the
 * stage, invite to speak and move to audience. The backend is VOX-STG-001.
 */
export function StageView({
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
  const perms = useVoicePerms(channel.guildId, channel.id);
  const members = useVoiceMembers(channel.id);
  const instance = useStage((s) => s.instances[channel.id]);
  const [error, setError] = useState('');
  const [dialog, setDialog] = useState<'start' | 'edit' | null>(null);
  const run = useAction(setError);
  const here =
    session.channelId === channel.id && session.status !== 'idle' && session.status !== 'error';
  // Moderators are who may start the stage and invite speakers (Manage Channels + Mute + Move).
  const moderator = perms.manageChannels && perms.muteMembers && perms.moveMembers;
  const controller = voiceController();

  useEffect(() => {
    void loadStages(channel.guildId).catch(() => undefined);
  }, [channel.guildId]);

  const speakers = members.filter((m) => !m.suppressed);
  const audience = audienceOrder(members.filter((m) => m.suppressed));
  const mine = members.find((m) => m.userId === me.id);
  const amSpeaker = here && !!mine && !mine.suppressed;
  const hand = here && mine?.handRaisedAt != null;
  const name = (id: string) => users[id]?.displayName ?? t('lounge.voiceChannel.unknown');
  const mutedNow = session.selfMute || session.serverMute;
  const deafNow = session.selfDeaf || session.serverDeaf;

  return (
    <section className="lng-channel-view vc-view vc-stage-view" aria-label={channel.name}>
      <ChannelHeader
        icon={CHANNEL_ICON[channel.type]}
        title={channel.name}
        isDm={false}
        status={undefined}
        topic={instance?.topic ?? channel.topic}
        membersOpen={false}
        onToggleMembers={() => undefined}
        onBack={onBack}
        searchLabel={t('lounge.voiceChannel.search', { name: channel.name })}
        guildId={channel.guildId}
        channelId={channel.id}
      />
      <VoiceStatusBar channel={channel} here={here} />
      <div className="vc-body">
        <div className="vc-main">
          <div className="vc-stage vc-stage-body">
            <div className="vc-stage-top">
              {instance ? (
                <>
                  <span className="vc-live-badge vc-stage-live">{t('lounge.stage.live')}</span>
                  <h2 className="vc-stage-topic" data-testid="stage-topic">
                    {instance.topic}
                  </h2>
                  {moderator && (
                    <>
                      <Button size="sm" variant="secondary" onClick={() => setDialog('edit')}>
                        {t('lounge.stage.editTopic')}
                      </Button>
                      <Button
                        size="sm"
                        variant="danger"
                        onClick={() => run(() => endStage(channel.id))}
                      >
                        {t('lounge.stage.end')}
                      </Button>
                    </>
                  )}
                </>
              ) : (
                <>
                  <p className="vc-note" role="status">
                    {t('lounge.stage.notRunning')}
                  </p>
                  {moderator && (
                    <Button size="sm" onClick={() => setDialog('start')}>
                      {t('lounge.stage.startButton')}
                    </Button>
                  )}
                </>
              )}
            </div>
            {error && (
              <p role="alert" className="vc-inline-error">
                {error}
              </p>
            )}
            {!here && (
              <div className="vc-stage-join">
                <p className="vc-note">
                  {t('lounge.stage.counts', {
                    speakers: speakers.length,
                    audience: audience.length,
                  })}
                </p>
                <Button disabled={perms.connect === false} onClick={() => join(channel)}>
                  {t('lounge.stage.join')}
                </Button>
              </div>
            )}
            <h3 className="vc-stage-heading">
              {t('lounge.stage.speakers', { n: speakers.length })}
            </h3>
            {speakers.length ? (
              <ul className="vc-grid" aria-label={t('lounge.stage.speakersList')}>
                {speakers.map((p) => (
                  <ParticipantTile
                    key={p.userId}
                    guildId={channel.guildId}
                    channelId={channel.id}
                    participant={{ ...p, video: p.video }}
                    onError={setError}
                    actions={
                      moderator && p.userId !== me.id ? (
                        <button
                          type="button"
                          className="lng-icon-btn"
                          aria-label={t('lounge.stage.moveToAudience', { name: name(p.userId) })}
                          onClick={() => run(() => setSpeaker(channel.guildId, p.userId, false))}
                        >
                          <Icon name="logout" size={16} />
                        </button>
                      ) : undefined
                    }
                  />
                ))}
              </ul>
            ) : (
              <p className="vc-note" role="status">
                {t('lounge.stage.noSpeakers')}
              </p>
            )}
            <h3 className="vc-stage-heading">
              {t('lounge.stage.audience', { n: audience.length })}
            </h3>
            <ul className="vc-audience" aria-label={t('lounge.stage.audienceList')}>
              {audience.map((p) => {
                const u = users[p.userId];
                return (
                  <li
                    key={p.userId}
                    className="vc-audience-row"
                    data-testid={`audience-${p.userId}`}
                    data-hand={p.handRaisedAt != null || undefined}
                  >
                    <Avatar name={name(p.userId)} src={u?.avatarUrl ?? null} size={32} />
                    <span className="vc-audience-name">
                      {name(p.userId)}
                      {p.userId === me.id ? t('lounge.voiceChannel.you') : ''}
                    </span>
                    {p.handRaisedAt != null && (
                      <span
                        className="vc-hand"
                        role="img"
                        aria-label={t('lounge.stage.handRaised')}
                      >
                        {'✋'}
                      </span>
                    )}
                    {moderator && p.userId !== me.id && (
                      <Button
                        size="sm"
                        variant={p.handRaisedAt != null ? 'primary' : 'secondary'}
                        aria-label={t('lounge.stage.invite', { name: name(p.userId) })}
                        onClick={() => run(() => setSpeaker(channel.guildId, p.userId, true))}
                      >
                        {t('lounge.stage.inviteShort')}
                      </Button>
                    )}
                  </li>
                );
              })}
              {!audience.length && (
                <li className="vc-note" role="status">
                  {t('lounge.stage.noAudience')}
                </li>
              )}
            </ul>
          </div>
          {here && (
            <div
              className="vc-controls"
              role="toolbar"
              aria-label={t('lounge.voiceChannel.callControls')}
            >
              <button
                type="button"
                className="vc-ctl"
                aria-label={
                  mutedNow ? t('lounge.voiceChannel.unmute') : t('lounge.voiceChannel.mute')
                }
                aria-pressed={mutedNow}
                disabled={session.serverMute || !amSpeaker}
                title={!amSpeaker ? t('lounge.stage.audienceMic') : undefined}
                data-on={mutedNow || undefined}
                onClick={() => controller.toggleMute()}
              >
                <Icon name={mutedNow ? 'mic-off' : 'mic'} size={24} />
              </button>
              <button
                type="button"
                className="vc-ctl"
                aria-label={
                  deafNow ? t('lounge.voiceChannel.undeafen') : t('lounge.voiceChannel.deafen')
                }
                aria-pressed={deafNow}
                disabled={session.serverDeaf}
                data-on={deafNow || undefined}
                onClick={() => controller.toggleDeafen()}
              >
                <Icon name={deafNow ? 'volume-off' : 'headphones'} size={24} />
              </button>
              {!amSpeaker && !moderator && (
                <Button
                  variant={hand ? 'primary' : 'secondary'}
                  disabled={!perms.requestToSpeak || !instance}
                  aria-pressed={hand}
                  onClick={() => run(() => raiseHand(channel.guildId, channel.id, !hand))}
                >
                  {hand ? t('lounge.stage.lowerHand') : t('lounge.stage.raiseHand')}
                </Button>
              )}
              {!amSpeaker && moderator && (
                <Button onClick={() => run(() => becomeSpeaker(channel.guildId, channel.id))}>
                  {t('lounge.stage.speak')}
                </Button>
              )}
              {amSpeaker && (
                <Button
                  variant="secondary"
                  onClick={() => run(() => stepDown(channel.guildId, channel.id))}
                >
                  {t('lounge.stage.stepDown')}
                </Button>
              )}
              <button
                type="button"
                className="vc-ctl vc-ctl-danger"
                aria-label={t('lounge.voiceChannel.disconnect')}
                onClick={() => controller.leave()}
              >
                <Icon name="logout" size={24} />
              </button>
            </div>
          )}
        </div>
      </div>
      <TopicDialog
        open={dialog !== null}
        initial={dialog === 'edit' ? (instance?.topic ?? '') : ''}
        starting={dialog === 'start'}
        onClose={() => setDialog(null)}
        onSubmit={(topic) =>
          run(() =>
            dialog === 'edit' ? setStageTopic(channel.id, topic) : startStage(channel.id, topic),
          )
        }
      />
    </section>
  );
}
