import { Button, Checkbox, Notice, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { t, useT } from '../../i18n';
import { OPTION_TYPES, type CommandInput, type CommandOption, type DevApp } from '../types';
import { CodeBlock } from '../ui';

export const MAX_OPTIONS = 25;
const NAME = /^[-_\p{L}\p{N}]{1,32}$/u;

/** Validation errors for a command draft, keyed by field. Mirrors the rules the server enforces (spec 3.5). */
export function validateCommand(c: CommandInput): Record<string, string> {
  const e: Record<string, string> = {};
  if (c.type === 1) {
    if (!NAME.test(c.name) || c.name !== c.name.toLowerCase())
      e.name = t('developers.commands.nameInvalid');
    if (c.description.trim().length < 1 || c.description.length > 100)
      e.description = t('developers.commands.describeInvalid');
  } else if (c.name.trim().length < 1 || c.name.length > 32) {
    e.name = t('developers.commands.nameLength');
  }
  if (c.options.length > MAX_OPTIONS)
    e.options = t('developers.commands.maxOptions', { max: MAX_OPTIONS });
  const seen = new Set<string>();
  let sawOptional = false;
  c.options.forEach((o, i) => {
    if (!NAME.test(o.name) || o.name !== o.name.toLowerCase())
      e[`option.${i}.name`] = t('developers.commands.nameInvalid');
    else if (seen.has(o.name)) e[`option.${i}.name`] = t('developers.commands.optionNameUnique');
    seen.add(o.name);
    if (o.description.trim().length < 1 || o.description.length > 100)
      e[`option.${i}.description`] = t('developers.commands.describeInvalid');
    if (o.required && sawOptional)
      e[`option.${i}.required`] = t('developers.commands.requiredFirst');
    if (!o.required) sawOptional = true;
  });
  return e;
}

/** The JSON body and curl call for a valid draft. */
export function commandRequest(
  origin: string,
  appId: string,
  guildId: string,
  c: CommandInput,
): { body: string; curl: string } {
  const body = JSON.stringify(
    {
      type: c.type,
      name: c.name,
      ...(c.type === 1 ? { description: c.description, options: c.options } : {}),
    },
    null,
    2,
  );
  const path = guildId
    ? `/api/v1/applications/${appId}/guilds/${guildId}/commands`
    : `/api/v1/applications/${appId}/commands`;
  return {
    body,
    curl: `curl -X POST ${origin}${path} \\\n  -H "Authorization: Bot $BOT_TOKEN" \\\n  -H "Content-Type: application/json" \\\n  -d '${body.replace(/'/g, "'\\''")}'`,
  };
}

const emptyOption = (): CommandOption => ({ type: 3, name: '', description: '', required: false });

/**
 * Command builder. The server manages application commands with the bot token only (spec 3.5): a signed-in
 * session cannot list, create or delete them, so this page checks a command against the rules and writes
 * the request to copy, rather than pretending to manage a list it cannot read.
 */
export function CommandsTab({ app }: { app: DevApp }) {
  const tr = useT();
  const [draft, setDraft] = useState<CommandInput>({
    type: 1,
    name: '',
    description: '',
    options: [],
  });
  const [guildId, setGuildId] = useState('');
  const problems = validateCommand(draft);
  const guildProblem =
    guildId !== '' && !/^[0-9]+$/.test(guildId) ? tr('developers.commands.guildDigits') : undefined;
  const valid = Object.keys(problems).length === 0 && !guildProblem && draft.name !== '';
  const set = (patch: Partial<CommandInput>) => setDraft((d) => ({ ...d, ...patch }));
  const setOption = (i: number, patch: Partial<CommandOption>) =>
    set({ options: draft.options.map((o, j) => (j === i ? { ...o, ...patch } : o)) });
  const touched = (k: string) =>
    draft.name !== '' || draft.description !== '' ? problems[k] : undefined;
  const req = valid ? commandRequest(window.location.origin, app.id, guildId, draft) : null;

  return (
    <div className="dev-stack">
      <Notice variant="info" title={tr('developers.commands.noticeTitle')}>
        {tr('developers.commands.noticeBody')}
      </Notice>
      <section className="dev-card" aria-labelledby="dev-cmds-h">
        <h2 id="dev-cmds-h" className="dev-card-title">
          {tr('developers.commands.builderHeading')}
        </h2>
        <div className="dev-form">
          <TextInput
            label={tr('developers.commands.guildLabel')}
            hint={tr('developers.commands.guildHint')}
            value={guildId}
            onChange={(e) => setGuildId(e.target.value.trim())}
            error={guildProblem}
            inputMode="numeric"
            autoComplete="off"
          />
          <div className="dev-field">
            <label className="dev-label" htmlFor="cmd-type">
              {tr('developers.commands.typeLabel')}
            </label>
            <select
              id="cmd-type"
              className="dev-select"
              value={draft.type}
              onChange={(e) => {
                const type = Number(e.target.value) as CommandInput['type'];
                set({ type, ...(type === 1 ? {} : { description: '', options: [] }) });
              }}
            >
              <option value={1}>{tr('developers.commands.typeSlash')}</option>
              <option value={2}>{tr('developers.commands.typeUser')}</option>
              <option value={3}>{tr('developers.commands.typeMessage')}</option>
            </select>
          </div>
          <TextInput
            label={tr('developers.commands.nameLabel')}
            value={draft.name}
            maxLength={32}
            onChange={(e) =>
              set({ name: draft.type === 1 ? e.target.value.toLowerCase() : e.target.value })
            }
            error={touched('name')}
            autoComplete="off"
          />
          {draft.type === 1 && (
            <TextInput
              label={tr('developers.commands.descriptionLabel')}
              value={draft.description}
              maxLength={100}
              onChange={(e) => set({ description: e.target.value })}
              error={touched('description')}
            />
          )}

          {draft.type === 1 && (
            <fieldset className="dev-fieldset">
              <legend>{tr('developers.commands.optionsLegend')}</legend>
              {draft.options.length === 0 && (
                <p className="dev-muted">{tr('developers.commands.noOptions')}</p>
              )}
              {draft.options.map((o, i) => (
                <div
                  key={i}
                  className="dev-option"
                  role="group"
                  aria-label={tr('developers.commands.optionGroup', { n: i + 1 })}
                >
                  <TextInput
                    label={tr('developers.commands.optionName', { n: i + 1 })}
                    value={o.name}
                    maxLength={32}
                    onChange={(e) => setOption(i, { name: e.target.value.toLowerCase() })}
                    error={touched(`option.${i}.name`)}
                  />
                  <TextInput
                    label={tr('developers.commands.optionDescription', { n: i + 1 })}
                    value={o.description}
                    maxLength={100}
                    onChange={(e) => setOption(i, { description: e.target.value })}
                    error={touched(`option.${i}.description`)}
                  />
                  <div className="dev-field">
                    <label className="dev-label" htmlFor={`opt-type-${i}`}>
                      {tr('developers.commands.optionType', { n: i + 1 })}
                    </label>
                    <select
                      id={`opt-type-${i}`}
                      className="dev-select"
                      value={o.type}
                      onChange={(e) => setOption(i, { type: Number(e.target.value) })}
                    >
                      {OPTION_TYPES.map((ot) => (
                        <option key={ot.value} value={ot.value}>
                          {ot.label}
                        </option>
                      ))}
                    </select>
                  </div>
                  <Checkbox
                    label={tr('developers.commands.optionRequired', { n: i + 1 })}
                    checked={o.required}
                    onChange={(e) => setOption(i, { required: e.target.checked })}
                  />
                  {touched(`option.${i}.required`) && (
                    <p className="dev-error" role="alert">
                      {touched(`option.${i}.required`)}
                    </p>
                  )}
                  {[3, 4, 10].includes(o.type) && (
                    <Checkbox
                      label={tr('developers.commands.optionAutocomplete', { n: i + 1 })}
                      checked={!!o.autocomplete}
                      onChange={(e) => setOption(i, { autocomplete: e.target.checked })}
                    />
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => set({ options: draft.options.filter((_, j) => j !== i) })}
                  >
                    {tr('developers.commands.removeOption', { n: i + 1 })}
                  </Button>
                </div>
              ))}
              <Button
                variant="secondary"
                size="sm"
                disabled={draft.options.length >= MAX_OPTIONS}
                onClick={() => set({ options: [...draft.options, emptyOption()] })}
              >
                {tr('developers.commands.addOption')}
              </Button>
            </fieldset>
          )}
        </div>
      </section>
      {req ? (
        <>
          <CodeBlock lang="json" title={tr('developers.commands.requestBody')} code={req.body} />
          <CodeBlock lang="curl" title="curl" code={req.curl} />
        </>
      ) : (
        <p className="dev-muted" role="status">
          {tr('developers.commands.fillIn')}
        </p>
      )}
    </div>
  );
}
