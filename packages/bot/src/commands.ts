import type { Rest } from './rest';
import {
  CallbackType,
  InteractionType,
  type MessageOptions,
  type RawInteraction,
  type RawOption,
  type User,
} from './types';
import { messageBody } from './rest';

// ---- builders (discord.js-like) ----

const OptionKind = {
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

export class OptionBuilder {
  protected readonly json: Record<string, unknown>;

  constructor(kind: number) {
    this.json = { type: kind };
  }

  setName(name: string): this {
    this.json.name = name;
    return this;
  }
  setDescription(description: string): this {
    this.json.description = description;
    return this;
  }
  setRequired(required = true): this {
    this.json.required = required;
    return this;
  }
  setAutocomplete(on = true): this {
    this.json.autocomplete = on;
    return this;
  }
  addChoices(...choices: { name: string; value: string | number }[]): this {
    this.json.choices = [...((this.json.choices as unknown[]) ?? []), ...choices];
    return this;
  }
  /** Options of a sub-command. */
  protected addOption(o: OptionBuilder): this {
    this.json.options = [...((this.json.options as unknown[]) ?? []), o.toJSON()];
    return this;
  }
  toJSON(): Record<string, unknown> {
    return { ...this.json };
  }
}

type Configure<T> = (o: T) => T | void;

/** Shared `add*Option` methods for commands and sub-commands. */
export class OptionHolder extends OptionBuilder {
  private opt(kind: number, f: Configure<OptionBuilder>): this {
    const o = new OptionBuilder(kind);
    f(o);
    return this.addOption(o);
  }
  addStringOption(f: Configure<OptionBuilder>): this {
    return this.opt(OptionKind.String, f);
  }
  addIntegerOption(f: Configure<OptionBuilder>): this {
    return this.opt(OptionKind.Integer, f);
  }
  addNumberOption(f: Configure<OptionBuilder>): this {
    return this.opt(OptionKind.Number, f);
  }
  addBooleanOption(f: Configure<OptionBuilder>): this {
    return this.opt(OptionKind.Boolean, f);
  }
  addUserOption(f: Configure<OptionBuilder>): this {
    return this.opt(OptionKind.User, f);
  }
  addChannelOption(f: Configure<OptionBuilder>): this {
    return this.opt(OptionKind.Channel, f);
  }
  addRoleOption(f: Configure<OptionBuilder>): this {
    return this.opt(OptionKind.Role, f);
  }
  addMentionableOption(f: Configure<OptionBuilder>): this {
    return this.opt(OptionKind.Mentionable, f);
  }
  /** A file the user attaches (option type 11). */
  addAttachmentOption(f: Configure<OptionBuilder>): this {
    return this.opt(OptionKind.Attachment, f);
  }
  addSubcommand(f: Configure<OptionHolder>): this {
    const o = new OptionHolder(OptionKind.SubCommand);
    f(o);
    return this.addOption(o);
  }
}

export class SlashCommandBuilder extends OptionHolder {
  constructor() {
    super(1); // command type 1: chat input
  }
  setDefaultMemberPermissions(bits: string | null): this {
    this.json.default_member_permissions = bits;
    return this;
  }
}

/** A user (2) or message (3) context-menu command. */
export class ContextMenuCommandBuilder extends OptionBuilder {
  constructor(kind: 'user' | 'message') {
    super(kind === 'user' ? 2 : 3);
  }
}

// ---- interactions ----

export class Options {
  readonly subcommandGroup: string | undefined;
  readonly subcommand: string | undefined;
  private readonly values = new Map<string, string | number | boolean>();
  readonly focused: { name: string; value: string } | undefined;

  constructor(raw: RawOption[] | undefined) {
    let list = raw;
    const path: string[] = [];
    let focused: Options['focused'];
    while (list) {
      let next: RawOption[] | undefined;
      for (const o of list) {
        if (o.type === OptionKind.SubCommand || o.type === OptionKind.SubCommandGroup) {
          path.push(o.name);
          next = o.options;
        } else if (o.value !== undefined) {
          this.values.set(o.name, o.value);
          if (o.focused) focused = { name: o.name, value: String(o.value) };
        }
      }
      list = next;
    }
    this.focused = focused;
    this.subcommand = path.at(-1);
    this.subcommandGroup = path.length > 1 ? path[0] : undefined;
  }

  getString(name: string): string | null {
    const v = this.values.get(name);
    return typeof v === 'string' ? v : null;
  }
  getInteger(name: string): number | null {
    const v = this.values.get(name);
    return typeof v === 'number' && Number.isInteger(v) ? v : null;
  }
  getNumber(name: string): number | null {
    const v = this.values.get(name);
    return typeof v === 'number' ? v : null;
  }
  getBoolean(name: string): boolean | null {
    const v = this.values.get(name);
    return typeof v === 'boolean' ? v : null;
  }
  /** User, role, channel, mentionable and attachment options carry a snowflake. */
  getId(name: string): string | null {
    return this.getString(name);
  }
}

/** Reply helpers shared by every interaction kind. */
export class Interaction {
  constructor(
    readonly raw: RawInteraction,
    protected readonly rest: Rest,
  ) {}

  get id(): string {
    return this.raw.id;
  }
  get channelId(): string | undefined {
    return this.raw.channel_id;
  }
  get guildId(): string | undefined {
    return this.raw.guild_id;
  }
  get user(): User | undefined {
    return this.raw.member?.user;
  }

  reply(message: MessageOptions | string): Promise<void> {
    return this.rest.interactionCallback(
      this.raw,
      CallbackType.ChannelMessage,
      messageBody(message),
    );
  }
  /** Acknowledge now; answer later with `editReply` (the 3 s rule). */
  deferReply(opts: { ephemeral?: boolean } = {}): Promise<void> {
    return this.rest.interactionCallback(
      this.raw,
      CallbackType.DeferredChannelMessage,
      opts.ephemeral ? { flags: 64 } : undefined,
    );
  }
  async editReply(message: MessageOptions | string): Promise<void> {
    await this.rest.interactionEditOriginal(this.raw, message);
  }
  async followUp(message: MessageOptions | string): Promise<void> {
    await this.rest.interactionFollowUp(this.raw, message);
  }
  deleteReply(): Promise<void> {
    return this.rest.interactionDeleteOriginal(this.raw);
  }
}

export class CommandInteraction extends Interaction {
  readonly options: Options;
  constructor(raw: RawInteraction, rest: Rest) {
    super(raw, rest);
    this.options = new Options(raw.data?.options);
  }
  get commandName(): string {
    return this.raw.data?.name ?? '';
  }
}

export class AutocompleteInteraction extends CommandInteraction {
  /** At most 25 choices are sent. */
  respond(choices: { name: string; value: string | number }[]): Promise<void> {
    return this.rest.interactionCallback(this.raw, CallbackType.Autocomplete, {
      choices: choices.slice(0, 25),
    });
  }
}

export class ComponentInteraction extends Interaction {
  get customId(): string {
    return this.raw.data?.custom_id ?? '';
  }
  /** Edit the message the component is on. */
  update(message: MessageOptions | string): Promise<void> {
    return this.rest.interactionCallback(
      this.raw,
      CallbackType.UpdateMessage,
      messageBody(message),
    );
  }
}

export type AnyInteraction =
  CommandInteraction | AutocompleteInteraction | ComponentInteraction | Interaction;

export function wrapInteraction(raw: RawInteraction, rest: Rest): AnyInteraction {
  switch (raw.type) {
    case InteractionType.ApplicationCommand:
      return new CommandInteraction(raw, rest);
    case InteractionType.Autocomplete:
      return new AutocompleteInteraction(raw, rest);
    case InteractionType.MessageComponent:
      return new ComponentInteraction(raw, rest);
    default:
      return new Interaction(raw, rest);
  }
}

// ---- router ----

type CommandHandler = (i: CommandInteraction) => Promise<void> | void;
type AutocompleteHandler = (i: AutocompleteInteraction) => Promise<void> | void;

export class CommandRouter {
  private readonly defs: unknown[] = [];
  private readonly handlers = new Map<string, CommandHandler>();
  private readonly autocompletes = new Map<string, AutocompleteHandler>();

  /** Register a definition and its handler. */
  add(
    def: { toJSON(): Record<string, unknown> },
    handler: CommandHandler,
    autocomplete?: AutocompleteHandler,
  ): this {
    const json = def.toJSON();
    const name = String(json.name);
    this.defs.push(json);
    this.handlers.set(name, handler);
    if (autocomplete) this.autocompletes.set(name, autocomplete);
    return this;
  }

  /** Body for `PUT /applications/{id}/commands`. */
  definitions(): unknown[] {
    return [...this.defs];
  }

  get size(): number {
    return this.defs.length;
  }

  /** Route an interaction; `false` when no registered command matches. */
  async dispatch(i: AnyInteraction): Promise<boolean> {
    if (i instanceof AutocompleteInteraction) {
      const h = this.autocompletes.get(i.commandName);
      if (!h) return false;
      await h(i);
      return true;
    }
    if (i instanceof CommandInteraction) {
      const h = this.handlers.get(i.commandName);
      if (!h) return false;
      await h(i);
      return true;
    }
    return false;
  }
}
