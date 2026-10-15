import { Button, Modal, Notice, RadioGroup, Radio, Spinner, Switch } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT, type MessageKey } from '../../i18n';
import { useChannels, useGuild } from '../data';
import { isMuted, MUTE_DURATIONS, useNotifSettings, type GuildNotif } from './notifSettings';

const LEVELS: { value: string; labelKey: MessageKey }[] = [
  { value: '0', labelKey: 'lounge.notificationDialog.levelAll' },
  { value: '1', labelKey: 'lounge.notificationDialog.levelMentions' },
  { value: '2', labelKey: 'lounge.notificationDialog.levelNothing' },
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
  const t = useT();
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

  const run = async (p: Promise<void>) => {
    setError(null);
    try {
      await p;
    } catch {
      setError(t('lounge.notificationDialog.saveError'));
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
          ? t('lounge.notificationDialog.channelTitle', {
              name: channel?.name ?? t('lounge.notificationDialog.channelFallback'),
            })
          : t('lounge.notificationDialog.title')
      }
      description={guild.name}
      footer={<Button onClick={onClose}>{t('lounge.notificationDialog.done')}</Button>}
    >
      {!notif ? (
        <p role="status">
          <Spinner label={t('lounge.notificationDialog.loading')} />
        </p>
      ) : (
        <div className="lng-form">
          {error && <Notice variant="danger">{error}</Notice>}
          <Switch
            label={
              channelId
                ? t('lounge.notificationDialog.muteChannel')
                : t('lounge.notificationDialog.muteServer')
            }
            description={
              channelId
                ? t('lounge.notificationDialog.mutedChannelsHint')
                : t('lounge.notificationDialog.mutedServersHint')
            }
            checked={muted}
            onChange={(e) => void setMute(e.target.checked)}
          />
          {!muted && (
            <label className="lng-field">
              <span className="lng-field-label">{t('lounge.notificationDialog.muteFor')}</span>
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
            legend={t('lounge.notificationDialog.notifyFor')}
            name="notify-level"
            value={level === null ? '' : String(level)}
            onValueChange={(v) => void setLevel(v === '' ? null : Number(v))}
          >
            {channelId && <Radio value="" label={t('lounge.notificationDialog.useDefault')} />}
            {LEVELS.map((l) => (
              <Radio key={l.value} value={l.value} label={t(l.labelKey)} />
            ))}
          </RadioGroup>
          {!channelId && (
            <>
              <Switch
                label={t('lounge.notificationDialog.suppressEveryone')}
                checked={notif.suppressEveryone}
                onChange={(e) =>
                  void run(patchGuild(guildId, { suppressEveryone: e.target.checked }))
                }
              />
              <Switch
                label={t('lounge.notificationDialog.suppressRoles')}
                checked={notif.suppressRoles}
                onChange={(e) => void run(patchGuild(guildId, { suppressRoles: e.target.checked }))}
              />
              <Switch
                label={t('lounge.notificationDialog.suppressHighlights')}
                checked={notif.suppressHighlights}
                onChange={(e) =>
                  void run(patchGuild(guildId, { suppressHighlights: e.target.checked }))
                }
              />
              <Switch
                label={t('lounge.notificationDialog.mobilePush')}
                checked={notif.mobilePush}
                onChange={(e) => void run(patchGuild(guildId, { mobilePush: e.target.checked }))}
              />
              <Overrides
                guildId={guildId}
                notif={notif}
                onError={() => setError(t('lounge.notificationDialog.saveError'))}
              />
            </>
          )}
        </div>
      )}
    </Modal>
  );
}

const LEVEL_KEYS: Record<number, MessageKey> = {
  0: 'lounge.notificationDialog.levelAll',
  1: 'lounge.notificationDialog.levelMentions',
  2: 'lounge.notificationDialog.levelNothing',
};

/** Channels with their own setting, each with a reset (LNG-NTF-004). */
function Overrides({
  guildId,
  notif,
  onError,
}: {
  guildId: string;
  notif: GuildNotif;
  onError: () => void;
}) {
  const t = useT();
  const channels = useChannels(guildId);
  const patchChannel = useNotifSettings((s) => s.patchChannel);
  const rows = Object.entries(notif.channels).filter(([, c]) => c.level !== null || isMuted(c));
  return (
    <fieldset className="lng-field">
      <legend className="lng-field-label">{t('lounge.notificationDialog.overrides')}</legend>
      {rows.length === 0 ? (
        <p className="lng-muted">{t('lounge.notificationDialog.noOverrides')}</p>
      ) : (
        <ul className="lng-notif-overrides">
          {rows.map(([id, c]) => {
            const name = channels.find((ch) => ch.id === id)?.name ?? id;
            return (
              <li key={id}>
                <span>
                  #{name}
                  {': '}
                  {c.level !== null && LEVEL_KEYS[c.level] ? t(LEVEL_KEYS[c.level]!) : null}
                  {isMuted(c) ? ` ${t('lounge.notificationDialog.overrideMuted')}` : null}
                </span>
                <Button
                  variant="ghost"
                  aria-label={t('lounge.notificationDialog.resetOverride', { name })}
                  onClick={() =>
                    void patchChannel(guildId, id, { level: null, mute: { muted: false } }).catch(
                      onError,
                    )
                  }
                >
                  {t('lounge.notificationDialog.reset')}
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </fieldset>
  );
}
