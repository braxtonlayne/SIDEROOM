import { Button, Notice, Radio, RadioGroup, Select, Switch, useToast } from '@sideroom/ui';
import { useEffect, useState } from 'react';
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
import { Card, Loaded, SectionTitle, Unavailable } from '../parts';
import { PUSH_INACTIVE_CHOICES } from '../prefs';
import type { PrefsFormProps } from './Appearance';

type Perm = NotificationPermission | 'unsupported';
const permission = (): Perm =>
  typeof Notification === 'undefined' ? 'unsupported' : Notification.permission;

/** Mute choices, in seconds (`-1` = until turned back on); the server accepts exactly these. */
export const MUTE_OPTIONS = [
  { value: 'off', label: 'Not muted' },
  { value: '900', label: 'For 15 minutes' },
  { value: '3600', label: 'For 1 hour' },
  { value: '10800', label: 'For 3 hours' },
  { value: '28800', label: 'For 8 hours' },
  { value: '86400', label: 'For 24 hours' },
  { value: '-1', label: 'Until I turn it back on' },
];
const LEVEL_OPTIONS = [
  { value: 'inherit', label: 'Server default' },
  { value: '0', label: 'All messages' },
  { value: '1', label: 'Only @mentions' },
  { value: '2', label: 'Nothing' },
];

const muteValue = (s: GuildSettings) =>
  !s.muted ? 'off' : String(s.mute_config?.selected_time_window ?? -1);
const levelValue = (s: GuildSettings) =>
  s.message_notifications === null ? 'inherit' : String(s.message_notifications);

function ServerRow({ guild }: { guild: GuildLite }) {
  const toast = useToast();
  const res = useResource(() => getGuildSettings(guild.id));
  const [error, setError] = useState<string | null>(null);
  const apply = async (body: Parameters<typeof patchGuildSettings>[1]) => {
    setError(null);
    try {
      const next = await patchGuildSettings(guild.id, body);
      res.setData(next);
      toast.show({ title: `Updated ${guild.name}`, duration: 1500 });
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
            aria-label={`Notifications for ${guild.name}`}
            options={LEVEL_OPTIONS}
            value={levelValue(res.data)}
            onValueChange={(v) =>
              void apply({ message_notifications: v === 'inherit' ? null : Number(v) })
            }
          />
          <Select
            aria-label={`Mute ${guild.name}`}
            options={MUTE_OPTIONS}
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
        <span className="us-muted">Loading...</span>
      ) : (
        <span className="us-muted">{errorText(res.error)}</span>
      )}
    </li>
  );
}

const DEFAULT_LEVELS = [
  { value: '0', label: 'All messages' },
  { value: '1', label: 'Only @mentions' },
  { value: '2', label: 'Nothing' },
];

/** The account's default level (`/users/@me/notification-defaults`), used where a server doesn't override it. */
function DefaultLevelCard() {
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
      toast.show({ title: 'Default level updated', duration: 1500 });
    } catch (e) {
      if (before) res.setData(before);
      setError(errorText(e));
    }
  };
  return (
    <Card
      id="us-notify-default"
      title="Default notification level"
      description="Used in every server that doesn't set its own. Saves as soon as you pick."
    >
      <Loaded res={res} what="your default level">
        {(d) => (
          <RadioGroup
            legend="Default notification level"
            value={String(d.message_notifications ?? 1)}
            onValueChange={(v) => void choose(v)}
          >
            {DEFAULT_LEVELS.map((l) => (
              <Radio key={l.value} value={l.value} label={l.label} />
            ))}
          </RadioGroup>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title="Couldn't save that">
          {error}
        </Notice>
      )}
    </Card>
  );
}

export function NotificationsSection({ prefs, setPrefs }: PrefsFormProps) {
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
      <SectionTitle>Notifications</SectionTitle>
      <DefaultLevelCard />
      <Card id="us-desktop" title="Desktop notifications">
        {perm === 'unsupported' ? (
          <Notice variant="info">This browser doesn&apos;t support desktop notifications.</Notice>
        ) : perm === 'denied' ? (
          <Notice variant="warning" title="Blocked by the browser">
            Allow notifications for this site in your browser&apos;s site settings, then come back.
          </Notice>
        ) : perm === 'default' ? (
          <Button variant="secondary" onClick={() => void ask()}>
            Enable desktop notifications
          </Button>
        ) : (
          <Switch
            label="Show desktop notifications"
            checked={prefs.desktopNotifications}
            onChange={(e) => setPrefs({ desktopNotifications: e.target.checked })}
          />
        )}
      </Card>
      <Card id="us-sounds" title="Sounds">
        <div className="us-stack">
          <Switch
            label="Message sound"
            checked={prefs.soundMessage}
            onChange={(e) => setPrefs({ soundMessage: e.target.checked })}
          />
          <Switch
            label="Mention sound"
            checked={prefs.soundMention}
            onChange={(e) => setPrefs({ soundMention: e.target.checked })}
          />
          <Switch
            label="Incoming call sound"
            checked={prefs.soundCall}
            onChange={(e) => setPrefs({ soundCall: e.target.checked })}
          />
        </div>
      </Card>
      <Card id="us-badges" title="Unread indicators">
        <div className="us-stack">
          <Switch
            label="Show the unread badge"
            description="A count on the browser tab and the installed app icon."
            checked={prefs.unreadBadge}
            onChange={(e) => setPrefs({ unreadBadge: e.target.checked })}
          />
          <Switch
            label="Flash the page title for new messages"
            description="The tab title blinks while the window is in the background."
            checked={prefs.titleBlink}
            onChange={(e) => setPrefs({ titleBlink: e.target.checked })}
          />
          <Switch
            label="Notify me about reactions to my messages"
            checked={prefs.reactionNotifications}
            onChange={(e) => setPrefs({ reactionNotifications: e.target.checked })}
          />
        </div>
      </Card>
      <Card
        id="us-push"
        title="Push and email"
        description="When you step away, notifications can go to your phone, or arrive as a summary."
      >
        <div className="us-stack">
          <label className="us-muted" id="us-push-label">
            Send push notifications after being inactive for
          </label>
          <Select
            aria-labelledby="us-push-label"
            options={PUSH_INACTIVE_CHOICES.map((m) => ({
              value: String(m),
              label: m === 0 ? 'Never' : m === 60 ? '1 hour' : `${m} minute${m === 1 ? '' : 's'}`,
            }))}
            value={String(prefs.pushInactiveMinutes)}
            onValueChange={(v) => setPrefs({ pushInactiveMinutes: Number(v) })}
          />
          <Switch
            label="Email me a digest of what I missed"
            checked={prefs.emailDigest}
            onChange={(e) => setPrefs({ emailDigest: e.target.checked })}
          />
        </div>
        <Unavailable>
          These choices are kept on your account, but the server doesn&apos;t deliver Web Push or
          email digests yet, so they don&apos;t change anything today.
        </Unavailable>
      </Card>
      <Card
        id="us-servers"
        title="Server overrides"
        description="Per-server notification level and mute. These save immediately."
      >
        <Loaded res={guilds} what="your servers">
          {(rows) =>
            rows.length === 0 ? (
              <p className="us-muted">You aren&apos;t in any servers.</p>
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
