import { Radio, RadioGroup, TextArea, TextInput } from '@sideroom/ui';
import { useCallback, useMemo } from 'react';
import { snowflakeTime } from '../../../lounge/liveMap';
import { cdnUrl, useChannels } from '../../../store';
import * as api from '../api';
import { useDraft, useGuildEvents, useResource, useSettingsForm } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, NotAvailable, SectionHeader, useAction } from '../ui';
import { ImageField } from './ImageField';

interface Fields {
  name: string;
  description: string;
  system_channel_id: string;
  afk_channel_id: string;
  afk_timeout: number;
  default_message_notifications: number;
  /** Stored hash, a data URI picked this session, or null. */
  icon: string | null;
  banner: string | null;
  splash: string | null;
}

const toFields = (g: api.GuildObject): Fields => ({
  name: g.name,
  description: g.description ?? '',
  system_channel_id: g.system_channel_id ?? '',
  afk_channel_id: g.afk_channel_id ?? '',
  afk_timeout: g.afk_timeout ?? 300,
  default_message_notifications: g.default_message_notifications,
  icon: g.icon,
  banner: g.banner ?? null,
  splash: g.splash ?? null,
});

const COMMUNITY_FLAGS: [keyof api.CommunityObject, string][] = [
  ['community_enabled', 'Community'],
  ['onboarding_enabled', 'Onboarding'],
  ['welcome_screen_enabled', 'Welcome screen'],
  ['guide_enabled', 'Server guide'],
  ['invites_paused', 'Invites paused'],
  ['age_restricted', 'Age-restricted'],
];

export function OverviewSection({ guildId }: SectionProps) {
  const guild = useResource(`guild:${guildId}`, () => api.getGuild(guildId));
  useGuildEvents(guildId, ['GUILD_UPDATE'], () => void guild.reload());
  return (
    <>
      <SectionHeader
        title="Server Overview"
        description="Customize how your server appears in invite links and, if enabled, in Server Discovery and Announcement Channel messages."
      />
      <Async res={guild} label="Loading server">
        {(g) => <OverviewForm guildId={guildId} guild={g} onSaved={(next) => guild.set(next)} />}
      </Async>
    </>
  );
}

function OverviewForm({
  guildId,
  guild,
  onSaved,
}: {
  guildId: string;
  guild: api.GuildObject;
  onSaved: (g: api.GuildObject) => void;
}) {
  const base = useMemo(() => toFields(guild), [guild]);
  const draft = useDraft(base);
  const action = useAction();
  const allChannels = useChannels(guildId);
  const channels = allChannels.filter((c) => c.type === 0 || c.type === 5);
  const voiceChannels = allChannels.filter((c) => c.type === 2);
  const community = useResource(`community:${guildId}`, () => api.getCommunity(guildId));
  const name = draft.value.name.trim();
  const nameError =
    draft.changed.name !== undefined && (name.length < 2 || name.length > 100)
      ? 'Server name must be between 2 and 100 characters.'
      : undefined;

  const save = useCallback(async () => {
    if (nameError) return;
    const c = draft.changed;
    const body: api.ModifyGuildBody = {};
    if (c.name !== undefined) body.name = name;
    if (c.description !== undefined) body.description = c.description.trim() || null;
    if (c.system_channel_id !== undefined) body.system_channel_id = c.system_channel_id || null;
    if (c.afk_channel_id !== undefined) body.afk_channel_id = c.afk_channel_id || null;
    if (c.afk_timeout !== undefined) body.afk_timeout = c.afk_timeout;
    if (c.default_message_notifications !== undefined)
      body.default_message_notifications = c.default_message_notifications;
    if (c.icon !== undefined) body.icon = c.icon;
    if (c.banner !== undefined) body.banner = c.banner;
    if (c.splash !== undefined) body.splash = c.splash;
    const next = await action.run(() => api.modifyGuild(guildId, body), 'Server settings saved.');
    if (next) {
      onSaved(next);
      draft.reset();
    }
  }, [nameError, draft, name, action, guildId, onSaved]);

  useSettingsForm({
    dirty: draft.dirty,
    saving: action.busy,
    save: () => void save(),
    reset: draft.reset,
  });

  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      <TextInput
        label="Server name"
        value={draft.value.name}
        maxLength={100}
        error={nameError}
        onChange={(e) => draft.set('name', e.target.value)}
      />
      <TextArea
        label="Description"
        hint="How did your server get started? Why should people join?"
        value={draft.value.description}
        maxLength={1000}
        onChange={(e) => draft.set('description', e.target.value)}
      />

      <section className="ss-card" aria-labelledby="ss-appearance">
        <h2 id="ss-appearance">Icon, banner and invite background</h2>
        <ImageField
          label="Server icon"
          hint="We recommend an image of at least 512x512. GIFs are allowed."
          ownerId={guildId}
          shape="square"
          value={draft.value.icon}
          onChange={(v) => draft.set('icon', v)}
        />
        <ImageField
          label="Server banner"
          hint="Shown above the channel list. 16:9 works best."
          ownerId={guildId}
          value={draft.value.banner}
          onChange={(v) => draft.set('banner', v)}
        />
        <ImageField
          label="Invite background"
          hint="Shown behind your server on invite links."
          ownerId={guildId}
          value={draft.value.splash}
          onChange={(v) => draft.set('splash', v)}
        />
        <ServerCard guildId={guildId} draft={draft.value} />
      </section>

      <section className="ss-card" aria-labelledby="ss-system">
        <h2 id="ss-system">System messages</h2>
        <div>
          <label className="ss-field-label" htmlFor="ss-system-channel">
            System messages channel
          </label>
          <select
            id="ss-system-channel"
            className="ss-select"
            value={draft.value.system_channel_id}
            onChange={(e) => draft.set('system_channel_id', e.target.value)}
          >
            <option value="">No system messages</option>
            {channels.map((c) => (
              <option key={c.id} value={c.id}>
                #{c.name}
              </option>
            ))}
          </select>
        </div>
        <NotAvailable>
          Switches for the welcome message, sticker reply button and setup tips: the server stores
          only the channel.
        </NotAvailable>
      </section>

      <section className="ss-card" aria-labelledby="ss-afk">
        <h2 id="ss-afk">Inactive channel</h2>
        <p className="ss-sub">
          Members who are idle in voice for the timeout are moved here. Voice chat is not available
          yet, so this is stored for when it is.
        </p>
        <div>
          <label className="ss-field-label" htmlFor="ss-afk-channel">
            Inactive channel
          </label>
          <select
            id="ss-afk-channel"
            className="ss-select"
            value={draft.value.afk_channel_id}
            onChange={(e) => draft.set('afk_channel_id', e.target.value)}
          >
            <option value="">No inactive channel</option>
            {voiceChannels.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="ss-field-label" htmlFor="ss-afk-timeout">
            Inactive timeout
          </label>
          <select
            id="ss-afk-timeout"
            className="ss-select"
            value={String(draft.value.afk_timeout)}
            onChange={(e) => draft.set('afk_timeout', Number(e.target.value))}
          >
            {api.AFK_TIMEOUTS.map((t) => (
              <option key={t.seconds} value={t.seconds}>
                {t.label}
              </option>
            ))}
          </select>
        </div>
      </section>

      <RadioGroup
        legend="Default notification settings"
        value={String(draft.value.default_message_notifications)}
        onValueChange={(v) => draft.set('default_message_notifications', Number(v))}
      >
        <Radio value="0" label="All messages" />
        <Radio value="1" label="Only @mentions" />
      </RadioGroup>

      <section className="ss-card" aria-labelledby="ss-flags">
        <h2 id="ss-flags">Community features</h2>
        <p className="ss-sub">Read-only. Change these in the Community section.</p>
        {community.data ? (
          <ul className="ss-list" aria-label="Community feature flags">
            {COMMUNITY_FLAGS.map(([key, label]) => (
              <li key={key} className="ss-row">
                <span className="ss-grow">{label}</span>
                <span>{community.data![key] ? 'On' : 'Off'}</span>
              </li>
            ))}
          </ul>
        ) : community.error ? (
          <p className="ss-na">{community.error}</p>
        ) : (
          <p className="ss-na">Loading...</p>
        )}
      </section>

      <section className="ss-card" aria-labelledby="ss-not-backed">
        <h2 id="ss-not-backed">Not available yet</h2>
        <NotAvailable>Vanity invite URL: the server stores no vanity code yet.</NotAvailable>
        <NotAvailable>Server tag and member badges.</NotAvailable>
        <NotAvailable>Server traits and the server widget.</NotAvailable>
      </section>
    </div>
  );
}

/** Live preview of how the server shows up on an invite (LNG-SSET-009). */
function ServerCard({ guildId, draft }: { guildId: string; draft: Fields }) {
  const img = (hash: string | null, size: number) =>
    hash ? (hash.startsWith('data:') ? hash : cdnUrl('icons', guildId, hash, { size })) : null;
  const banner = img(draft.banner, 512);
  const icon = img(draft.icon, 128);
  const created = snowflakeTime(guildId);
  const est = created
    ? new Date(created).toLocaleDateString(undefined, { month: 'short', year: 'numeric' })
    : '';
  const name = draft.name.trim() || 'Your server';
  return (
    <div className="ss-server-card" role="group" aria-label="Server card preview">
      <div className="ss-server-banner">{banner && <img src={banner} alt="" />}</div>
      <div className="ss-server-meta">
        <span className="ss-server-icon" aria-hidden="true">
          {icon ? <img src={icon} alt="" /> : name.charAt(0).toUpperCase()}
        </span>
        <div>
          <strong>{name}</strong>
          {est && <div className="ss-sub">Est. {est}</div>}
        </div>
      </div>
    </div>
  );
}
