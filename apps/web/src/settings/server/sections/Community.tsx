import { Button, Checkbox, MultiSelect, Switch, Tabs, TextArea, TextInput } from '@sideroom/ui';
import { useCallback, useMemo, useState } from 'react';
import { t as translate, useT } from '../../../i18n';
import { useChannels, useGuild } from '../../../store';
import * as api from '../api';
import { useDraft, useGuildEvents, useResource, useSettingsForm } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { CommunityWizard } from './CommunityWizard';
import { OnboardingPreview } from './OnboardingPreview';
import { ActionStatus, Async, NotAvailable, SectionHeader, useAction } from '../ui';

export function CommunitySection({ guildId }: SectionProps) {
  const t = useT();
  return (
    <>
      <SectionHeader
        title={t('settings.community.title')}
        description={t('settings.community.description')}
      />
      <Tabs
        label={t('settings.community.settingsLabel')}
        items={[
          {
            id: 'overview',
            label: t('settings.community.tabOverview'),
            content: <CommunityOverview guildId={guildId} />,
          },
          {
            id: 'onboarding',
            label: t('settings.community.tabOnboarding'),
            content: <OnboardingTab guildId={guildId} />,
          },
          {
            id: 'welcome',
            label: t('settings.community.tabWelcome'),
            content: <WelcomeTab guildId={guildId} />,
          },
          { id: 'more', label: t('settings.community.tabMore'), content: <NotYet /> },
        ]}
      />
    </>
  );
}

function NotYet() {
  const t = useT();
  return (
    <div className="ss-form">
      <NotAvailable>{t('settings.community.notYetGuide')}</NotAvailable>
      <NotAvailable>{t('settings.community.notYetRules')}</NotAvailable>
      <NotAvailable>{t('settings.community.notYetInsights')}</NotAvailable>
      <NotAvailable>{t('settings.community.notYetDirectory')}</NotAvailable>
    </div>
  );
}

function ChannelSelect({
  id,
  label,
  value,
  onChange,
  channels,
  none,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (v: string) => void;
  channels: { id: string; name?: string | null }[];
  none?: string;
}) {
  const t = useT();
  return (
    <div>
      <label className="ss-field-label" htmlFor={id}>
        {label}
      </label>
      <select
        id={id}
        className="ss-select"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        <option value="">{none ?? t('settings.community.none')}</option>
        {channels.map((c) => (
          <option key={c.id} value={c.id}>
            #{c.name}
          </option>
        ))}
      </select>
    </div>
  );
}

// ---- Overview ----

function CommunityOverview({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`community:${guildId}`, () => api.getCommunity(guildId));
  useGuildEvents(guildId, ['GUILD_UPDATE'], () => void res.reload());
  return (
    <Async res={res} label={t('settings.community.loading')}>
      {(c) => <CommunityForm guildId={guildId} community={c} onSaved={(n) => res.set(n)} />}
    </Async>
  );
}

interface CommunityFields {
  community_enabled: boolean;
  rules_channel_id: string;
  public_updates_channel_id: string;
  safety_alerts_channel_id: string;
  description: string;
  preferred_locale: string;
  guide_enabled: boolean;
  raid_alert_joins_per_minute: number;
  age_restricted: boolean;
}

const toCommunityFields = (c: api.CommunityObject): CommunityFields => ({
  community_enabled: c.community_enabled,
  rules_channel_id: c.rules_channel_id ?? '',
  public_updates_channel_id: c.public_updates_channel_id ?? '',
  safety_alerts_channel_id: c.safety_alerts_channel_id ?? '',
  description: c.description ?? '',
  preferred_locale: c.preferred_locale,
  guide_enabled: c.guide_enabled,
  raid_alert_joins_per_minute: c.raid_alert_joins_per_minute,
  age_restricted: c.age_restricted,
});

function CommunityForm({
  guildId,
  community,
  onSaved,
}: {
  guildId: string;
  community: api.CommunityObject;
  onSaved: (c: api.CommunityObject) => void;
}) {
  const t = useT();
  const base = useMemo(() => toCommunityFields(community), [community]);
  const draft = useDraft(base);
  const action = useAction();
  const [wizard, setWizard] = useState(false);
  const channels = useChannels(guildId).filter((c) => c.type === 0 || c.type === 5);
  const { guild } = useGuild(guildId);
  const v = draft.value;
  const needsChannels =
    v.community_enabled && (!v.rules_channel_id || !v.public_updates_channel_id);
  const error = needsChannels ? t('settings.community.needsChannels') : null;

  const save = useCallback(async () => {
    if (error) {
      action.setError(error);
      return;
    }
    const c = draft.changed;
    const body: api.CommunityPatch = {};
    if (c.community_enabled !== undefined) body.community_enabled = c.community_enabled;
    if (c.rules_channel_id !== undefined) body.rules_channel_id = c.rules_channel_id || null;
    if (c.public_updates_channel_id !== undefined)
      body.public_updates_channel_id = c.public_updates_channel_id || null;
    if (c.safety_alerts_channel_id !== undefined)
      body.safety_alerts_channel_id = c.safety_alerts_channel_id || null;
    if (c.description !== undefined) body.description = c.description.trim() || null;
    if (c.preferred_locale !== undefined) body.preferred_locale = c.preferred_locale;
    if (c.guide_enabled !== undefined) body.guide_enabled = c.guide_enabled;
    if (c.raid_alert_joins_per_minute !== undefined)
      body.raid_alert_joins_per_minute = c.raid_alert_joins_per_minute;
    if (c.age_restricted !== undefined) body.age_restricted = c.age_restricted;
    const next = await action.run(
      () => api.modifyCommunity(guildId, body),
      t('settings.community.saved'),
    );
    if (next) {
      onSaved(next);
      draft.reset();
    }
  }, [error, action, draft, guildId, onSaved, t]);

  useSettingsForm({
    dirty: draft.dirty,
    saving: action.busy,
    save: () => void save(),
    reset: draft.reset,
  });

  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      <Switch
        label={t('settings.community.enable')}
        description={t('settings.community.enableDescription', {
          name: guild?.name ?? t('settings.community.thisServer'),
        })}
        checked={v.community_enabled}
        onChange={(e) => {
          // Turning Community on goes through the wizard; turning it off is a plain setting.
          if (e.target.checked && !community.community_enabled) setWizard(true);
          else draft.set('community_enabled', e.target.checked);
        }}
      />
      {wizard && (
        <CommunityWizard
          guildId={guildId}
          onClose={() => setWizard(false)}
          onDone={(c) => {
            setWizard(false);
            onSaved(c);
            draft.reset();
          }}
        />
      )}
      <ChannelSelect
        id="ss-rules-channel"
        label={t('settings.community.rulesChannel')}
        value={v.rules_channel_id}
        channels={channels}
        onChange={(x) => draft.set('rules_channel_id', x)}
      />
      <ChannelSelect
        id="ss-updates-channel"
        label={t('settings.community.updatesChannel')}
        value={v.public_updates_channel_id}
        channels={channels}
        onChange={(x) => draft.set('public_updates_channel_id', x)}
      />
      <ChannelSelect
        id="ss-safety-channel"
        label={t('settings.community.safetyChannel')}
        value={v.safety_alerts_channel_id}
        channels={channels}
        onChange={(x) => draft.set('safety_alerts_channel_id', x)}
      />
      <TextArea
        label={t('settings.community.descriptionLabel')}
        value={v.description}
        maxLength={300}
        onChange={(e) => draft.set('description', e.target.value)}
      />
      <TextInput
        label={t('settings.community.language')}
        hint={t('settings.community.languageHint')}
        value={v.preferred_locale}
        maxLength={16}
        onChange={(e) => draft.set('preferred_locale', e.target.value)}
      />
      <Switch
        label={t('settings.community.guide')}
        checked={v.guide_enabled}
        onChange={(e) => draft.set('guide_enabled', e.target.checked)}
      />
      <TextInput
        label={t('settings.community.raidLabel')}
        hint={t('settings.community.raidHint')}
        type="number"
        min={0}
        value={String(v.raid_alert_joins_per_minute)}
        onChange={(e) =>
          draft.set('raid_alert_joins_per_minute', Math.max(0, Number(e.target.value) || 0))
        }
      />
      <Switch
        label={t('settings.community.ageRestricted')}
        checked={v.age_restricted}
        onChange={(e) => draft.set('age_restricted', e.target.checked)}
      />
    </div>
  );
}

// ---- Onboarding ----

function OnboardingTab({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`onboarding:${guildId}`, () => api.getOnboarding(guildId));
  return (
    <Async res={res} label={t('settings.community.loadingOnboarding')}>
      {(o) => <OnboardingForm guildId={guildId} onboarding={o} onSaved={(n) => res.set(n)} />}
    </Async>
  );
}

interface OnboardingFields {
  enabled: boolean;
  default_channel_ids: string[];
  prompts: api.OnboardingPrompt[];
}

const blankOption = (): api.OnboardingOption => ({ title: '', channel_ids: [], role_ids: [] });
const blankPrompt = (): api.OnboardingPrompt => ({
  title: '',
  single_select: false,
  required: false,
  in_onboarding: true,
  options: [blankOption(), blankOption()],
});

export function validateOnboarding(f: OnboardingFields): string | null {
  for (const [i, p] of f.prompts.entries()) {
    if (!p.title.trim()) return translate('settings.community.questionNeedsTitle', { n: i + 1 });
    if (p.options.length < 2)
      return translate('settings.community.questionNeedsAnswers', { n: i + 1 });
    if (p.options.some((o) => !o.title.trim()))
      return translate('settings.community.answerNeedsTitle', { n: i + 1 });
  }
  return null;
}

function OnboardingForm({
  guildId,
  onboarding,
  onSaved,
}: {
  guildId: string;
  onboarding: api.OnboardingObject;
  onSaved: (o: api.OnboardingObject) => void;
}) {
  const base = useMemo<OnboardingFields>(
    () => ({
      enabled: onboarding.enabled,
      default_channel_ids: onboarding.default_channel_ids,
      prompts: onboarding.prompts,
    }),
    [onboarding],
  );
  const t = useT();
  const draft = useDraft(base);
  const action = useAction();
  const { roles } = useGuild(guildId);
  const channels = useChannels(guildId).filter((c) => c.type === 0 || c.type === 5);
  const channelOptions = channels.map((c) => ({ value: c.id, label: `#${c.name}` }));
  const roleOptions = roles
    .filter((r) => r.id !== guildId)
    .map((r) => ({ value: r.id, label: r.name }));
  const f = draft.value;
  const problem = validateOnboarding(f);

  const save = useCallback(async () => {
    if (problem) {
      action.setError(problem);
      return;
    }
    const next = await action.run(
      () =>
        api.putOnboarding(guildId, {
          enabled: f.enabled,
          default_channel_ids: f.default_channel_ids,
          prompts: f.prompts,
        }),
      t('settings.community.onboardingSaved'),
    );
    if (next) {
      onSaved(next);
      draft.reset();
    }
  }, [problem, action, guildId, f, onSaved, draft, t]);

  useSettingsForm({
    dirty: draft.dirty,
    saving: action.busy,
    save: () => void save(),
    reset: draft.reset,
  });

  const setPrompt = (i: number, patch: Partial<api.OnboardingPrompt>) =>
    draft.set(
      'prompts',
      f.prompts.map((p, j) => (j === i ? { ...p, ...patch } : p)),
    );
  const setOption = (i: number, k: number, patch: Partial<api.OnboardingOption>) =>
    setPrompt(i, {
      options: f.prompts[i]!.options.map((o, j) => (j === k ? { ...o, ...patch } : o)),
    });

  return (
    <div className="ss-form" style={{ maxWidth: 720 }}>
      <ActionStatus error={action.error} done={action.done} />
      <Switch
        label={t('settings.community.enableOnboarding')}
        description={t('settings.community.enableOnboardingDescription')}
        checked={f.enabled}
        onChange={(e) => draft.set('enabled', e.target.checked)}
      />
      <MultiSelect
        aria-label={t('settings.community.defaultChannels')}
        placeholder={t('settings.community.defaultChannelsPlaceholder')}
        options={channelOptions}
        values={f.default_channel_ids}
        onValuesChange={(v) => draft.set('default_channel_ids', v)}
      />
      {f.prompts.map((p, i) => (
        <fieldset key={i} className="ss-card">
          <legend>{t('settings.community.questionN', { n: i + 1 })}</legend>
          <TextInput
            label={t('settings.community.question')}
            value={p.title}
            maxLength={100}
            onChange={(e) => setPrompt(i, { title: e.target.value })}
          />
          <Checkbox
            label={t('settings.community.singleSelect')}
            checked={p.single_select}
            onChange={(e) => setPrompt(i, { single_select: e.target.checked })}
          />
          <Checkbox
            label={t('settings.community.required')}
            checked={p.required}
            onChange={(e) => setPrompt(i, { required: e.target.checked })}
          />
          <Checkbox
            label={t('settings.community.showDuring')}
            checked={p.in_onboarding}
            onChange={(e) => setPrompt(i, { in_onboarding: e.target.checked })}
          />
          {p.options.map((o, k) => (
            <div key={k} className="ss-card">
              <TextInput
                label={t('settings.community.answerOf', { k: k + 1, i: i + 1 })}
                value={o.title}
                maxLength={50}
                onChange={(e) => setOption(i, k, { title: e.target.value })}
              />
              <TextInput
                label={t('settings.community.answerDescription', { k: k + 1 })}
                value={o.description ?? ''}
                maxLength={100}
                onChange={(e) => setOption(i, k, { description: e.target.value })}
              />
              <MultiSelect
                aria-label={t('settings.community.channelsFor', { k: k + 1, i: i + 1 })}
                placeholder={t('settings.community.channels')}
                options={channelOptions}
                values={o.channel_ids}
                onValuesChange={(v) => setOption(i, k, { channel_ids: v })}
              />
              <MultiSelect
                aria-label={t('settings.community.rolesFor', { k: k + 1, i: i + 1 })}
                placeholder={t('settings.community.roles')}
                options={roleOptions}
                values={o.role_ids}
                onValuesChange={(v) => setOption(i, k, { role_ids: v })}
              />
              <div>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setPrompt(i, { options: p.options.filter((_, j) => j !== k) })}
                >
                  {t('settings.community.removeAnswer', { k: k + 1 })}
                </Button>
              </div>
            </div>
          ))}
          <div className="ss-actions">
            <Button
              size="sm"
              variant="secondary"
              onClick={() => setPrompt(i, { options: [...p.options, blankOption()] })}
            >
              {t('settings.community.addAnswer')}
            </Button>
            <Button
              size="sm"
              variant="danger"
              onClick={() =>
                draft.set(
                  'prompts',
                  f.prompts.filter((_, j) => j !== i),
                )
              }
            >
              {t('settings.community.removeQuestion', { i: i + 1 })}
            </Button>
          </div>
        </fieldset>
      ))}
      <div className="ss-actions">
        <OnboardingPreview
          prompts={f.prompts}
          defaultChannelIds={f.default_channel_ids}
          channels={channels}
          roles={roles}
        />
        <Button
          variant="secondary"
          disabled={f.prompts.length >= 5}
          onClick={() => draft.set('prompts', [...f.prompts, blankPrompt()])}
        >
          {t('settings.community.addQuestion')}
        </Button>
      </div>
    </div>
  );
}

// ---- Welcome screen ----

function WelcomeTab({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`welcome:${guildId}`, () => api.getWelcomeScreen(guildId));
  return (
    <Async res={res} label={t('settings.community.loadingWelcome')}>
      {(w) => <WelcomeForm guildId={guildId} welcome={w} onSaved={(n) => res.set(n)} />}
    </Async>
  );
}

interface WelcomeFields {
  enabled: boolean;
  description: string;
  welcome_channels: api.WelcomeChannel[];
}

function WelcomeForm({
  guildId,
  welcome,
  onSaved,
}: {
  guildId: string;
  welcome: api.WelcomeScreenObject;
  onSaved: (w: api.WelcomeScreenObject) => void;
}) {
  const base = useMemo<WelcomeFields>(
    () => ({
      enabled: welcome.enabled,
      description: welcome.description ?? '',
      welcome_channels: welcome.welcome_channels,
    }),
    [welcome],
  );
  const t = useT();
  const draft = useDraft(base);
  const action = useAction();
  const channels = useChannels(guildId).filter((c) => c.type === 0 || c.type === 5);
  const f = draft.value;
  const problem = f.welcome_channels.some((c) => !c.channel_id || !c.description.trim())
    ? t('settings.community.welcomeProblem')
    : null;

  const save = useCallback(async () => {
    if (problem) {
      action.setError(problem);
      return;
    }
    const next = await action.run(
      () =>
        api.modifyWelcomeScreen(guildId, {
          enabled: f.enabled,
          description: f.description.trim() || null,
          welcome_channels: f.welcome_channels,
        }),
      t('settings.community.welcomeSaved'),
    );
    if (next) {
      onSaved(next);
      draft.reset();
    }
  }, [problem, action, guildId, f, onSaved, draft, t]);

  useSettingsForm({
    dirty: draft.dirty,
    saving: action.busy,
    save: () => void save(),
    reset: draft.reset,
  });

  const setCh = (i: number, patch: Partial<api.WelcomeChannel>) =>
    draft.set(
      'welcome_channels',
      f.welcome_channels.map((c, j) => (j === i ? { ...c, ...patch } : c)),
    );

  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      <Switch
        label={t('settings.community.enableWelcome')}
        checked={f.enabled}
        onChange={(e) => draft.set('enabled', e.target.checked)}
      />
      <TextArea
        label={t('settings.community.serverDescription')}
        value={f.description}
        maxLength={140}
        onChange={(e) => draft.set('description', e.target.value)}
      />
      {f.welcome_channels.map((c, i) => (
        <fieldset key={i} className="ss-card">
          <legend>{t('settings.community.welcomeChannelN', { n: i + 1 })}</legend>
          <ChannelSelect
            id={`ss-welcome-ch-${i}`}
            label={t('settings.community.channelN', { n: i + 1 })}
            value={c.channel_id}
            channels={channels}
            none={t('settings.community.chooseChannel')}
            onChange={(v) => setCh(i, { channel_id: v })}
          />
          <TextInput
            label={t('settings.community.channelDescription', { n: i + 1 })}
            value={c.description}
            maxLength={50}
            onChange={(e) => setCh(i, { description: e.target.value })}
          />
          <TextInput
            label={t('settings.community.channelEmoji', { n: i + 1 })}
            value={c.emoji_name ?? ''}
            maxLength={8}
            onChange={(e) => setCh(i, { emoji_name: e.target.value || null })}
          />
          <div>
            <Button
              size="sm"
              variant="ghost"
              onClick={() =>
                draft.set(
                  'welcome_channels',
                  f.welcome_channels.filter((_, j) => j !== i),
                )
              }
            >
              {t('settings.community.removeChannel', { n: i + 1 })}
            </Button>
          </div>
        </fieldset>
      ))}
      <div>
        <Button
          variant="secondary"
          disabled={f.welcome_channels.length >= 5}
          onClick={() =>
            draft.set('welcome_channels', [
              ...f.welcome_channels,
              { channel_id: '', description: '', emoji_name: null },
            ])
          }
        >
          {t('settings.community.addChannel')}
        </Button>
      </div>
    </div>
  );
}
