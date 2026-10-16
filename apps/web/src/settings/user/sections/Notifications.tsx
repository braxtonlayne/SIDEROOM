import { Button, Notice, Radio, RadioGroup, Select, Switch, useToast } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT, type MessageKey } from '../../../i18n';
import {
  errorText,
  getGuildSettings,
  getNotificationDefaults,
  listGuilds,
  patchGuildSettings,
  patchNotificationDefaults,
  useResource,
  type GuildLite,
  type GuildSettings,
} from '../api';
import { WebPushCard } from '../../../pwa/WebPushCard';
import { Card, Loaded, SectionTitle, Unavailable } from '../parts';
import { KeywordsCard } from './KeywordsCard';
import { PlatformDmCard } from './AccountSwitchCards';
import { PUSH_INACTIVE_CHOICES } from '../prefs';
import type { PrefsFormProps } from './Appearance';

type Perm = NotificationPermission | 'unsupported';
const permission = (): Perm =>
  typeof Notification === 'undefined' ? 'unsupported' : Notification.permission;

/** Mute choices, in seconds (`-1` = until turned back on); the server accepts exactly these. */
export const MUTE_OPTIONS: { value: string; label: MessageKey }[] = [
  { value: 'off', label: 'settings.notifications.muteOff' },
  { value: '900', label: 'settings.notifications.mute15m' },
  { value: '3600', label: 'settings.notifications.mute1h' },
  { value: '10800', label: 'settings.notifications.mute3h' },
  { value: '28800', label: 'settings.notifications.mute8h' },
  { value: '86400', label: 'settings.notifications.mute24h' },
  { value: '-1', label: 'settings.notifications.muteForever' },
];
const LEVEL_OPTIONS: { value: string; label: MessageKey }[] = [
  { value: 'inherit', label: 'settings.notifications.levelInherit' },
  { value: '0', label: 'settings.notifications.levelAll' },
  { value: '1', label: 'settings.notifications.levelMentions' },
  { value: '2', label: 'settings.notifications.levelNothing' },
];

const muteValue = (s: GuildSettings) =>
  !s.muted ? 'off' : String(s.mute_config?.selected_time_window ?? -1);
const levelValue = (s: GuildSettings) =>
  s.message_notifications === null ? 'inherit' : String(s.message_notifications);

function ServerRow({ guild }: { guild: GuildLite }) {
  const t = useT();
  const toast = useToast();
  const res = useResource(() => getGuildSettings(guild.id));
  const [error, setError] = useState<string | null>(null);
  const apply = async (body: Parameters<typeof patchGuildSettings>[1]) => {
    setError(null);
    try {
      const next = await patchGuildSettings(guild.id, body);
      res.setData(next);
      toast.show({
        title: t('settings.notifications.updated', { name: guild.name }),
        duration: 1500,
      });
    } catch (e) {
      setError(errorText(e));
    }
  };
  return (
    <li>
      <div className="us-row-main">
        <strong>{guild.name}</strong>
        {error && (
          <p role="alert" className="us-muted">
            {error}
          </p>
        )}
      </div>
      {res.data ? (
        <div className="us-actions">
          <Select
            aria-label={t('settings.notifications.forAria', { name: guild.name })}
            options={LEVEL_OPTIONS.map((o) => ({ value: o.value, label: t(o.label) }))}
            value={levelValue(res.data)}
            onValueChange={(v) =>
              void apply({ message_notifications: v === 'inherit' ? null : Number(v) })
            }
          />
          <Select
            aria-label={t('settings.notifications.muteAria', { name: guild.name })}
            options={MUTE_OPTIONS.map((o) => ({ value: o.value, label: t(o.label) }))}
            value={muteValue(res.data)}
            onValueChange={(v) =>
              void apply(
                v === 'off'
                  ? { muted: false }
                  : { muted: true, mute_config: { selected_time_window: Number(v) } },
              )
            }
          />
        </div>
      ) : res.loading ? (
        <span className="us-muted">{t('settings.notifications.loading')}</span>
      ) : (
        <span className="us-muted">{errorText(res.error)}</span>
      )}
    </li>
  );
}

const DEFAULT_LEVELS: { value: string; label: MessageKey }[] = [
  { value: '0', label: 'settings.notifications.levelAll' },
  { value: '1', label: 'settings.notifications.levelMentions' },
  { value: '2', label: 'settings.notifications.levelNothing' },
];

/** The account's default level (`/users/@me/notification-defaults`), used where a server doesn't override it. */
function DefaultLevelCard() {
  const t = useT();
  const toast = useToast();
  const res = useResource(getNotificationDefaults);
  const [error, setError] = useState<string | null>(null);
  const choose = async (value: string) => {
    setError(null);
    const before = res.data;
    const level = Number(value);
    res.setData({ message_notifications: level });
    try {
      res.setData(await patchNotificationDefaults(level));
      toast.show({ title: t('settings.notifications.defaultUpdated'), duration: 1500 });
    } catch (e) {
      if (before) res.setData(before);
      setError(errorText(e));
    }
  };
  return (
    <Card
      id="us-notify-default"
      title={t('settings.notifications.defaultTitle')}
      description={t('settings.notifications.defaultDesc')}
    >
      <Loaded res={res} what={t('settings.notifications.defaultWhat')}>
        {(d) => (
          <RadioGroup
            legend={t('settings.notifications.defaultTitle')}
            value={String(d.message_notifications ?? 1)}
            onValueChange={(v) => void choose(v)}
          >
            {DEFAULT_LEVELS.map((l) => (
              <Radio key={l.value} value={l.value} label={t(l.label)} />
            ))}
          </RadioGroup>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title={t('settings.notifications.saveError')}>
          {error}
        </Notice>
      )}
    </Card>
  );
}

export function NotificationsSection({ prefs, setPrefs }: PrefsFormProps) {
  const t = useT();
  const guilds = useResource(listGuilds);
  const [perm, setPerm] = useState<Perm>(permission);
  useEffect(() => setPerm(permission()), []);
  const ask = async () => {
    if (typeof Notification === 'undefined') return;
    const result = await Notification.requestPermission();
    setPerm(result);
    if (result === 'granted') setPrefs({ desktopNotifications: true });
  };
  return (
    <>
      <SectionTitle>{t('settings.notifications.title')}</SectionTitle>
      <DefaultLevelCard />
      <KeywordsCard />
      <Card id="us-desktop" title={t('settings.notifications.desktop')}>
        {perm === 'unsupported' ? (
          <Notice variant="info">{t('settings.notifications.unsupported')}</Notice>
        ) : perm === 'denied' ? (
          <Notice variant="warning" title={t('settings.notifications.blockedTitle')}>
            {t('settings.notifications.blockedBody')}
          </Notice>
        ) : perm === 'default' ? (
          <Button variant="secondary" onClick={() => void ask()}>
            {t('settings.notifications.enable')}
          </Button>
        ) : (
          <Switch
            label={t('settings.notifications.show')}
            checked={prefs.desktopNotifications}
            onChange={(e) => setPrefs({ desktopNotifications: e.target.checked })}
          />
        )}
      </Card>
      <Card id="us-sounds" title={t('settings.notifications.sounds')}>
        <div className="us-stack">
          <Switch
            label={t('settings.notifications.soundMessage')}
            checked={prefs.soundMessage}
            onChange={(e) => setPrefs({ soundMessage: e.target.checked })}
          />
          <Switch
            label={t('settings.notifications.soundMention')}
            checked={prefs.soundMention}
            onChange={(e) => setPrefs({ soundMention: e.target.checked })}
          />
          <Switch
            label={t('settings.notifications.soundCall')}
            checked={prefs.soundCall}
            onChange={(e) => setPrefs({ soundCall: e.target.checked })}
          />
        </div>
      </Card>
      <Card id="us-badges" title={t('settings.notifications.unread')}>
        <div className="us-stack">
          <Switch
            label={t('settings.notifications.badge')}
            description={t('settings.notifications.badgeDesc')}
            checked={prefs.unreadBadge}
            onChange={(e) => setPrefs({ unreadBadge: e.target.checked })}
          />
          <Switch
            label={t('settings.notifications.blink')}
            description={t('settings.notifications.blinkDesc')}
            checked={prefs.titleBlink}
            onChange={(e) => setPrefs({ titleBlink: e.target.checked })}
          />
          <Switch
            label={t('settings.notifications.reactions')}
            checked={prefs.reactionNotifications}
            onChange={(e) => setPrefs({ reactionNotifications: e.target.checked })}
          />
        </div>
      </Card>
      <Card
        id="us-push"
        title={t('settings.notifications.pushTitle')}
        description={t('settings.notifications.pushDesc')}
      >
        <div className="us-stack">
          <label className="us-muted" id="us-push-label">
            {t('settings.notifications.pushAfter')}
          </label>
          <Select
            aria-labelledby="us-push-label"
            options={PUSH_INACTIVE_CHOICES.map((m) => ({
              value: String(m),
              label:
                m === 0
                  ? t('settings.notifications.never')
                  : m === 60
                    ? t('settings.notifications.oneHour')
                    : t('settings.notifications.minutes', { n: m }),
            }))}
            value={String(prefs.pushInactiveMinutes)}
            onValueChange={(v) => setPrefs({ pushInactiveMinutes: Number(v) })}
          />
          <Switch
            label={t('settings.notifications.digest')}
            checked={prefs.emailDigest}
            onChange={(e) => setPrefs({ emailDigest: e.target.checked })}
          />
        </div>
        <Unavailable>{t('settings.notifications.pushUnavailable')}</Unavailable>
      </Card>
      <PlatformDmCard />
      <Card id="us-webpush" title={t('pwa.push.title')}>
        <WebPushCard />
      </Card>
      <Card
        id="us-servers"
        title={t('settings.notifications.serversTitle')}
        description={t('settings.notifications.serversDesc')}
      >
        <Loaded res={guilds} what={t('settings.notifications.serversWhat')}>
          {(rows) =>
            rows.length === 0 ? (
              <p className="us-muted">{t('settings.notifications.noServers')}</p>
            ) : (
              <ul className="us-list">
                {rows.map((g) => (
                  <ServerRow key={g.id} guild={g} />
                ))}
              </ul>
            )
          }
        </Loaded>
      </Card>
    </>
  );
}
