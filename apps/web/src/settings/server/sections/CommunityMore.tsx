import { Badge, Button, Switch, TextArea, TextInput } from '@sideroom/ui';
import { useCallback, useMemo, useState } from 'react';
import { useT } from '../../../i18n';
import { useChannels } from '../../../store';
import * as api from '../api';
import * as more from '../apiMore';
import { useDraft, useResource, useSettingsForm } from '../hooks';
import { ActionStatus, Async, formatDate, formatDateTime, useAction } from '../ui';

/** Limits mirrored from crates/sr-chat/src/settings/{guide,applications,community}.rs. */
const GUIDE = {
  welcome: 300,
  todos: 5,
  todoTitle: 100,
  todoDesc: 200,
  res: 10,
  resTitle: 50,
  resBody: 2000,
};
const APPLY = { questions: 5, label: 100, description: 300 };
const RULES = { max: 16, length: 300, description: 300 };

// ---- Server Guide editor (LNG-SSET-066) ----

export function GuideTab({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`guide:${guildId}`, () => more.getGuide(guildId));
  return (
    <Async res={res} label={t('settings.communityMore.loadingGuide')}>
      {(g) => <GuideForm guildId={guildId} guide={g} onSaved={(n) => res.set(n)} />}
    </Async>
  );
}

interface GuideFields {
  welcome: string;
  todos: more.GuideTodo[];
  resources: more.GuideResource[];
}

function GuideForm({
  guildId,
  guide,
  onSaved,
}: {
  guildId: string;
  guide: more.GuideObject;
  onSaved: (g: more.GuideObject) => void;
}) {
  const t = useT();
  const base = useMemo<GuideFields>(
    () => ({
      welcome: guide.welcome_message ?? '',
      todos: guide.todos,
      resources: guide.resources,
    }),
    [guide],
  );
  const draft = useDraft(base);
  const action = useAction();
  const channels = useChannels(guildId).filter((c) => c.type === 0 || c.type === 5);
  const f = draft.value;
  const problem =
    f.todos.some((x) => !x.title.trim()) || f.resources.some((x) => !x.title.trim())
      ? t('settings.communityMore.guideProblem')
      : null;

  const save = useCallback(async () => {
    if (problem) {
      action.setError(problem);
      return;
    }
    const next = await action.run(
      () =>
        more.putGuide(guildId, {
          welcome_message: f.welcome.trim() || null,
          todos: f.todos.map((x) => ({
            ...(x.id ? { id: x.id } : {}),
            title: x.title.trim(),
            description: x.description?.trim() || null,
            channel_id: x.channel_id || null,
          })),
          resources: f.resources.map((x) => ({
            ...(x.id ? { id: x.id } : {}),
            title: x.title.trim(),
            content: x.content,
          })),
        }),
      t('settings.communityMore.guideSaved'),
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

  const setTodo = (i: number, patch: Partial<more.GuideTodo>) =>
    draft.set(
      'todos',
      f.todos.map((x, j) => (j === i ? { ...x, ...patch } : x)),
    );
  const setRes = (i: number, patch: Partial<more.GuideResource>) =>
    draft.set(
      'resources',
      f.resources.map((x, j) => (j === i ? { ...x, ...patch } : x)),
    );

  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      <p className="ss-sub">
        {guide.enabled ? t('settings.communityMore.guideOn') : t('settings.communityMore.guideOff')}
      </p>
      <TextArea
        label={t('settings.communityMore.guideWelcome')}
        value={f.welcome}
        maxLength={GUIDE.welcome}
        onChange={(e) => draft.set('welcome', e.target.value)}
      />
      {f.todos.map((x, i) => (
        <fieldset key={x.id ?? `new${i}`} className="ss-card">
          <legend>{t('settings.communityMore.todoN', { n: i + 1 })}</legend>
          <TextInput
            label={t('settings.communityMore.todoTitle', { n: i + 1 })}
            value={x.title}
            maxLength={GUIDE.todoTitle}
            onChange={(e) => setTodo(i, { title: e.target.value })}
          />
          <TextInput
            label={t('settings.communityMore.todoDescription', { n: i + 1 })}
            value={x.description ?? ''}
            maxLength={GUIDE.todoDesc}
            onChange={(e) => setTodo(i, { description: e.target.value })}
          />
          <div>
            <label className="ss-field-label" htmlFor={`ss-guide-ch-${i}`}>
              {t('settings.communityMore.todoChannel', { n: i + 1 })}
            </label>
            <select
              id={`ss-guide-ch-${i}`}
              className="ss-select"
              value={x.channel_id ?? ''}
              onChange={(e) => setTodo(i, { channel_id: e.target.value || null })}
            >
              <option value="">{t('settings.communityMore.noChannel')}</option>
              {channels.map((c) => (
                <option key={c.id} value={c.id}>
                  #{c.name}
                </option>
              ))}
            </select>
          </div>
          <div>
            <Button
              size="sm"
              variant="ghost"
              onClick={() =>
                draft.set(
                  'todos',
                  f.todos.filter((_, j) => j !== i),
                )
              }
            >
              {t('settings.communityMore.removeTodo', { n: i + 1 })}
            </Button>
          </div>
        </fieldset>
      ))}
      <div>
        <Button
          variant="secondary"
          disabled={f.todos.length >= GUIDE.todos}
          onClick={() =>
            draft.set('todos', [...f.todos, { title: '', description: '', channel_id: null }])
          }
        >
          {t('settings.communityMore.addTodo')}
        </Button>
      </div>
      {f.resources.map((x, i) => (
        <fieldset key={x.id ?? `newr${i}`} className="ss-card">
          <legend>{t('settings.communityMore.resourceN', { n: i + 1 })}</legend>
          <TextInput
            label={t('settings.communityMore.resourceTitle', { n: i + 1 })}
            value={x.title}
            maxLength={GUIDE.resTitle}
            onChange={(e) => setRes(i, { title: e.target.value })}
          />
          <TextArea
            label={t('settings.communityMore.resourceBody', { n: i + 1 })}
            value={x.content}
            maxLength={GUIDE.resBody}
            onChange={(e) => setRes(i, { content: e.target.value })}
          />
          <div>
            <Button
              size="sm"
              variant="ghost"
              onClick={() =>
                draft.set(
                  'resources',
                  f.resources.filter((_, j) => j !== i),
                )
              }
            >
              {t('settings.communityMore.removeResource', { n: i + 1 })}
            </Button>
          </div>
        </fieldset>
      ))}
      <div>
        <Button
          variant="secondary"
          disabled={f.resources.length >= GUIDE.res}
          onClick={() => draft.set('resources', [...f.resources, { title: '', content: '' }])}
        >
          {t('settings.communityMore.addResource')}
        </Button>
      </div>
    </div>
  );
}

// ---- Rules screening editor (LNG-SSET-041) ----

export function RulesTab({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`community:${guildId}`, () => api.getCommunity(guildId));
  return (
    <Async res={res} label={t('settings.communityMore.loadingRules')}>
      {(c) => <RulesForm guildId={guildId} community={c} onSaved={(n) => res.set(n)} />}
    </Async>
  );
}

interface RulesFields {
  enabled: boolean;
  description: string;
  rules: string[];
}

function RulesForm({
  guildId,
  community,
  onSaved,
}: {
  guildId: string;
  community: api.CommunityObject;
  onSaved: (c: api.CommunityObject) => void;
}) {
  const t = useT();
  const base = useMemo<RulesFields>(
    () => ({
      enabled: !!community.screening_enabled,
      description: community.screening_description ?? '',
      rules: community.screening_rules ?? [],
    }),
    [community],
  );
  const draft = useDraft(base);
  const action = useAction();
  const f = draft.value;
  const problem =
    f.enabled && (f.rules.length === 0 || f.rules.some((r) => !r.trim()))
      ? t('settings.communityMore.rulesProblem')
      : null;

  const save = useCallback(async () => {
    if (problem) {
      action.setError(problem);
      return;
    }
    const next = await action.run(
      () =>
        api.modifyCommunity(guildId, {
          screening_enabled: f.enabled,
          screening_description: f.description.trim() || null,
          screening_rules: f.rules.map((r) => r.trim()).filter(Boolean),
        }),
      t('settings.communityMore.rulesSaved'),
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

  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      {!community.community_enabled && (
        <p className="ss-sub">{t('settings.communityMore.rulesNeedCommunity')}</p>
      )}
      <Switch
        label={t('settings.communityMore.rulesEnable')}
        description={t('settings.communityMore.rulesEnableHint')}
        checked={f.enabled}
        onChange={(e) => draft.set('enabled', e.target.checked)}
      />
      <TextArea
        label={t('settings.communityMore.rulesDescription')}
        value={f.description}
        maxLength={RULES.description}
        onChange={(e) => draft.set('description', e.target.value)}
      />
      {f.rules.map((r, i) => (
        <div key={i} className="ss-row">
          <div className="ss-grow">
            <TextInput
              label={t('settings.communityMore.ruleN', { n: i + 1 })}
              value={r}
              maxLength={RULES.length}
              onChange={(e) =>
                draft.set(
                  'rules',
                  f.rules.map((x, j) => (j === i ? e.target.value : x)),
                )
              }
            />
          </div>
          <Button
            size="sm"
            variant="ghost"
            onClick={() =>
              draft.set(
                'rules',
                f.rules.filter((_, j) => j !== i),
              )
            }
          >
            {t('settings.communityMore.removeRule', { n: i + 1 })}
          </Button>
        </div>
      ))}
      <div>
        <Button
          variant="secondary"
          disabled={f.rules.length >= RULES.max}
          onClick={() => draft.set('rules', [...f.rules, ''])}
        >
          {t('settings.communityMore.addRule')}
        </Button>
      </div>
    </div>
  );
}

// ---- Access: apply to join, applications queue (LNG-SSET-039, LNG-SSET-042) ----

export function AccessTab({ guildId }: { guildId: string }) {
  const t = useT();
  const res = useResource(`appform:${guildId}`, () => more.getApplicationForm(guildId));
  return (
    <Async res={res} label={t('settings.communityMore.loadingAccess')}>
      {(form) => (
        <>
          <AccessSummary guildId={guildId} form={form} />
          <ApplicationFormEditor guildId={guildId} form={form} onSaved={(n) => res.set(n)} />
          <ApplicationQueue guildId={guildId} questions={form.questions} />
        </>
      )}
    </Async>
  );
}

/** Tells how people can get in today: invites, applying, and whether the Directory lists the server. */
function AccessSummary({ guildId, form }: { guildId: string; form: more.ApplicationFormObject }) {
  const t = useT();
  const community = useResource(`community:${guildId}`, () => api.getCommunity(guildId));
  const listing = useResource(`listing:${guildId}`, () => more.getOwnListing(guildId));
  const listed = listing.data?.status === 'approved';
  return (
    <section className="ss-card" aria-label={t('settings.communityMore.accessTitle')}>
      <h2>{t('settings.communityMore.accessTitle')}</h2>
      <ul className="ss-list">
        <li>
          {community.data?.invites_paused
            ? t('settings.communityMore.accessInvitesPaused')
            : t('settings.communityMore.accessInvites')}
        </li>
        <li>
          {form.enabled
            ? t('settings.communityMore.accessApplyOn')
            : t('settings.communityMore.accessApplyOff')}
        </li>
        <li>
          {listed
            ? t('settings.communityMore.accessListed')
            : t('settings.communityMore.accessNotListed')}
        </li>
      </ul>
    </section>
  );
}

interface FormFields {
  enabled: boolean;
  description: string;
  questions: more.ApplicationQuestion[];
}

function ApplicationFormEditor({
  guildId,
  form,
  onSaved,
}: {
  guildId: string;
  form: more.ApplicationFormObject;
  onSaved: (f: more.ApplicationFormObject) => void;
}) {
  const t = useT();
  const base = useMemo<FormFields>(
    () => ({
      enabled: form.enabled,
      description: form.description ?? '',
      questions: form.questions,
    }),
    [form],
  );
  const draft = useDraft(base);
  const action = useAction();
  const f = draft.value;
  const problem = f.questions.some((q) => !q.label.trim())
    ? t('settings.communityMore.applyProblem')
    : null;

  const save = useCallback(async () => {
    if (problem) {
      action.setError(problem);
      return;
    }
    const next = await action.run(
      () =>
        more.putApplicationForm(guildId, {
          enabled: f.enabled,
          description: f.description.trim() || null,
          questions: f.questions.map((q) => ({
            ...(q.id ? { id: q.id } : {}),
            label: q.label.trim(),
            kind: q.kind,
            required: q.required,
          })),
        }),
      t('settings.communityMore.applySaved'),
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

  const setQ = (i: number, patch: Partial<more.ApplicationQuestion>) =>
    draft.set(
      'questions',
      f.questions.map((q, j) => (j === i ? { ...q, ...patch } : q)),
    );

  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      <Switch
        label={t('settings.communityMore.applyEnable')}
        description={t('settings.communityMore.applyEnableHint')}
        checked={f.enabled}
        onChange={(e) => draft.set('enabled', e.target.checked)}
      />
      <TextArea
        label={t('settings.communityMore.applyDescription')}
        value={f.description}
        maxLength={APPLY.description}
        onChange={(e) => draft.set('description', e.target.value)}
      />
      {f.questions.map((q, i) => (
        <fieldset key={q.id ?? `newq${i}`} className="ss-card">
          <legend>{t('settings.communityMore.questionN', { n: i + 1 })}</legend>
          <TextInput
            label={t('settings.communityMore.questionLabel', { n: i + 1 })}
            value={q.label}
            maxLength={APPLY.label}
            onChange={(e) => setQ(i, { label: e.target.value })}
          />
          <Switch
            label={t('settings.communityMore.questionLong', { n: i + 1 })}
            checked={q.kind === 'paragraph'}
            onChange={(e) => setQ(i, { kind: e.target.checked ? 'paragraph' : 'short' })}
          />
          <Switch
            label={t('settings.communityMore.questionRequired', { n: i + 1 })}
            checked={q.required}
            onChange={(e) => setQ(i, { required: e.target.checked })}
          />
          <div>
            <Button
              size="sm"
              variant="ghost"
              onClick={() =>
                draft.set(
                  'questions',
                  f.questions.filter((_, j) => j !== i),
                )
              }
            >
              {t('settings.communityMore.removeQuestion', { n: i + 1 })}
            </Button>
          </div>
        </fieldset>
      ))}
      <div>
        <Button
          variant="secondary"
          disabled={f.questions.length >= APPLY.questions}
          onClick={() =>
            draft.set('questions', [...f.questions, { label: '', kind: 'short', required: false }])
          }
        >
          {t('settings.communityMore.addQuestion')}
        </Button>
      </div>
    </div>
  );
}

function ApplicationQueue({
  guildId,
  questions,
}: {
  guildId: string;
  questions: more.ApplicationQuestion[];
}) {
  const t = useT();
  const res = useResource(`applications:${guildId}`, () => more.listApplications(guildId));
  return (
    <section className="ss-card" aria-label={t('settings.communityMore.queueTitle')}>
      <h2>{t('settings.communityMore.queueTitle')}</h2>
      <Async res={res} label={t('settings.communityMore.loadingQueue')}>
        {(apps) =>
          apps.length === 0 ? (
            <p className="ss-sub">{t('settings.communityMore.queueEmpty')}</p>
          ) : (
            <ul className="ss-list">
              {apps.map((a) => (
                <ApplicationRow
                  key={a.id}
                  guildId={guildId}
                  app={a}
                  questions={questions}
                  onChanged={(n) =>
                    res.set((cur) => (cur ?? []).map((x) => (x.id === n.id ? n : x)))
                  }
                />
              ))}
            </ul>
          )
        }
      </Async>
    </section>
  );
}

function ApplicationRow({
  guildId,
  app,
  questions,
  onChanged,
}: {
  guildId: string;
  app: more.ApplicationObject;
  questions: more.ApplicationQuestion[];
  onChanged: (a: more.ApplicationObject) => void;
}) {
  const t = useT();
  const action = useAction();
  const [reason, setReason] = useState('');
  const name = app.user.global_name || app.user.username;
  const decide = async (approve: boolean) => {
    const next = await action.run(() =>
      more.decideApplication(guildId, app.id, {
        approve,
        ...(!approve && reason.trim() ? { reason: reason.trim() } : {}),
      }),
    );
    if (next) onChanged(next);
  };
  const label = (id: string) => questions.find((q) => q.id === id)?.label ?? id;
  const statusText =
    app.status === 'pending'
      ? t('settings.communityMore.statusPending')
      : app.status === 'approved'
        ? t('settings.communityMore.statusApproved')
        : t('settings.communityMore.statusRejected');
  return (
    <li className="ss-card" aria-label={name}>
      <div className="ss-row">
        <div className="ss-grow">
          <strong className="ss-name">{name}</strong>
          <div className="ss-meta">{formatDateTime(app.created_at)}</div>
        </div>
        <Badge>{statusText}</Badge>
      </div>
      <dl>
        {app.answers.map((x) => (
          <div key={x.question_id}>
            <dt className="ss-sub">{label(x.question_id)}</dt>
            <dd>{x.value}</dd>
          </div>
        ))}
      </dl>
      {app.status === 'rejected' && app.rejection_reason && (
        <p className="ss-sub">{app.rejection_reason}</p>
      )}
      {app.status === 'pending' && (
        <>
          <ActionStatus error={action.error} />
          <TextInput
            label={t('settings.communityMore.rejectReason', { name })}
            value={reason}
            maxLength={512}
            onChange={(e) => setReason(e.target.value)}
          />
          <div className="ss-actions">
            <Button
              size="sm"
              disabled={action.busy}
              aria-label={t('settings.communityMore.approve', { name })}
              onClick={() => void decide(true)}
            >
              {t('settings.communityMore.approveShort')}
            </Button>
            <Button
              size="sm"
              variant="danger"
              disabled={action.busy}
              aria-label={t('settings.communityMore.reject', { name })}
              onClick={() => void decide(false)}
            >
              {t('settings.communityMore.rejectShort')}
            </Button>
          </div>
        </>
      )}
    </li>
  );
}

// ---- Insights (LNG-SSET-068) ----

export function InsightsTab({ guildId }: { guildId: string }) {
  const t = useT();
  const [days, setDays] = useState(30);
  const res = useResource(`insights:${guildId}:${days}`, () => more.getInsights(guildId, days));
  const channels = useChannels(guildId);
  return (
    <div className="ss-form">
      <div>
        <label className="ss-field-label" htmlFor="ss-insights-days">
          {t('settings.communityMore.insightsRange')}
        </label>
        <select
          id="ss-insights-days"
          className="ss-select"
          value={days}
          onChange={(e) => setDays(Number(e.target.value))}
        >
          {[7, 30, 90].map((d) => (
            <option key={d} value={d}>
              {t('settings.communityMore.insightsDays', { count: d })}
            </option>
          ))}
        </select>
      </div>
      <Async res={res} label={t('settings.communityMore.loadingInsights')}>
        {(ins) => {
          if (ins.days.length === 0)
            return <p className="ss-sub">{t('settings.communityMore.insightsEmpty')}</p>;
          const last = ins.days[ins.days.length - 1]!;
          const first = ins.days[0]!;
          const sum = (k: 'joins' | 'messages' | 'voice_minutes') =>
            ins.days.reduce((a, d) => a + d[k], 0);
          const maxMsg = Math.max(1, ...ins.days.map((d) => d.messages));
          const maxMembers = Math.max(1, ...ins.days.map((d) => d.members));
          return (
            <>
              <ul className="ss-list" aria-label={t('settings.communityMore.insightsTotals')}>
                <li>
                  {t('settings.communityMore.insightsMembers', {
                    count: last.members,
                    change: last.members - first.members,
                  })}
                </li>
                <li>{t('settings.communityMore.insightsJoins', { count: sum('joins') })}</li>
                <li>{t('settings.communityMore.insightsMessages', { count: sum('messages') })}</li>
                <li>
                  {t('settings.communityMore.insightsVoice', { count: sum('voice_minutes') })}
                </li>
              </ul>
              <table className="ss-table">
                <caption>{t('settings.communityMore.insightsPerDay')}</caption>
                <thead>
                  <tr>
                    <th scope="col">{t('settings.communityMore.colDate')}</th>
                    <th scope="col">{t('settings.communityMore.colMembers')}</th>
                    <th scope="col">{t('settings.communityMore.colMessages')}</th>
                  </tr>
                </thead>
                <tbody>
                  {ins.days.map((d) => (
                    <tr key={d.date}>
                      <th scope="row">{formatDate(`${d.date}T00:00:00Z`)}</th>
                      <td>
                        <span
                          className="ss-bar"
                          aria-hidden="true"
                          style={{ inlineSize: `${Math.round((d.members / maxMembers) * 100)}%` }}
                        />
                        {d.members}
                      </td>
                      <td>
                        <span
                          className="ss-bar"
                          aria-hidden="true"
                          style={{ inlineSize: `${Math.round((d.messages / maxMsg) * 100)}%` }}
                        />
                        {d.messages}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <section className="ss-card" aria-label={t('settings.communityMore.insightsTop')}>
                <h2>{t('settings.communityMore.insightsTop')}</h2>
                {ins.top_channels.length === 0 ? (
                  <p className="ss-sub">{t('settings.communityMore.insightsNoTop')}</p>
                ) : (
                  <ol className="ss-list">
                    {ins.top_channels.map((c) => (
                      <li key={c.channel_id}>
                        {t('settings.communityMore.insightsTopRow', {
                          name: channels.find((x) => x.id === c.channel_id)?.name ?? c.channel_id,
                          count: c.messages,
                        })}
                      </li>
                    ))}
                  </ol>
                )}
              </section>
            </>
          );
        }}
      </Async>
    </div>
  );
}

// ---- Directory listing request (LNG-SSET-069) ----

export function DirectoryTab({ guildId }: { guildId: string }) {
  const t = useT();
  const listing = useResource(`listing:${guildId}`, () => more.getOwnListing(guildId));
  const cats = useResource('directory:categories', () => more.listDirectoryCategories());
  return (
    <Async res={listing} label={t('settings.communityMore.loadingDirectory')}>
      {(l) => (
        <Async res={cats} label={t('settings.communityMore.loadingDirectory')}>
          {(categories) => (
            <ListingForm
              guildId={guildId}
              listing={l}
              categories={categories}
              onChanged={(n) => listing.set(n)}
            />
          )}
        </Async>
      )}
    </Async>
  );
}

const prettyCategory = (id: string) =>
  id.replace(/[-_]+/g, ' ').replace(/^./, (c) => c.toUpperCase());

function ListingForm({
  guildId,
  listing,
  categories,
  onChanged,
}: {
  guildId: string;
  listing: more.ListingObject | null;
  categories: string[];
  onChanged: (l: more.ListingObject | null) => void;
}) {
  const t = useT();
  const action = useAction();
  const [category, setCategory] = useState(listing?.category ?? categories[0] ?? '');
  const [description, setDescription] = useState(listing?.description ?? '');
  const submit = async () => {
    const next = await action.run(
      () => more.requestListing(guildId, { category, description: description.trim() }),
      t('settings.communityMore.directorySaved'),
    );
    if (next) onChanged(next);
  };
  const withdraw = async () => {
    const ok = await action.run(async () => {
      await more.withdrawListing(guildId);
      return true;
    }, t('settings.communityMore.directoryWithdrawn'));
    if (ok) onChanged(null);
  };
  const status =
    listing?.status === 'approved'
      ? t('settings.communityMore.listingApproved')
      : listing?.status === 'rejected'
        ? t('settings.communityMore.listingRejected')
        : listing?.status === 'removed'
          ? t('settings.communityMore.listingRemoved')
          : t('settings.communityMore.listingPending');
  return (
    <div className="ss-form">
      <ActionStatus error={action.error} done={action.done} />
      {listing ? (
        <section className="ss-card" aria-label={t('settings.communityMore.directoryStatus')}>
          <h2>{t('settings.communityMore.directoryStatus')}</h2>
          <p>
            <Badge>{status}</Badge>{' '}
            <span className="ss-sub">
              {t('settings.communityMore.directoryRequested', {
                date: formatDate(listing.requested_at),
              })}
            </span>
          </p>
          {listing.decision_note && (
            <p>
              <strong>{t('settings.communityMore.directoryNote')}</strong> {listing.decision_note}
            </p>
          )}
          {listing.status === 'approved' && (
            <p className="ss-sub">{t('settings.communityMore.directoryEditResets')}</p>
          )}
        </section>
      ) : (
        <p className="ss-sub">{t('settings.communityMore.directoryIntro')}</p>
      )}
      <div>
        <label className="ss-field-label" htmlFor="ss-listing-category">
          {t('settings.communityMore.directoryCategory')}
        </label>
        <select
          id="ss-listing-category"
          className="ss-select"
          value={category}
          onChange={(e) => setCategory(e.target.value)}
        >
          {categories.map((c) => (
            <option key={c} value={c}>
              {prettyCategory(c)}
            </option>
          ))}
        </select>
      </div>
      <TextArea
        label={t('settings.communityMore.directoryDescription')}
        value={description}
        maxLength={300}
        onChange={(e) => setDescription(e.target.value)}
      />
      <div className="ss-actions">
        <Button disabled={action.busy || !category} onClick={() => void submit()}>
          {listing
            ? t('settings.communityMore.directoryUpdate')
            : t('settings.communityMore.directoryRequest')}
        </Button>
        {listing && (
          <Button variant="danger" disabled={action.busy} onClick={() => void withdraw()}>
            {t('settings.communityMore.directoryWithdraw')}
          </Button>
        )}
      </div>
    </div>
  );
}
