import {
  Badge,
  Button,
  Checkbox,
  ConfirmDialog,
  EmptyState,
  Modal,
  MultiSelect,
  Radio,
  RadioGroup,
  Switch,
  TextArea,
  TextInput,
} from '@sideroom/ui';
import { useCallback, useMemo, useState } from 'react';
import { t as translate, useT, type MessageKey } from '../../../i18n';
import { useChannels, useGuild } from '../../../store';
import * as api from '../api';
import { ACTION, TRIGGER } from '../api';
import { useDraft, useGuildEvents, useResource, useSettingsForm } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';

/** `label` and `hint` are message keys, resolved at render time. */
export const VERIFICATION_LEVELS: { value: number; label: MessageKey; hint: MessageKey }[] = [
  { value: 0, label: 'settings.moderation.verifyNone', hint: 'settings.moderation.verifyNoneHint' },
  { value: 1, label: 'settings.moderation.verifyLow', hint: 'settings.moderation.verifyLowHint' },
  {
    value: 2,
    label: 'settings.moderation.verifyMedium',
    hint: 'settings.moderation.verifyMediumHint',
  },
  { value: 3, label: 'settings.moderation.verifyHigh', hint: 'settings.moderation.verifyHighHint' },
  {
    value: 4,
    label: 'settings.moderation.verifyHighest',
    hint: 'settings.moderation.verifyHighestHint',
  },
];
/** `label` is a message key, resolved at render time. */
export const CONTENT_FILTERS: { value: number; label: MessageKey }[] = [
  { value: 0, label: 'settings.moderation.filterNone' },
  { value: 1, label: 'settings.moderation.filterNoRole' },
  { value: 2, label: 'settings.moderation.filterAll' },
];

/** Message keys, resolved at render time. */
export const TRIGGER_LABEL: Record<number, MessageKey> = {
  [TRIGGER.KEYWORD]: 'settings.moderation.triggerKeyword',
  [TRIGGER.SPAM]: 'settings.moderation.triggerSpam',
  [TRIGGER.PRESET]: 'settings.moderation.triggerPreset',
  [TRIGGER.MENTION_SPAM]: 'settings.moderation.triggerMention',
  [TRIGGER.MEMBER_PROFILE]: 'settings.moderation.triggerProfile',
  [TRIGGER.LINKS]: 'settings.moderation.triggerLinks',
};
const CREATABLE = [
  TRIGGER.KEYWORD,
  TRIGGER.MENTION_SPAM,
  TRIGGER.LINKS,
  TRIGGER.SPAM,
  TRIGGER.PRESET,
];
const PRESETS: { id: number; label: MessageKey }[] = [
  { id: 1, label: 'settings.moderation.presetProfanity' },
  { id: 2, label: 'settings.moderation.presetSexual' },
  { id: 3, label: 'settings.moderation.presetSlurs' },
];
/** `label` is a message key, resolved at render time. */
export const TIMEOUT_OPTIONS: { seconds: number; label: MessageKey }[] = [
  { seconds: 60, label: 'settings.moderation.timeout60s' },
  { seconds: 300, label: 'settings.moderation.timeout5m' },
  { seconds: 600, label: 'settings.moderation.timeout10m' },
  { seconds: 3600, label: 'settings.moderation.timeout1h' },
  { seconds: 86400, label: 'settings.moderation.timeout1d' },
  { seconds: 604800, label: 'settings.moderation.timeout1w' },
];

const lines = (s: string) =>
  s
    .split(/[\n,]/)
    .map((x) => x.trim())
    .filter(Boolean);

export function ModerationSection({ guildId }: SectionProps) {
  const t = useT();
  const rules = useResource(`automod:${guildId}`, () => api.listRules(guildId));
  useGuildEvents(
    guildId,
    ['AUTO_MODERATION_RULE_CREATE', 'AUTO_MODERATION_RULE_UPDATE', 'AUTO_MODERATION_RULE_DELETE'],
    () => void rules.reload(),
  );
  const [editing, setEditing] = useState<api.AutoModRule | 'new' | null>(null);
  const [toDelete, setToDelete] = useState<api.AutoModRule | null>(null);
  const action = useAction();

  const toggle = async (rule: api.AutoModRule, enabled: boolean) => {
    const next = await action.run(() => api.modifyRule(guildId, rule.id, { enabled }));
    if (next) rules.set((cur) => (cur ?? []).map((r) => (r.id === next.id ? next : r)));
  };
  const remove = async () => {
    if (!toDelete) return;
    const ok = await action.run(async () => {
      await api.deleteRule(guildId, toDelete.id);
      return true;
    });
    if (ok) {
      rules.set((cur) => (cur ?? []).filter((r) => r.id !== toDelete.id));
      setToDelete(null);
    }
  };

  return (
    <>
      <SectionHeader
        title={t('settings.moderation.title')}
        description={t('settings.moderation.description')}
      />
      <SafetyForm guildId={guildId} />
      <h2 className="ss-title" style={{ fontSize: '1rem', margin: '32px 0 12px' }}>
        {t('settings.moderation.rules')}
      </h2>
      <div className="ss-actions" style={{ marginBottom: 12 }}>
        <Button onClick={() => setEditing('new')}>{t('settings.moderation.createRule')}</Button>
      </div>
      <ActionStatus error={action.error} />
      <Async res={rules} label={t('settings.moderation.loadingRules')}>
        {(list) =>
          list.length === 0 ? (
            <EmptyState
              title={t('settings.moderation.noRulesTitle')}
              description={t('settings.moderation.noRulesDescription')}
            />
          ) : (
            <ul className="ss-list" aria-label={t('settings.moderation.rulesLabel')}>
              {list.map((r) => (
                <li key={r.id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">{r.name}</div>
                    <div className="ss-meta">
                      {TRIGGER_LABEL[r.trigger_type]
                        ? t(TRIGGER_LABEL[r.trigger_type]!)
                        : t('settings.moderation.triggerFallback', { type: r.trigger_type })}{' '}
                      -{' '}
                      {r.actions
                        .map((a) =>
                          a.type === ACTION.BLOCK
                            ? t('settings.moderation.actionBlock')
                            : a.type === ACTION.ALERT
                              ? t('settings.moderation.actionAlert')
                              : t('settings.moderation.actionTimeout'),
                        )
                        .join(', ') || t('settings.moderation.noActions')}
                    </div>
                  </div>
                  <Badge variant={r.enabled ? 'success' : 'neutral'}>
                    {r.enabled ? t('settings.moderation.on') : t('settings.moderation.off')}
                  </Badge>
                  <Switch
                    label={
                      <span className="ss-sr">
                        {t('settings.moderation.enableNamed', { name: r.name })}
                      </span>
                    }
                    checked={r.enabled}
                    disabled={action.busy}
                    onChange={(e) => void toggle(r, e.target.checked)}
                  />
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={t('settings.moderation.editRule', { name: r.name })}
                    onClick={() => setEditing(r)}
                  >
                    {t('settings.moderation.edit')}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={t('settings.moderation.deleteRuleNamed', { name: r.name })}
                    onClick={() => setToDelete(r)}
                  >
                    {t('settings.moderation.delete')}
                  </Button>
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      {editing && (
        <RuleEditor
          guildId={guildId}
          rule={editing === 'new' ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={async () => {
            setEditing(null);
            await rules.reload();
          }}
        />
      )}
      <ConfirmDialog
        open={!!toDelete}
        danger
        title={t('settings.moderation.deleteTitle', {
          name: toDelete?.name ?? t('settings.moderation.ruleWord'),
        })}
        description={t('settings.moderation.deleteDescription')}
        confirmLabel={t('settings.moderation.deleteRule')}
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToDelete(null)}
      />
    </>
  );
}

function SafetyForm({ guildId }: { guildId: string }) {
  const t = useT();
  const guild = useResource(`guild:${guildId}`, () => api.getGuild(guildId));
  useGuildEvents(guildId, ['GUILD_UPDATE'], () => void guild.reload());
  return (
    <Async res={guild} label={t('settings.moderation.loadingSafety')}>
      {(g) => <SafetyFields guildId={guildId} guild={g} onSaved={(n) => guild.set(n)} />}
    </Async>
  );
}

function SafetyFields({
  guildId,
  guild,
  onSaved,
}: {
  guildId: string;
  guild: api.GuildObject;
  onSaved: (g: api.GuildObject) => void;
}) {
  const base = useMemo(
    () => ({
      verification_level: guild.verification_level,
      explicit_content_filter: guild.explicit_content_filter,
    }),
    [guild],
  );
  const t = useT();
  const draft = useDraft(base);
  const action = useAction();
  const save = useCallback(async () => {
    const next = await action.run(
      () => api.modifyGuild(guildId, draft.changed),
      t('settings.moderation.safetySaved'),
    );
    if (next) {
      onSaved(next);
      draft.reset();
    }
  }, [action, guildId, draft, onSaved, t]);
  useSettingsForm({
    dirty: draft.dirty,
    saving: action.busy,
    save: () => void save(),
    reset: draft.reset,
  });
  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      <RadioGroup
        legend={t('settings.moderation.verificationLevel')}
        value={String(draft.value.verification_level)}
        onValueChange={(v) => draft.set('verification_level', Number(v))}
      >
        {VERIFICATION_LEVELS.map((l) => (
          <Radio key={l.value} value={String(l.value)} label={t(l.label)} description={t(l.hint)} />
        ))}
      </RadioGroup>
      <RadioGroup
        legend={t('settings.moderation.contentFilter')}
        value={String(draft.value.explicit_content_filter)}
        onValueChange={(v) => draft.set('explicit_content_filter', Number(v))}
      >
        {CONTENT_FILTERS.map((l) => (
          <Radio key={l.value} value={String(l.value)} label={t(l.label)} />
        ))}
      </RadioGroup>
    </div>
  );
}

export interface RuleForm {
  name: string;
  trigger: number;
  keywords: string;
  regex: string;
  allow: string;
  presets: number[];
  mentionLimit: number;
  raidProtection: boolean;
  blockAllLinks: boolean;
  blockDomains: string;
  allowDomains: string;
  block: boolean;
  blockMessage: string;
  alert: boolean;
  alertChannel: string;
  timeout: boolean;
  timeoutSeconds: number;
  exemptRoles: string[];
  exemptChannels: string[];
  enabled: boolean;
}

export const emptyRule = (): RuleForm => ({
  name: '',
  trigger: TRIGGER.KEYWORD,
  keywords: '',
  regex: '',
  allow: '',
  presets: [],
  mentionLimit: 5,
  raidProtection: false,
  blockAllLinks: false,
  blockDomains: '',
  allowDomains: '',
  block: true,
  blockMessage: '',
  alert: false,
  alertChannel: '',
  timeout: false,
  timeoutSeconds: 600,
  exemptRoles: [],
  exemptChannels: [],
  enabled: true,
});

export function ruleToForm(r: api.AutoModRule): RuleForm {
  const m = r.trigger_metadata;
  const block = r.actions.find((a) => a.type === ACTION.BLOCK);
  const alert = r.actions.find((a) => a.type === ACTION.ALERT);
  const timeout = r.actions.find((a) => a.type === ACTION.TIMEOUT);
  return {
    name: r.name,
    trigger: r.trigger_type,
    keywords: (m.keyword_filter ?? []).join('\n'),
    regex: (m.regex_patterns ?? []).join('\n'),
    allow: (m.allow_list ?? []).join('\n'),
    presets: m.presets ?? [],
    mentionLimit: m.mention_total_limit ?? 5,
    raidProtection: !!m.mention_raid_protection_enabled,
    blockAllLinks: !!m.link_block_all,
    blockDomains: (m.link_block_domains ?? []).join('\n'),
    allowDomains: (m.link_allow_domains ?? []).join('\n'),
    block: !!block,
    blockMessage: block?.metadata.custom_message ?? '',
    alert: !!alert,
    alertChannel: alert?.metadata.channel_id ?? '',
    timeout: !!timeout,
    timeoutSeconds: timeout?.metadata.duration_seconds ?? 600,
    exemptRoles: r.exempt_roles,
    exemptChannels: r.exempt_channels,
    enabled: r.enabled,
  };
}

/** Validation message for a rule form, or null when it can be sent. */
export function validateRule(f: RuleForm): string | null {
  if (f.name.trim().length < 1 || f.name.trim().length > 100)
    return translate('settings.moderation.vName');
  if (!f.block && !f.alert && !f.timeout) return translate('settings.moderation.vAction');
  if (f.alert && !f.alertChannel) return translate('settings.moderation.vAlertChannel');
  if (f.trigger === TRIGGER.KEYWORD) {
    if (lines(f.keywords).length === 0 && lines(f.regex).length === 0)
      return translate('settings.moderation.vKeyword');
    if (lines(f.keywords).length > 1000) return translate('settings.moderation.vMaxKeywords');
    if (lines(f.regex).length > 10) return translate('settings.moderation.vMaxRegex');
    for (const p of lines(f.regex)) {
      try {
        new RegExp(p);
      } catch {
        return translate('settings.moderation.vBadRegex', { pattern: p });
      }
    }
  }
  if (f.trigger === TRIGGER.PRESET && f.presets.length === 0)
    return translate('settings.moderation.vWordList');
  if (f.trigger === TRIGGER.MENTION_SPAM && (f.mentionLimit < 1 || f.mentionLimit > 50))
    return translate('settings.moderation.vMention');
  if (f.trigger === TRIGGER.LINKS && !f.blockAllLinks && lines(f.blockDomains).length === 0)
    return translate('settings.moderation.vLinks');
  if (f.block && f.blockMessage.length > 150)
    return translate('settings.moderation.vCustomMessage');
  return null;
}

export function formToBody(f: RuleForm, creating: boolean): api.AutoModBody {
  const md: api.TriggerMetadata = {};
  switch (f.trigger) {
    case TRIGGER.KEYWORD:
    case TRIGGER.MEMBER_PROFILE:
      md.keyword_filter = lines(f.keywords);
      md.regex_patterns = lines(f.regex);
      md.allow_list = lines(f.allow);
      break;
    case TRIGGER.PRESET:
      md.presets = f.presets;
      md.allow_list = lines(f.allow);
      break;
    case TRIGGER.MENTION_SPAM:
      md.mention_total_limit = f.mentionLimit;
      md.mention_raid_protection_enabled = f.raidProtection;
      break;
    case TRIGGER.LINKS:
      md.link_block_all = f.blockAllLinks;
      md.link_block_domains = lines(f.blockDomains);
      md.link_allow_domains = lines(f.allowDomains);
      break;
  }
  const actions: api.RuleAction[] = [];
  if (f.block)
    actions.push({
      type: ACTION.BLOCK,
      metadata: f.blockMessage.trim() ? { custom_message: f.blockMessage.trim() } : {},
    });
  if (f.alert) actions.push({ type: ACTION.ALERT, metadata: { channel_id: f.alertChannel } });
  if (f.timeout)
    actions.push({ type: ACTION.TIMEOUT, metadata: { duration_seconds: f.timeoutSeconds } });
  return {
    name: f.name.trim(),
    ...(creating
      ? { event_type: f.trigger === TRIGGER.MEMBER_PROFILE ? 2 : 1, trigger_type: f.trigger }
      : {}),
    trigger_metadata: md,
    actions,
    enabled: f.enabled,
    exempt_roles: f.exemptRoles,
    exempt_channels: f.exemptChannels,
  };
}

function RuleEditor({
  guildId,
  rule,
  onClose,
  onSaved,
}: {
  guildId: string;
  rule: api.AutoModRule | null;
  onClose: () => void;
  onSaved: () => Promise<void>;
}) {
  const t = useT();
  const [f, setF] = useState<RuleForm>(() => (rule ? ruleToForm(rule) : emptyRule()));
  const set = <K extends keyof RuleForm>(k: K, v: RuleForm[K]) =>
    setF((cur) => ({ ...cur, [k]: v }));
  const [touched, setTouched] = useState(false);
  const [sample, setSample] = useState('');
  const action = useAction();
  const { roles } = useGuild(guildId);
  const channels = useChannels(guildId).filter((c) => c.type === 0 || c.type === 5);
  const problem = validateRule(f);
  const creating = !rule;

  const submit = async () => {
    setTouched(true);
    if (problem) return;
    const body = formToBody(f, creating);
    const saved = await action.run(() =>
      rule ? api.modifyRule(guildId, rule.id, body) : api.createRule(guildId, body),
    );
    if (saved) await onSaved();
  };

  const regexResult = (() => {
    const pats = lines(f.regex);
    if (!sample || pats.length === 0) return null;
    for (const p of pats) {
      try {
        if (new RegExp(p, 'i').test(sample))
          return t('settings.moderation.regexMatches', { pattern: p });
      } catch {
        return t('settings.moderation.regexInvalid', { pattern: p });
      }
    }
    return t('settings.moderation.regexNoMatch');
  })();

  return (
    <Modal
      open
      onClose={onClose}
      size="lg"
      title={
        rule
          ? t('settings.moderation.editTitle', { name: rule.name })
          : t('settings.moderation.createTitle')
      }
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('settings.moderation.cancel')}
          </Button>
          <Button loading={action.busy} onClick={() => void submit()}>
            {rule ? t('settings.moderation.saveRule') : t('settings.moderation.createRule')}
          </Button>
        </>
      }
    >
      <div className="ss-form" style={{ maxWidth: 'none' }}>
        <ActionStatus error={action.error} />
        {touched && problem && <ActionStatus error={problem} />}
        <TextInput
          label={t('settings.moderation.ruleName')}
          value={f.name}
          maxLength={100}
          onChange={(e) => set('name', e.target.value)}
        />
        {creating ? (
          <div>
            <label className="ss-field-label" htmlFor="ss-rule-trigger">
              {t('settings.moderation.trigger')}
            </label>
            <select
              id="ss-rule-trigger"
              className="ss-select"
              value={f.trigger}
              onChange={(e) => set('trigger', Number(e.target.value))}
            >
              {CREATABLE.map((tr) => (
                <option key={tr} value={tr}>
                  {t(TRIGGER_LABEL[tr]!)}
                </option>
              ))}
            </select>
          </div>
        ) : (
          <p className="ss-sub">
            {t('settings.moderation.triggerFixed', {
              trigger: TRIGGER_LABEL[f.trigger] ? t(TRIGGER_LABEL[f.trigger]!) : f.trigger,
            })}
          </p>
        )}

        {(f.trigger === TRIGGER.KEYWORD || f.trigger === TRIGGER.MEMBER_PROFILE) && (
          <>
            <TextArea
              label={t('settings.moderation.keywords')}
              hint={t('settings.moderation.keywordsHint')}
              value={f.keywords}
              onChange={(e) => set('keywords', e.target.value)}
            />
            <TextArea
              label={t('settings.moderation.regex')}
              hint={t('settings.moderation.regexHint')}
              value={f.regex}
              onChange={(e) => set('regex', e.target.value)}
            />
            <TextInput
              label={t('settings.moderation.regexTester')}
              placeholder={t('settings.moderation.regexSample')}
              value={sample}
              onChange={(e) => setSample(e.target.value)}
              hint={regexResult ?? undefined}
            />
            <TextArea
              label={t('settings.moderation.allowedWords')}
              hint={t('settings.moderation.allowedWordsHint')}
              value={f.allow}
              onChange={(e) => set('allow', e.target.value)}
            />
          </>
        )}
        {f.trigger === TRIGGER.PRESET && (
          <fieldset className="ss-card">
            <legend>{t('settings.moderation.wordLists')}</legend>
            {PRESETS.map((p) => (
              <Checkbox
                key={p.id}
                label={t(p.label)}
                checked={f.presets.includes(p.id)}
                onChange={(e) =>
                  set(
                    'presets',
                    e.target.checked ? [...f.presets, p.id] : f.presets.filter((x) => x !== p.id),
                  )
                }
              />
            ))}
          </fieldset>
        )}
        {f.trigger === TRIGGER.MENTION_SPAM && (
          <>
            <TextInput
              label={t('settings.moderation.mentionLimit')}
              type="number"
              min={1}
              max={50}
              value={String(f.mentionLimit)}
              onChange={(e) => set('mentionLimit', Number(e.target.value))}
            />
            <Switch
              label={t('settings.moderation.raidProtection')}
              description={t('settings.moderation.raidProtectionHint')}
              checked={f.raidProtection}
              onChange={(e) => set('raidProtection', e.target.checked)}
            />
          </>
        )}
        {f.trigger === TRIGGER.LINKS && (
          <>
            <Switch
              label={t('settings.moderation.blockAllLinks')}
              checked={f.blockAllLinks}
              onChange={(e) => set('blockAllLinks', e.target.checked)}
            />
            <TextArea
              label={t('settings.moderation.blockedDomains')}
              hint={t('settings.moderation.blockedDomainsHint')}
              value={f.blockDomains}
              onChange={(e) => set('blockDomains', e.target.value)}
            />
            <TextArea
              label={t('settings.moderation.allowedDomains')}
              hint={t('settings.moderation.allowedDomainsHint')}
              value={f.allowDomains}
              onChange={(e) => set('allowDomains', e.target.value)}
            />
          </>
        )}
        {f.trigger === TRIGGER.SPAM && (
          <p className="ss-sub">{t('settings.moderation.spamNote')}</p>
        )}

        <fieldset className="ss-card">
          <legend>{t('settings.moderation.actions')}</legend>
          <Checkbox
            label={t('settings.moderation.blockMessage')}
            checked={f.block}
            onChange={(e) => set('block', e.target.checked)}
          />
          {f.block && (
            <TextInput
              label={t('settings.moderation.customMessage')}
              value={f.blockMessage}
              maxLength={150}
              onChange={(e) => set('blockMessage', e.target.value)}
            />
          )}
          <Checkbox
            label={t('settings.moderation.sendAlert')}
            checked={f.alert}
            onChange={(e) => set('alert', e.target.checked)}
          />
          {f.alert && (
            <div>
              <label className="ss-field-label" htmlFor="ss-rule-alert">
                {t('settings.moderation.alertChannel')}
              </label>
              <select
                id="ss-rule-alert"
                className="ss-select"
                value={f.alertChannel}
                onChange={(e) => set('alertChannel', e.target.value)}
              >
                <option value="">{t('settings.moderation.chooseChannel')}</option>
                {channels.map((c) => (
                  <option key={c.id} value={c.id}>
                    #{c.name}
                  </option>
                ))}
              </select>
            </div>
          )}
          <Checkbox
            label={t('settings.moderation.timeoutMember')}
            checked={f.timeout}
            onChange={(e) => set('timeout', e.target.checked)}
          />
          {f.timeout && (
            <div>
              <label className="ss-field-label" htmlFor="ss-rule-timeout">
                {t('settings.moderation.timeoutDuration')}
              </label>
              <select
                id="ss-rule-timeout"
                className="ss-select"
                value={f.timeoutSeconds}
                onChange={(e) => set('timeoutSeconds', Number(e.target.value))}
              >
                {TIMEOUT_OPTIONS.map((o) => (
                  <option key={o.seconds} value={o.seconds}>
                    {t(o.label)}
                  </option>
                ))}
              </select>
            </div>
          )}
        </fieldset>

        <fieldset className="ss-card">
          <legend>{t('settings.moderation.exemptions')}</legend>
          <MultiSelect
            aria-label={t('settings.moderation.exemptRoles')}
            placeholder={t('settings.moderation.exemptRoles')}
            options={roles
              .filter((r) => r.id !== guildId)
              .map((r) => ({ value: r.id, label: r.name }))}
            values={f.exemptRoles}
            onValuesChange={(v) => set('exemptRoles', v)}
          />
          <MultiSelect
            aria-label={t('settings.moderation.exemptChannels')}
            placeholder={t('settings.moderation.exemptChannels')}
            options={channels.map((c) => ({ value: c.id, label: `#${c.name}` }))}
            values={f.exemptChannels}
            onValuesChange={(v) => set('exemptChannels', v)}
          />
        </fieldset>
        <Switch
          label={t('settings.moderation.ruleEnabled')}
          checked={f.enabled}
          onChange={(e) => set('enabled', e.target.checked)}
        />
      </div>
    </Modal>
  );
}
