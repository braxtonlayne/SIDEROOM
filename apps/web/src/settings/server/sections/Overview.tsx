import { Button, Radio, RadioGroup, TextArea, TextInput } from '@sideroom/ui';
import { useCallback, useMemo } from 'react';
import { useChannels } from '../../../store';
import * as api from '../api';
import { useDraft, useGuildEvents, useResource, useSettingsForm } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, NotAvailable, SectionHeader, useAction } from '../ui';

interface Fields {
  name: string;
  description: string;
  system_channel_id: string;
  default_message_notifications: number;
}

const toFields = (g: api.GuildObject): Fields => ({
  name: g.name,
  description: g.description ?? '',
  system_channel_id: g.system_channel_id ?? '',
  default_message_notifications: g.default_message_notifications,
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
  const channels = useChannels(guildId).filter((c) => c.type === 0 || c.type === 5);
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
    if (c.default_message_notifications !== undefined)
      body.default_message_notifications = c.default_message_notifications;
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
        <h2 id="ss-appearance">Icon and banner</h2>
        <p className="ss-sub">We recommend an image of at least 512x512 for the server icon.</p>
        <div className="ss-actions">
          <Button variant="secondary" disabled aria-describedby="ss-media-soon">
            Change server icon
          </Button>
          <Button variant="secondary" disabled aria-describedby="ss-media-soon">
            Upload banner
          </Button>
        </div>
        <p id="ss-media-soon" className="ss-na">
          Coming soon: image uploads arrive with the media pipeline.
        </p>
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
      </section>

      <section className="ss-card" aria-labelledby="ss-afk">
        <h2 id="ss-afk">Inactive channel</h2>
        <NotAvailable>
          Voice AFK channel and timeout need server support that has not shipped.
        </NotAvailable>
        <div>
          <label className="ss-field-label" htmlFor="ss-afk-channel">
            Inactive channel
          </label>
          <select id="ss-afk-channel" className="ss-select" disabled value="">
            <option value="">No inactive channel</option>
          </select>
        </div>
        <div>
          <label className="ss-field-label" htmlFor="ss-afk-timeout">
            Inactive timeout
          </label>
          <select id="ss-afk-timeout" className="ss-select" disabled value="300">
            <option value="300">5 minutes</option>
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
    </div>
  );
}
