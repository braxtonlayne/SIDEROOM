/**
 * Application commands in the composer (DEV-BOT-008): the types of the command index, ranking for the
 * picker, parsing of typed `name:value` arguments, and building the `data.options` of an interaction.
 * Pure, so it is unit-tested without a DOM. Server rules: docs/specs/developer-platform.md sections 3.5-3.6.
 */

/** Discord option types. */
export const OptionType = {
  SubCommand: 1,
  SubCommandGroup: 2,
  String: 3,
  Integer: 4,
  Boolean: 5,
  User: 6,
  Channel: 7,
  Role: 8,
  Mentionable: 9,
  Number: 10,
  Attachment: 11,
} as const;

export interface CommandChoice {
  name: string;
  value: string | number;
}

export interface CommandOption {
  type: number;
  name: string;
  description: string;
  required?: boolean;
  choices?: CommandChoice[];
  autocomplete?: boolean;
  options?: CommandOption[];
  min_value?: number;
  max_value?: number;
  min_length?: number;
  max_length?: number;
  channel_types?: number[];
}

/** 1 chat input, 2 user, 3 message. */
export interface AppCommand {
  id: string;
  application_id: string;
  type: 1 | 2 | 3;
  name: string;
  description: string;
  options?: CommandOption[];
  guild_id?: string;
  nsfw?: boolean;
}

export interface IndexApp {
  id: string;
  name: string;
  icon: string | null;
  bot_id: string;
}

/** `GET /channels/{id}/application-commands`. */
export interface CommandIndex {
  applications: IndexApp[];
  application_commands: AppCommand[];
}

export interface IndexedCommand {
  command: AppCommand;
  app: IndexApp;
}

/** Commands of one kind joined with their application, in the index order. */
export function indexedCommands(
  index: CommandIndex | undefined,
  kind: 1 | 2 | 3,
): IndexedCommand[] {
  if (!index) return [];
  const apps = new Map(index.applications.map((a) => [a.id, a]));
  const out: IndexedCommand[] = [];
  for (const command of index.application_commands) {
    const app = apps.get(command.application_id);
    if (app && command.type === kind) out.push({ command, app });
  }
  return out;
}

/** Exact, then prefix, then substring matches; ties keep the index order. */
export function searchCommands(list: IndexedCommand[], query: string): IndexedCommand[] {
  const q = query.toLowerCase();
  const scored = list
    .map((c, i) => {
      const n = c.command.name.toLowerCase();
      const score = n === q ? 0 : n.startsWith(q) ? 1 : n.includes(q) ? 2 : 3;
      return { c, i, score };
    })
    .filter((x) => x.score < 3);
  scored.sort((a, b) => a.score - b.score || a.i - b.i);
  return scored.map((x) => x.c);
}

/** The command a message such as `/echo text:hi` names, and the text after the name. */
export function matchTyped(
  text: string,
  list: IndexedCommand[],
): { entry: IndexedCommand; rest: string } | null {
  const m = /^\/(\S+)(?:\s+([\s\S]*))?$/.exec(text.trim());
  if (!m) return null;
  const name = m[1]!.toLowerCase();
  const entry = list.find((c) => c.command.name.toLowerCase() === name);
  return entry ? { entry, rest: (m[2] ?? '').trim() } : null;
}

/** A path into the option tree: sub-command group, sub-command (each optional). */
export type SubPath = string[];

const isSub = (o: CommandOption) =>
  o.type === OptionType.SubCommand || o.type === OptionType.SubCommandGroup;

function descend(options: CommandOption[] | undefined, path: SubPath): CommandOption[] {
  let current = options ?? [];
  for (const name of path) {
    current = current.find((o) => o.name === name && isSub(o))?.options ?? [];
  }
  return current;
}

/** The options that take values for the chosen sub-command path (or the command's own). */
export function leafOptions(options: CommandOption[] | undefined, path: SubPath): CommandOption[] {
  return descend(options, path).filter((o) => !isSub(o));
}

/** The sub-commands (or groups) offered at this level of `path`. */
export function subChoices(options: CommandOption[] | undefined, path: SubPath): CommandOption[] {
  return descend(options, path).filter(isSub);
}

/** Whether `path` ends at a runnable level (no deeper sub-command to pick). */
export function pathComplete(options: CommandOption[] | undefined, path: SubPath): boolean {
  return subChoices(options, path).length === 0;
}

export interface TypedArgs {
  path: SubPath;
  values: Record<string, string>;
}

const KEYED = /(?:^|\s)([a-z0-9_-]+):/i;

/**
 * Reads `/cmd [group] [sub] name:value other:"two words"` into a sub-command path and option values.
 * Without any `name:` token, the whole remainder goes to the first string option, which makes
 * `/echo hello world` work. Unknown keys are ignored.
 */
export function parseTypedArgs(command: AppCommand, rest: string): TypedArgs {
  const path: SubPath = [];
  let text = rest.trim();
  for (;;) {
    const subs = subChoices(command.options, path);
    if (subs.length === 0) break;
    const m = /^(\S+)\s*([\s\S]*)$/.exec(text);
    const hit = m && subs.find((s) => s.name.toLowerCase() === m[1]!.toLowerCase());
    if (!m || !hit) break;
    path.push(hit.name);
    text = (m[2] ?? '').trim();
  }
  const opts = leafOptions(command.options, path);
  const values: Record<string, string> = {};
  const firstKey = KEYED.exec(text);
  if (!firstKey) {
    const first = opts.find((o) => o.type === OptionType.String);
    if (first && text) values[first.name] = text;
    return { path, values };
  }
  const re = /(?:^|\s)([a-z0-9_-]+):("([^"]*)"|[^\s"]*)/gi;
  for (const m of text.matchAll(re)) {
    const opt = opts.find((o) => o.name.toLowerCase() === m[1]!.toLowerCase());
    if (opt) values[opt.name] = m[3] ?? m[2] ?? '';
  }
  // Free text before the first key goes to the first string option that has no value yet.
  const lead = text.slice(0, firstKey.index).trim();
  if (lead) {
    const first = opts.find((o) => o.type === OptionType.String && !(o.name in values));
    if (first) values[first.name] = lead;
  }
  return { path, values };
}

export type OptionError =
  'required' | 'number' | 'integer' | 'min' | 'max' | 'minLength' | 'maxLength' | 'choice';

/** First problem with one option's raw text value, or null. Empty optional values are fine. */
export function optionError(opt: CommandOption, raw: string | undefined): OptionError | null {
  const v = (raw ?? '').trim();
  if (!v) return opt.required ? 'required' : null;
  switch (opt.type) {
    case OptionType.Integer:
    case OptionType.Number: {
      const n = Number(v);
      if (!Number.isFinite(n)) return 'number';
      if (opt.type === OptionType.Integer && !Number.isInteger(n)) return 'integer';
      if (opt.min_value !== undefined && n < opt.min_value) return 'min';
      if (opt.max_value !== undefined && n > opt.max_value) return 'max';
      return null;
    }
    case OptionType.String: {
      const len = [...(raw ?? '')].length;
      if (opt.min_length !== undefined && len < opt.min_length) return 'minLength';
      if (opt.max_length !== undefined && len > opt.max_length) return 'maxLength';
      if (opt.choices?.length && !opt.choices.some((c) => String(c.value) === raw)) return 'choice';
      return null;
    }
    default:
      return null;
  }
}

/** Option names with a problem for the current values (attachments are not supported yet). */
export function formErrors(
  command: AppCommand,
  path: SubPath,
  values: Record<string, string>,
): Record<string, OptionError> {
  const out: Record<string, OptionError> = {};
  for (const opt of leafOptions(command.options, path)) {
    if (opt.type === OptionType.Attachment) {
      if (opt.required) out[opt.name] = 'required';
      continue;
    }
    const e = optionError(opt, values[opt.name]);
    if (e) out[opt.name] = e;
  }
  return out;
}

export interface WireOption {
  type: number;
  name: string;
  value?: string | number | boolean;
  options?: WireOption[];
}

/** `data.options` for the chosen sub-command path and values (empty values are left out). */
export function buildOptions(
  command: AppCommand,
  path: SubPath,
  values: Record<string, string>,
): WireOption[] {
  const leaves: WireOption[] = [];
  for (const opt of leafOptions(command.options, path)) {
    const raw = values[opt.name];
    if (raw === undefined || raw.trim() === '') continue;
    let value: string | number | boolean;
    switch (opt.type) {
      case OptionType.Integer:
      case OptionType.Number:
        value = Number(raw);
        break;
      case OptionType.Boolean:
        value = raw === 'true';
        break;
      case OptionType.Attachment:
        continue;
      default:
        value = raw;
    }
    leaves.push({ type: opt.type, name: opt.name, value });
  }
  // Wrap from the inside out: [group, sub] becomes group { sub { leaves } }.
  let wrapped = leaves;
  for (let i = path.length - 1; i >= 0; i--) {
    const level = subChoices(command.options, path.slice(0, i)).find((o) => o.name === path[i]);
    wrapped = [{ type: level?.type ?? OptionType.SubCommand, name: path[i]!, options: wrapped }];
  }
  return wrapped;
}

/** Whether the command can run without a form: it declares no options at all. */
export function runsImmediately(command: AppCommand): boolean {
  return !command.options?.length;
}
