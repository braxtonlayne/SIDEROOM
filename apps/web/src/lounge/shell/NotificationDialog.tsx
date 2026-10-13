import { Button, Modal, Notice, RadioGroup, Radio, Spinner, Switch } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useChannels, useGuild } from '../data';
import { isMuted, MUTE_DURATIONS, useNotifSettings } from './notifSettings';

const LEVELS = [
  { value: '0', label: 'All messages' },
  { value: '1', label: 'Only @mentions' },
  { value: '2', label: 'Nothing' },
];

/**
 * Notification Settings for a server, or for one channel in it when `channelId` is set (LNG-RAIL-012,
 * LNG-CHN-013). Changes save as soon as they are made.
 */
export function NotificationDialog({
  guildId,
  channelId,
  onClose,
}: {
  guildId: string;
  channelId?: string | undefined;
  onClose: () => void;
}) {
  const guild = useGuild(guildId);
  const channel = useChannels(guildId).find((c) => c.id === channelId);
  const notif = useNotifSettings((s) => s.byGuild[guildId]);
  const load = useNotifSettings((s) => s.load);
  const patchGuild = useNotifSettings((s) => s.patchGuild);
  const patchChannel = useNotifSettings((s) => s.patchChannel);
  const [error, setError] = useState<string | null>(null);
  const [duration, setDuration] = useState('3600');

  useEffect(() => {
    void load(guildId, true);
  }, [guildId, load]);

  if (!guild) return null;
  const scope = channelId ? notif?.channels[channelId] : notif;
  const muted = isMuted(scope);
  const level = scope?.level ?? null;
  const noun = channelId ? 'channel' : 'server';

  const run = async (p: Promise<void>) => {
    setError(null);
    try {
      await p;
    } catch {
      setError('Could not save that change. Try again.');
    }
  };
  const setMute = (on: boolean) => {
    const mute = on
      ? { muted: true as const, seconds: Number(duration) }
      : { muted: false as const };
    return run(
      channelId ? patchChannel(guildId, channelId, { mute }) : patchGuild(guildId, { mute }),
    );
  };
  const setLevel = (v: number | null) =>
    run(
      channelId
        ? patchChannel(guildId, channelId, { level: v })
        : patchGuild(guildId, { level: v }),
    );

  return (
    <Modal
      open
      onClose={onClose}
      size="sm"
      title={
        channelId
          ? `Notification Settings: #${channel?.name ?? 'channel'}`
          : 'Notification Settings'
      }
      description={guild.name}
      footer={<Button onClick={onClose}>Done</Button>}
    >
      {!notif ? (
        <p role="status">
          <Spinner label="Loading notification settings" />
        </p>
      ) : (
        <div className="lng-form">
          {error && <Notice variant="danger">{error}</Notice>}
          <Switch
            label={`Mute ${channelId ? 'Channel' : 'Server'}`}
            description={`Muted ${noun}s show no unread or mention indicators.`}
            checked={muted}
            onChange={(e) => void setMute(e.target.checked)}
          />
          {!muted && (
            <label className="lng-field">
              <span className="lng-field-label">Mute for</span>
              <select
                className="lng-select"
                value={duration}
                onChange={(e) => setDuration(e.target.value)}
              >
                {MUTE_DURATIONS.map((d) => (
                  <option key={d.seconds} value={String(d.seconds)}>
                    {d.label}
                  </option>
                ))}
              </select>
            </label>
          )}
          <RadioGroup
            legend="Notify me for"
            name="notify-level"
            value={level === null ? '' : String(level)}
            onValueChange={(v) => void setLevel(v === '' ? null : Number(v))}
          >
            {channelId && <Radio value="" label="Use server default" />}
            {LEVELS.map((l) => (
              <Radio key={l.value} value={l.value} label={l.label} />
            ))}
          </RadioGroup>
          {!channelId && (
            <>
              <Switch
                label="Suppress @everyone and @here"
                checked={notif.suppressEveryone}
                onChange={(e) =>
                  void run(patchGuild(guildId, { suppressEveryone: e.target.checked }))
                }
              />
              <Switch
                label="Suppress all role @mentions"
                checked={notif.suppressRoles}
                onChange={(e) => void run(patchGuild(guildId, { suppressRoles: e.target.checked }))}
              />
            </>
          )}
        </div>
      )}
    </Modal>
  );
}
