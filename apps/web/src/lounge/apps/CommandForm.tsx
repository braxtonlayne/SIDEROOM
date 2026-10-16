import { Button, Checkbox, Modal, TextInput } from '@sideroom/ui';
import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { useT, type MessageKey } from '../../i18n';
import { useChannels, useMembers, useRoles, type Snowflake } from '../data';
import {
  buildOptions,
  formErrors,
  leafOptions,
  OptionType,
  pathComplete,
  subChoices,
  type CommandOption,
  type IndexedCommand,
  type OptionError,
  type SubPath,
  type TypedArgs,
} from './commands';
import {
  newNonce,
  requestAutocomplete,
  sendInteraction,
  type AutocompleteChoice,
  type Outcome,
} from './interactions';
import { uploadFiles, type PendingFile } from '../msg/upload';
import { useReportOutcome } from './useReportOutcome';
import './apps.css';

const ERROR_KEY = {
  required: 'lounge.apps.form.errRequired',
  number: 'lounge.apps.form.errNumber',
  integer: 'lounge.apps.form.errInteger',
  min: 'lounge.apps.form.errMin',
  max: 'lounge.apps.form.errMax',
  minLength: 'lounge.apps.form.errMinLength',
  maxLength: 'lounge.apps.form.errMaxLength',
  choice: 'lounge.apps.form.errChoice',
} as const satisfies Record<OptionError, MessageKey>;

/** Debounce for autocomplete requests while typing. */
export const AUTOCOMPLETE_DELAY_MS = 250;

export interface CommandFormProps {
  entry: IndexedCommand;
  channelId: Snowflake;
  guildId: Snowflake;
  prefill?: TypedArgs;
  onClose: () => void;
  /** Called with the outcome once the app answered (or failed). */
  onDone?: (o: Outcome) => void;
}

/**
 * The option form of an app command (DEV-BOT-008): sub-command pickers, typed inputs, choice lists,
 * autocomplete (answered by the app over the gateway), and server pickers for user, role and channel options.
 */
export function CommandForm({
  entry,
  channelId,
  guildId,
  prefill,
  onClose,
  onDone,
}: CommandFormProps) {
  const t = useT();
  const report = useReportOutcome();
  const { command, app } = entry;
  const [path, setPath] = useState<SubPath>(prefill?.path ?? []);
  const [values, setValues] = useState<Record<string, string>>(prefill?.values ?? {});
  const [busy, setBusy] = useState(false);
  const [touched, setTouched] = useState(false);
  const [files, setFiles] = useState<Record<string, File>>({});
  const [uploadError, setUploadError] = useState<string | null>(null);
  const errors = formErrors(command, path, values);
  const complete = pathComplete(command.options, path);
  const opts = complete ? leafOptions(command.options, path) : [];
  const blocked = Object.keys(errors).length > 0 || !complete;

  const run = () => {
    setTouched(true);
    if (blocked || busy) return;
    setBusy(true);
    setUploadError(null);
    void (async () => {
      // File options: upload each chosen file to this channel, then name the upload slot (DEV-BOT-008).
      const sent = { ...values };
      const chosen = opts.filter((o) => o.type === OptionType.Attachment && files[o.name]);
      if (chosen.length > 0) {
        const pending: PendingFile[] = chosen.map((o) => {
          const file = files[o.name]!;
          return {
            id: o.name,
            file,
            name: file.name,
            spoiler: false,
            description: '',
            progress: 0,
            status: 'queued',
          };
        });
        try {
          const done = await uploadFiles(channelId, pending, {
            onUpdate: () => undefined,
            signal: new AbortController().signal,
          });
          chosen.forEach((o, i) => {
            // `uploadedFilename` is `{upload id}/{filename}`; the server wants the upload id.
            sent[o.name] = (done[i]?.uploadedFilename ?? '').split('/')[0] ?? '';
          });
        } catch (e) {
          setBusy(false);
          setUploadError(e instanceof Error ? e.message : '');
          return;
        }
      }
      const o = await sendInteraction({
        type: 2,
        application_id: command.application_id,
        channel_id: channelId,
        guild_id: guildId,
        nonce: newNonce(),
        data: { id: command.id, options: buildOptions(command, path, sent) },
      });
      setBusy(false);
      report(o);
      onDone?.(o);
      // A modal opens from the app's answer; the form is done either way.
      if (o.ok || o.reason !== 'rejected') onClose();
    })();
  };

  const choose = (level: number, name: string) => {
    setPath((p) => (name ? [...p.slice(0, level), name] : p.slice(0, level)));
    setValues({});
    setFiles({});
  };

  // One select per sub-command level that still needs a choice.
  const levels: { level: number; options: CommandOption[]; value: string }[] = [];
  for (let i = 0; i <= path.length; i++) {
    const options = subChoices(command.options, path.slice(0, i));
    if (options.length === 0) break;
    levels.push({ level: i, options, value: path[i] ?? '' });
  }

  return (
    <Modal
      open
      onClose={onClose}
      size="md"
      title={t('lounge.apps.form.title', { name: command.name })}
      description={t('lounge.apps.form.description', {
        app: app.name,
        description: command.description,
      })}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t('lounge.apps.form.cancel')}
          </Button>
          <Button onClick={run} loading={busy} disabled={busy}>
            {t('lounge.apps.form.run')}
          </Button>
        </>
      }
    >
      <form
        className="lng-appform"
        onSubmit={(e) => {
          e.preventDefault();
          run();
        }}
      >
        {levels.map((l) => (
          <label key={l.level} className="lng-appform-field">
            <span className="lng-appform-label">{t('lounge.apps.form.subcommand')}</span>
            <select
              className="lng-appform-select"
              value={l.value}
              onChange={(e) => choose(l.level, e.target.value)}
            >
              <option value="">{t('lounge.apps.form.pick')}</option>
              {l.options.map((o) => (
                <option key={o.name} value={o.name}>
                  {o.name}
                </option>
              ))}
            </select>
          </label>
        ))}
        {opts.map((opt) => (
          <OptionField
            key={`${path.join('.')}:${opt.name}`}
            opt={opt}
            value={values[opt.name] ?? ''}
            error={touched ? errors[opt.name] : undefined}
            guildId={guildId}
            autocomplete={(text) =>
              requestAutocomplete(
                {
                  application_id: command.application_id,
                  channel_id: channelId,
                  guild_id: guildId,
                },
                command.id,
                autocompleteOptions(command, path, values, opt, text),
              )
            }
            onChange={(v) => setValues((cur) => ({ ...cur, [opt.name]: v }))}
            onFile={(f) => {
              setFiles((cur) => {
                const next = { ...cur };
                if (f) next[opt.name] = f;
                else delete next[opt.name];
                return next;
              });
              setValues((cur) => ({ ...cur, [opt.name]: f ? f.name : '' }));
            }}
          />
        ))}
        {uploadError !== null && (
          <p role="alert" className="lng-appform-error">
            {uploadError || t('lounge.apps.form.uploadFailed')}
          </p>
        )}
        {touched && !complete && (
          <p role="alert" className="lng-appform-error">
            {t('lounge.apps.form.pickSubcommand')}
          </p>
        )}
        <button type="submit" hidden aria-hidden="true" tabIndex={-1} />
      </form>
    </Modal>
  );
}

/** `data.options` of an autocomplete request: the values so far plus the focused option. */
function autocompleteOptions(
  command: IndexedCommand['command'],
  path: SubPath,
  values: Record<string, string>,
  focused: CommandOption,
  text: string,
) {
  const all = { ...values, [focused.name]: text };
  const built = buildOptions(command, path, all);
  const mark = (list: ReturnType<typeof buildOptions>): ReturnType<typeof buildOptions> =>
    list.map((o) =>
      o.options
        ? { ...o, options: mark(o.options) }
        : o.name === focused.name
          ? ({ ...o, focused: true } as typeof o)
          : o,
    );
  return mark(built);
}

interface FieldProps {
  opt: CommandOption;
  value: string;
  error: OptionError | undefined;
  guildId: Snowflake;
  autocomplete: (text: string) => Promise<AutocompleteChoice[]>;
  onChange: (v: string) => void;
  /** The file of an attachment option (null when cleared). */
  onFile: (f: File | null) => void;
}

function OptionField({ opt, value, error, guildId, autocomplete, onChange, onFile }: FieldProps) {
  const t = useT();
  const members = useMembers(guildId);
  const roles = useRoles(guildId);
  const channels = useChannels(guildId);
  const listId = useId();
  const [choices, setChoices] = useState<AutocompleteChoice[]>([]);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  const label = `${opt.name}${opt.required ? ' *' : ''}`;
  const hint = opt.description;
  const errText = error
    ? t(ERROR_KEY[error], {
        min: opt.min_value ?? opt.min_length ?? 0,
        max: opt.max_value ?? opt.max_length ?? 0,
      })
    : undefined;

  const selectOptions = useMemo(() => {
    switch (opt.type) {
      case OptionType.User:
        return members.map((m) => ({ value: m.user.id, label: m.nick || m.user.displayName }));
      case OptionType.Role:
        return roles.filter((r) => r.id !== guildId).map((r) => ({ value: r.id, label: r.name }));
      case OptionType.Mentionable:
        return [
          ...members.map((m) => ({
            value: m.user.id,
            label: `@${m.nick || m.user.displayName}`,
          })),
          ...roles
            .filter((r) => r.id !== guildId)
            .map((r) => ({ value: r.id, label: `@${r.name}` })),
        ];
      case OptionType.Channel:
        return channels
          .filter((c) => c.type !== 'category')
          .map((c) => ({ value: c.id, label: `#${c.name}` }));
      default:
        return (opt.choices ?? []).map((c) => ({ value: String(c.value), label: c.name }));
    }
  }, [opt, members, roles, channels, guildId]);

  if (opt.type === OptionType.Attachment) {
    return (
      <label className="lng-appform-field">
        <span className="lng-appform-label">{label}</span>
        <input
          type="file"
          className="lng-appform-file"
          aria-invalid={errText ? true : undefined}
          onChange={(e) => onFile(e.target.files?.[0] ?? null)}
        />
        {value && (
          <span className="lng-appform-hint">
            {t('lounge.apps.form.fileChosen', { name: value })}
          </span>
        )}
        <span className="lng-appform-hint">{hint}</span>
        {errText && (
          <span role="alert" className="lng-appform-error">
            {errText}
          </span>
        )}
      </label>
    );
  }
  if (opt.type === OptionType.Boolean) {
    return (
      <div className="lng-appform-field">
        <Checkbox
          label={label}
          description={hint}
          checked={value === 'true'}
          onChange={(e) => onChange(e.target.checked ? 'true' : 'false')}
        />
      </div>
    );
  }
  const picker =
    opt.type === OptionType.User ||
    opt.type === OptionType.Role ||
    opt.type === OptionType.Mentionable ||
    opt.type === OptionType.Channel ||
    (opt.choices?.length ?? 0) > 0;
  if (picker) {
    return (
      <label className="lng-appform-field">
        <span className="lng-appform-label">{label}</span>
        <select
          className="lng-appform-select"
          aria-invalid={errText ? true : undefined}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">{t('lounge.apps.form.pick')}</option>
          {selectOptions.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
        <span className="lng-appform-hint">{hint}</span>
        {errText && (
          <span role="alert" className="lng-appform-error">
            {errText}
          </span>
        )}
      </label>
    );
  }
  const numeric = opt.type === OptionType.Integer || opt.type === OptionType.Number;
  return (
    <div className="lng-appform-field">
      <TextInput
        label={label}
        hint={hint}
        error={errText}
        value={value}
        list={opt.autocomplete ? listId : undefined}
        inputMode={numeric ? 'decimal' : undefined}
        autoComplete="off"
        onChange={(e) => {
          const v = e.target.value;
          onChange(v);
          if (!opt.autocomplete) return;
          clearTimeout(timer.current);
          timer.current = setTimeout(() => {
            void autocomplete(v).then(setChoices);
          }, AUTOCOMPLETE_DELAY_MS);
        }}
      />
      {opt.autocomplete && (
        <datalist id={listId}>
          {choices.map((c) => (
            <option key={String(c.value)} value={String(c.value)}>
              {c.name}
            </option>
          ))}
        </datalist>
      )}
    </div>
  );
}
