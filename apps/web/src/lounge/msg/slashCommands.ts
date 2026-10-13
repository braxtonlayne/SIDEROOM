/** Built-in slash commands (LNG-CMP-015). Text commands expand locally; `poll` opens the poll dialog. */

export interface SlashCommand {
  name: string;
  description: string;
  /** Takes an argument (shown as a hint, kept in the box when picked). */
  arg?: string;
}

export const SLASH_COMMANDS: readonly SlashCommand[] = [
  { name: 'shrug', description: 'Appends ¯\\_(ツ)_/¯ to your message', arg: 'message' },
  { name: 'tableflip', description: 'Appends (╯°□°）╯︵ ┻━┻ to your message', arg: 'message' },
  { name: 'unflip', description: 'Appends ┬─┬ ノ( ゜-゜ノ) to your message', arg: 'message' },
  { name: 'me', description: 'Displays text with emphasis', arg: 'message' },
  { name: 'spoiler', description: 'Marks your message as a spoiler', arg: 'message' },
  { name: 'poll', description: 'Create a poll' },
];

const SHRUG = '¯\\_(ツ)_/¯';
const TABLEFLIP = '(╯°□°）╯︵ ┻━┻';
const UNFLIP = '┬─┬ ノ( ゜-゜ノ)';

export type SlashResult =
  | { kind: 'send'; content: string }
  | { kind: 'poll' }
  | { kind: 'none' }
  | { kind: 'error'; message: string };

/** Evaluates a message that starts with `/`. Unknown commands are sent as plain text. */
export function runSlash(text: string): SlashResult {
  const m = /^\/([a-z]+)(?:\s+([\s\S]*))?$/.exec(text.trim());
  if (!m) return { kind: 'none' };
  const name = m[1]!;
  const arg = (m[2] ?? '').trim();
  const withSuffix = (suffix: string) => ({
    kind: 'send' as const,
    content: arg ? `${arg} ${suffix}` : suffix,
  });
  switch (name) {
    case 'shrug':
      return withSuffix(SHRUG);
    case 'tableflip':
      return withSuffix(TABLEFLIP);
    case 'unflip':
      return withSuffix(UNFLIP);
    case 'me':
      return arg
        ? { kind: 'send', content: `_${arg.replace(/_/g, '\\_')}_` }
        : { kind: 'error', message: '/me needs a message.' };
    case 'spoiler':
      return arg
        ? { kind: 'send', content: `||${arg}||` }
        : { kind: 'error', message: '/spoiler needs a message.' };
    case 'poll':
      return { kind: 'poll' };
    default:
      return { kind: 'none' };
  }
}

export function commandSuggestions(query: string) {
  const q = query.toLowerCase();
  return SLASH_COMMANDS.filter((c) => c.name.startsWith(q)).map((c) => ({
    key: `/${c.name}`,
    label: `/${c.name}`,
    hint: c.description,
    insert: `/${c.name}${c.arg ? ' ' : ''}`,
  }));
}
