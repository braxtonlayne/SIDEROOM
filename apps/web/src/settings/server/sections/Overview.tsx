import { Radio, RadioGroup, TextArea, TextInput } from '@sideroom/ui';
import { useCallback, useMemo } from 'react';
import { formatDate, type MessageKey, useT } from '../../../i18n';
import { snowflakeTime } from '../../../lounge/liveMap';
import { cdnUrl, useChannels } from '../../../store';
import * as api from '../api';
import { useDraft, useGuildEvents, useResource, useSettingsForm } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, NotAvailable, SectionHeader, useAction } from '../ui';
import { BannerPresets } from './BannerPresets';
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

const COMMUNITY_FLAGS: [keyof api.CommunityObject, MessageKey][] = [
  ['community_enabled', 'settings.overview.flagCommunity'],
  ['onboarding_enabled', 'settings.overview.flagOnboarding'],
  ['welcome_screen_enabled', 'settings.overview.flagWelcome'],
  ['guide_enabled', 'settings.overview.flagGuide'],
  ['invites_paused', 'settings.overview.flagInvitesPaused'],
  ['age_restricted', 'settings.overview.flagAgeRestricted'],
];

export function OverviewSection({ guildId }: SectionProps) {
  const t = useT();
  const guild = useResource(`guild:${guildId}`, () => api.getGuild(guildId));
  useGuildEvents(guildId, ['GUILD_UPDATE'], () => void guild.reload());
  return (
    <>
      <SectionHeader
        title={t('settings.overview.title')}
        description={t('settings.overview.description')}
      />
      <Async res={guild} label={t('settings.overview.loading')}>
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
  const t = useT();
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
      ? t('settings.overview.nameError')
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
    const next = await action.run(
      () => api.modifyGuild(guildId, body),
      t('settings.overview.saved'),
    );
    if (next) {
      onSaved(next);
      draft.reset();
    }
  }, [nameError, draft, name, action, guildId, onSaved, t]);

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
        label={t('settings.overview.nameLabel')}
        value={draft.value.name}
        maxLength={100}
        error={nameError}
        onChange={(e) => draft.set('name', e.target.value)}
      />
      <TextArea
        label={t('settings.overview.descriptionLabel')}
        hint={t('settings.overview.descriptionHint')}
        value={draft.value.description}
        maxLength={1000}
        onChange={(e) => draft.set('description', e.target.value)}
      />

      <section className="ss-card" aria-labelledby="ss-appearance">
        <h2 id="ss-appearance">{t('settings.overview.appearanceTitle')}</h2>
        <ImageField
          label={t('settings.overview.iconLabel')}
          hint={t('settings.overview.iconHint')}
          ownerId={guildId}
          shape="square"
          value={draft.value.icon}
          onChange={(v) => draft.set('icon', v)}
        />
        <ImageField
          label={t('settings.overview.bannerLabel')}
          hint={t('settings.overview.bannerHint')}
          ownerId={guildId}
          value={draft.value.banner}
          onChange={(v) => draft.set('banner', v)}
        />
        <BannerPresets
          onPick={(uri) => {
            action.clear();
            draft.set('banner', uri);
          }}
          onUnsupported={() => action.setError(t('settings.bannerPresets.unsupported'))}
        />
        <ImageField
          label={t('settings.overview.splashLabel')}
          hint={t('settings.overview.splashHint')}
          ownerId={guildId}
          value={draft.value.splash}
          onChange={(v) => draft.set('splash', v)}
        />
        <ServerCard guildId={guildId} draft={draft.value} />
      </section>

      <section className="ss-card" aria-labelledby="ss-system">
        <h2 id="ss-system">{t('settings.overview.systemTitle')}</h2>
        <div>
          <label className="ss-field-label" htmlFor="ss-system-channel">
            {t('settings.overview.systemChannel')}
          </label>
          <select
            id="ss-system-channel"
            className="ss-select"
            value={draft.value.system_channel_id}
            onChange={(e) => draft.set('system_channel_id', e.target.value)}
          >
            <option value="">{t('settings.overview.noSystemMessages')}</option>
            {channels.map((c) => (
              <option key={c.id} value={c.id}>
                #{c.name}
              </option>
            ))}
          </select>
        </div>
        <NotAvailable>{t('settings.overview.systemNotAvailable')}</NotAvailable>
      </section>

      <section className="ss-card" aria-labelledby="ss-afk">
        <h2 id="ss-afk">{t('settings.overview.afkTitle')}</h2>
        <p className="ss-sub">{t('settings.overview.afkDescription')}</p>
        <div>
          <label className="ss-field-label" htmlFor="ss-afk-channel">
            {t('settings.overview.afkChannel')}
          </label>
          <select
            id="ss-afk-channel"
            className="ss-select"
            value={draft.value.afk_channel_id}
            onChange={(e) => draft.set('afk_channel_id', e.target.value)}
          >
            <option value="">{t('settings.overview.noAfkChannel')}</option>
            {voiceChannels.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="ss-field-label" htmlFor="ss-afk-timeout">
            {t('settings.overview.afkTimeout')}
          </label>
          <select
            id="ss-afk-timeout"
            className="ss-select"
            value={String(draft.value.afk_timeout)}
            onChange={(e) => draft.set('afk_timeout', Number(e.target.value))}
          >
            {api.AFK_TIMEOUTS.map((o) => (
              <option key={o.seconds} value={o.seconds}>
                {o.label}
              </option>
            ))}
          </select>
        </div>
      </section>

      <RadioGroup
        legend={t('settings.overview.notificationsLegend')}
        value={String(draft.value.default_message_notifications)}
        onValueChange={(v) => draft.set('default_message_notifications', Number(v))}
      >
        <Radio value="0" label={t('settings.overview.allMessages')} />
        <Radio value="1" label={t('settings.overview.onlyMentions')} />
      </RadioGroup>

      <section className="ss-card" aria-labelledby="ss-flags">
        <h2 id="ss-flags">{t('settings.overview.flagsTitle')}</h2>
        <p className="ss-sub">{t('settings.overview.flagsHint')}</p>
        {community.data ? (
          <ul className="ss-list" aria-label={t('settings.overview.flagsLabel')}>
            {COMMUNITY_FLAGS.map(([key, label]) => (
              <li key={key} className="ss-row">
                <span className="ss-grow">{t(label)}</span>
                <span>
                  {community.data![key] ? t('settings.overview.on') : t('settings.overview.off')}
                </span>
              </li>
            ))}
          </ul>
        ) : community.error ? (
          <p className="ss-na">{community.error}</p>
        ) : (
          <p className="ss-na">{t('settings.overview.loadingShort')}</p>
        )}
      </section>

      <section className="ss-card" aria-labelledby="ss-not-backed">
        <h2 id="ss-not-backed">{t('settings.overview.notBackedTitle')}</h2>
        <NotAvailable>{t('settings.overview.vanity')}</NotAvailable>
        <NotAvailable>{t('settings.overview.traits')}</NotAvailable>
      </section>
    </div>
  );
}

/** Live preview of how the server shows up on an invite (LNG-SSET-009). */
function ServerCard({ guildId, draft }: { guildId: string; draft: Fields }) {
  const t = useT();
  const img = (hash: string | null, size: number) =>
    hash ? (hash.startsWith('data:') ? hash : cdnUrl('icons', guildId, hash, { size })) : null;
  const banner = img(draft.banner, 512);
  const icon = img(draft.icon, 128);
  const created = snowflakeTime(guildId);
  const est = created ? formatDate(new Date(created), { month: 'short', year: 'numeric' }) : '';
  const name = draft.name.trim() || t('settings.overview.cardDefaultName');
  return (
    <div className="ss-server-card" role="group" aria-label={t('settings.overview.cardLabel')}>
      <div className="ss-server-banner">{banner && <img src={banner} alt="" />}</div>
      <div className="ss-server-meta">
        <span className="ss-server-icon" aria-hidden="true">
          {icon ? <img src={icon} alt="" /> : name.charAt(0).toUpperCase()}
        </span>
        <div>
          <strong>{name}</strong>
          {est && <div className="ss-sub">{t('settings.overview.cardEst', { date: est })}</div>}
        </div>
      </div>
    </div>
  );
}
