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
import { useChannels, useGuild } from '../../../store';
import * as api from '../api';
import { ACTION, TRIGGER } from '../api';
import { useDraft, useGuildEvents, useResource, useSettingsForm } from '../hooks';
import type { SectionProps } from '../ServerSettings';
import { ActionStatus, Async, SectionHeader, useAction } from '../ui';

export const VERIFICATION_LEVELS = [
  { value: 0, label: 'None', hint: 'Unrestricted.' },
  { value: 1, label: 'Low', hint: 'Must have a verified email address.' },
  { value: 2, label: 'Medium', hint: 'Also registered for more than 5 minutes.' },
  { value: 3, label: 'High', hint: 'Also a member of this server for more than 10 minutes.' },
  { value: 4, label: 'Highest', hint: 'Must have two-factor authentication enabled.' },
];
export const CONTENT_FILTERS = [
  { value: 0, label: "Don't scan any media content" },
  { value: 1, label: 'Scan media from members without a role' },
  { value: 2, label: 'Scan media from all members' },
];

export const TRIGGER_LABEL: Record<number, string> = {
  [TRIGGER.KEYWORD]: 'Block custom words',
  [TRIGGER.SPAM]: 'Block spam content',
  [TRIGGER.PRESET]: 'Block preset word lists',
  [TRIGGER.MENTION_SPAM]: 'Block mention spam',
  [TRIGGER.MEMBER_PROFILE]: 'Flag words in profiles',
  [TRIGGER.LINKS]: 'Block links',
};
const CREATABLE = [
  TRIGGER.KEYWORD,
  TRIGGER.MENTION_SPAM,
  TRIGGER.LINKS,
  TRIGGER.SPAM,
  TRIGGER.PRESET,
];
const PRESETS = [
  { id: 1, label: 'Profanity' },
  { id: 2, label: 'Sexual content' },
  { id: 3, label: 'Slurs' },
];
export const TIMEOUT_OPTIONS = [
  { seconds: 60, label: '60 seconds' },
  { seconds: 300, label: '5 minutes' },
  { seconds: 600, label: '10 minutes' },
  { seconds: 3600, label: '1 hour' },
  { seconds: 86400, label: '1 day' },
  { seconds: 604800, label: '1 week' },
];

const lines = (s: string) =>
  s
    .split(/[\n,]/)
    .map((x) => x.trim())
    .filter(Boolean);

export function ModerationSection({ guildId }: SectionProps) {
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
        title="AutoMod"
        description="Let AutoMod catch unwanted messages before anyone sees them."
      />
      <SafetyForm guildId={guildId} />
      <h2 className="ss-title" style={{ fontSize: '1rem', margin: '32px 0 12px' }}>
        Rules
      </h2>
      <div className="ss-actions" style={{ marginBottom: 12 }}>
        <Button onClick={() => setEditing('new')}>Create rule</Button>
      </div>
      <ActionStatus error={action.error} />
      <Async res={rules} label="Loading rules">
        {(list) =>
          list.length === 0 ? (
            <EmptyState
              title="No AutoMod rules"
              description="Create a rule to block words, links or mention spam."
            />
          ) : (
            <ul className="ss-list" aria-label="AutoMod rules">
              {list.map((r) => (
                <li key={r.id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">{r.name}</div>
                    <div className="ss-meta">
                      {TRIGGER_LABEL[r.trigger_type] ?? `Trigger ${r.trigger_type}`} -{' '}
                      {r.actions
                        .map((a) =>
                          a.type === ACTION.BLOCK
                            ? 'Block'
                            : a.type === ACTION.ALERT
                              ? 'Alert'
                              : 'Timeout',
                        )
                        .join(', ') || 'No actions'}
                    </div>
                  </div>
                  <Badge variant={r.enabled ? 'success' : 'neutral'}>
                    {r.enabled ? 'On' : 'Off'}
                  </Badge>
                  <Switch
                    label={<span className="ss-sr">{`Enable ${r.name}`}</span>}
                    checked={r.enabled}
                    disabled={action.busy}
                    onChange={(e) => void toggle(r, e.target.checked)}
                  />
                  <Button
                    size="sm"
                    variant="secondary"
                    aria-label={`Edit rule ${r.name}`}
                    onClick={() => setEditing(r)}
                  >
                    Edit
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    aria-label={`Delete rule ${r.name}`}
                    onClick={() => setToDelete(r)}
                  >
                    Delete
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
        title={`Delete ${toDelete?.name ?? 'rule'}`}
        description="AutoMod stops enforcing this rule immediately."
        confirmLabel="Delete rule"
        loading={action.busy}
        onConfirm={() => void remove()}
        onCancel={() => setToDelete(null)}
      />
    </>
  );
}

function SafetyForm({ guildId }: { guildId: string }) {
  const guild = useResource(`guild:${guildId}`, () => api.getGuild(guildId));
  useGuildEvents(guildId, ['GUILD_UPDATE'], () => void guild.reload());
  return (
    <Async res={guild} label="Loading safety settings">
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
  const draft = useDraft(base);
  const action = useAction();
  const save = useCallback(async () => {
    const next = await action.run(
      () => api.modifyGuild(guildId, draft.changed),
      'Safety settings saved.',
    );
    if (next) {
      onSaved(next);
      draft.reset();
    }
  }, [action, guildId, draft, onSaved]);
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
        legend="Verification level"
        value={String(draft.value.verification_level)}
        onValueChange={(v) => draft.set('verification_level', Number(v))}
      >
        {VERIFICATION_LEVELS.map((l) => (
          <Radio key={l.value} value={String(l.value)} label={l.label} description={l.hint} />
        ))}
      </RadioGroup>
      <RadioGroup
        legend="Explicit media content filter"
        value={String(draft.value.explicit_content_filter)}
        onValueChange={(v) => draft.set('explicit_content_filter', Number(v))}
      >
        {CONTENT_FILTERS.map((l) => (
          <Radio key={l.value} value={String(l.value)} label={l.label} />
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
    return 'Give the rule a name (up to 100 characters).';
  if (!f.block && !f.alert && !f.timeout) return 'Choose at least one action.';
  if (f.alert && !f.alertChannel) return 'Choose a channel for the alert.';
  if (f.trigger === TRIGGER.KEYWORD) {
    if (lines(f.keywords).length === 0 && lines(f.regex).length === 0)
      return 'Add at least one keyword or regular expression.';
    if (lines(f.keywords).length > 1000) return 'A rule can have at most 1000 keywords.';
    if (lines(f.regex).length > 10) return 'A rule can have at most 10 regular expressions.';
    for (const p of lines(f.regex)) {
      try {
        new RegExp(p);
      } catch {
        return `"${p}" is not a valid regular expression.`;
      }
    }
  }
  if (f.trigger === TRIGGER.PRESET && f.presets.length === 0)
    return 'Choose at least one word list.';
  if (f.trigger === TRIGGER.MENTION_SPAM && (f.mentionLimit < 1 || f.mentionLimit > 50))
    return 'The mention limit must be between 1 and 50.';
  if (f.trigger === TRIGGER.LINKS && !f.blockAllLinks && lines(f.blockDomains).length === 0)
    return 'Block all links or list at least one domain.';
  if (f.block && f.blockMessage.length > 150)
    return 'The custom message can be at most 150 characters.';
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
        if (new RegExp(p, 'i').test(sample)) return `Matches /${p}/`;
      } catch {
        return `/${p}/ is not valid`;
      }
    }
    return 'No match';
  })();

  return (
    <Modal
      open
      onClose={onClose}
      size="lg"
      title={rule ? `Edit ${rule.name}` : 'Create AutoMod rule'}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button loading={action.busy} onClick={() => void submit()}>
            {rule ? 'Save rule' : 'Create rule'}
          </Button>
        </>
      }
    >
      <div className="ss-form" style={{ maxWidth: 'none' }}>
        <ActionStatus error={action.error} />
        {touched && problem && <ActionStatus error={problem} />}
        <TextInput
          label="Rule name"
          value={f.name}
          maxLength={100}
          onChange={(e) => set('name', e.target.value)}
        />
        {creating ? (
          <div>
            <label className="ss-field-label" htmlFor="ss-rule-trigger">
              Trigger
            </label>
            <select
              id="ss-rule-trigger"
              className="ss-select"
              value={f.trigger}
              onChange={(e) => set('trigger', Number(e.target.value))}
            >
              {CREATABLE.map((t) => (
                <option key={t} value={t}>
                  {TRIGGER_LABEL[t]}
                </option>
              ))}
            </select>
          </div>
        ) : (
          <p className="ss-sub">
            Trigger: {TRIGGER_LABEL[f.trigger] ?? f.trigger} (cannot be changed)
          </p>
        )}

        {(f.trigger === TRIGGER.KEYWORD || f.trigger === TRIGGER.MEMBER_PROFILE) && (
          <>
            <TextArea
              label="Keywords"
              hint="One per line or separated by commas. Use * as a wildcard, for example *bad*. Up to 1000."
              value={f.keywords}
              onChange={(e) => set('keywords', e.target.value)}
            />
            <TextArea
              label="Regular expressions"
              hint="One per line, up to 10. Rust regex syntax; the tester below uses your browser's engine, which differs in edge cases."
              value={f.regex}
              onChange={(e) => set('regex', e.target.value)}
            />
            <TextInput
              label="Regex tester"
              placeholder="Type a sample message"
              value={sample}
              onChange={(e) => setSample(e.target.value)}
              hint={regexResult ?? undefined}
            />
            <TextArea
              label="Allowed words"
              hint="Messages containing only these words are never blocked."
              value={f.allow}
              onChange={(e) => set('allow', e.target.value)}
            />
          </>
        )}
        {f.trigger === TRIGGER.PRESET && (
          <fieldset className="ss-card">
            <legend>Word lists</legend>
            {PRESETS.map((p) => (
              <Checkbox
                key={p.id}
                label={p.label}
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
              label="Mention limit per message"
              type="number"
              min={1}
              max={50}
              value={String(f.mentionLimit)}
              onChange={(e) => set('mentionLimit', Number(e.target.value))}
            />
            <Switch
              label="Mention raid protection"
              description="Also block bursts of mentions across several messages."
              checked={f.raidProtection}
              onChange={(e) => set('raidProtection', e.target.checked)}
            />
          </>
        )}
        {f.trigger === TRIGGER.LINKS && (
          <>
            <Switch
              label="Block all links"
              checked={f.blockAllLinks}
              onChange={(e) => set('blockAllLinks', e.target.checked)}
            />
            <TextArea
              label="Blocked domains"
              hint="One per line."
              value={f.blockDomains}
              onChange={(e) => set('blockDomains', e.target.value)}
            />
            <TextArea
              label="Allowed domains"
              hint="Links to these domains are never blocked."
              value={f.allowDomains}
              onChange={(e) => set('allowDomains', e.target.value)}
            />
          </>
        )}
        {f.trigger === TRIGGER.SPAM && (
          <p className="ss-sub">
            Blocks duplicate messages and message floods. It has no extra settings.
          </p>
        )}

        <fieldset className="ss-card">
          <legend>Actions</legend>
          <Checkbox
            label="Block message"
            checked={f.block}
            onChange={(e) => set('block', e.target.checked)}
          />
          {f.block && (
            <TextInput
              label="Custom message shown to the member"
              value={f.blockMessage}
              maxLength={150}
              onChange={(e) => set('blockMessage', e.target.value)}
            />
          )}
          <Checkbox
            label="Send alert to a channel"
            checked={f.alert}
            onChange={(e) => set('alert', e.target.checked)}
          />
          {f.alert && (
            <div>
              <label className="ss-field-label" htmlFor="ss-rule-alert">
                Alert channel
              </label>
              <select
                id="ss-rule-alert"
                className="ss-select"
                value={f.alertChannel}
                onChange={(e) => set('alertChannel', e.target.value)}
              >
                <option value="">Choose a channel</option>
                {channels.map((c) => (
                  <option key={c.id} value={c.id}>
                    #{c.name}
                  </option>
                ))}
              </select>
            </div>
          )}
          <Checkbox
            label="Timeout member"
            checked={f.timeout}
            onChange={(e) => set('timeout', e.target.checked)}
          />
          {f.timeout && (
            <div>
              <label className="ss-field-label" htmlFor="ss-rule-timeout">
                Timeout duration
              </label>
              <select
                id="ss-rule-timeout"
                className="ss-select"
                value={f.timeoutSeconds}
                onChange={(e) => set('timeoutSeconds', Number(e.target.value))}
              >
                {TIMEOUT_OPTIONS.map((t) => (
                  <option key={t.seconds} value={t.seconds}>
                    {t.label}
                  </option>
                ))}
              </select>
            </div>
          )}
        </fieldset>

        <fieldset className="ss-card">
          <legend>Exemptions</legend>
          <MultiSelect
            aria-label="Exempt roles"
            placeholder="Exempt roles"
            options={roles
              .filter((r) => r.id !== guildId)
              .map((r) => ({ value: r.id, label: r.name }))}
            values={f.exemptRoles}
            onValuesChange={(v) => set('exemptRoles', v)}
          />
          <MultiSelect
            aria-label="Exempt channels"
            placeholder="Exempt channels"
            options={channels.map((c) => ({ value: c.id, label: `#${c.name}` }))}
            values={f.exemptChannels}
            onValuesChange={(v) => set('exemptChannels', v)}
          />
        </fieldset>
        <Switch
          label="Rule enabled"
          checked={f.enabled}
          onChange={(e) => set('enabled', e.target.checked)}
        />
      </div>
    </Modal>
  );
}
