import { Button, Notice, Radio, RadioGroup, Select, Switch, useToast } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import {
  errorText,
  getGuildSettings,
  listGuilds,
  patchGuildSettings,
  useResource,
  type GuildLite,
  type GuildSettings,
} from '../api';
import { Card, Loaded, SectionTitle } from '../parts';
import type { DefaultNotify } from '../prefs';
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
      <Card
        id="us-notify-default"
        title="Default notification level"
        description="Used where a server doesn't override it. Stored in this browser; the server has no global default yet."
      >
        <RadioGroup
          legend="Default notification level"
          value={prefs.defaultNotify}
          onValueChange={(v) => setPrefs({ defaultNotify: v as DefaultNotify })}
        >
          <Radio value="all" label="All messages" />
          <Radio value="mentions" label="Only @mentions" />
          <Radio value="nothing" label="Nothing" />
        </RadioGroup>
      </Card>
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
