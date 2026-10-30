/**
 * UTL-SHL-001: the tool registry. One entry per tool of the utility drawer; the drawer, the gadget layer and the
 * pin rail read nothing else about a tool. utl-shell owns this file; the other wave-32/33 agents APPEND their
 * entry to `TOOLS` (one object each, at the end, never reordering another's) and own the component the entry
 * loads. The registry holds no component code: `load` is a dynamic import, so a tool costs nothing until the
 * member opens it, and nothing here is imported by a public surface (D-385).
 *
 * To add a tool:
 *   1. Write the component in `apps/web/src/tools/<yourdir>/`, default export, taking `ToolProps`.
 *   2. Put its strings in your own catalog fragment (`i18n/messages/en/p32-<name>-<n>.ts`), keys
 *      `tools.<id>.name` and `tools.<id>.description` at least (no underscores in key segments).
 *   3. Append an entry below. `aliases` are English search words (stage 1 of the search, wave 32); the task
 *      phrases for stage 3 live in `corpus.json`.
 */
import type { ComponentType } from 'react';
import type { IconName } from '@sideroom/ui';
import type { MessageKey } from '../i18n/messages/en';

/** Where the tool is shown right now. A tool may adapt its layout, never its behaviour. */
export type ToolHost = 'drawer' | 'gadget' | 'rail';

export interface ToolProps {
  host: ToolHost;
  /** Closes the host (the drawer view, the gadget or the rail panel); absent when there is nothing to close. */
  onClose?: () => void;
}

/** A tool's default and minimum window, in CSS pixels, when it is popped out. */
export interface ToolSize {
  width: number;
  height: number;
  minWidth: number;
  minHeight: number;
}

/** Search and list grouping; also the order of the groups in the drawer. */
export type ToolGroup = 'text' | 'dev' | 'math' | 'time' | 'media' | 'collab' | 'fun' | 'other';
export const TOOL_GROUPS: readonly ToolGroup[] = [
  'math',
  'text',
  'dev',
  'time',
  'media',
  'collab',
  'fun',
  'other',
];

export interface ToolEntry {
  /** Lowercase letters, digits and hyphens; stable forever (it is stored in the account's layout). */
  id: string;
  /** i18n keys: `tools.<id>.name`, `tools.<id>.description`. */
  nameKey: MessageKey;
  descriptionKey: MessageKey;
  icon: IconName;
  group: ToolGroup;
  /** English search words besides the name ("calc" for the calculator). */
  aliases: readonly string[];
  size: ToolSize;
  /** False for a tool that only makes sense in the drawer (it then shows no "pop out" and no "pin"). */
  floatable?: boolean;
  load: () => Promise<{ default: ComponentType<ToolProps> }>;
}

export const DEFAULT_TOOL_SIZE: ToolSize = {
  width: 360,
  height: 420,
  minWidth: 240,
  minHeight: 160,
};

/** The drawer's own guide: how the drawer works. Keeps the shell usable and testable before any other tool lands. */
const guide: ToolEntry = {
  id: 'guide',
  nameKey: 'tools.guide.name',
  descriptionKey: 'tools.guide.description',
  icon: 'info',
  group: 'other',
  aliases: ['help', 'drawer', 'shortcuts', 'how'],
  size: { width: 340, height: 300, minWidth: 240, minHeight: 160 },
  load: () => import('./drawer/GuideTool'),
};

/** The catalog segment of a tool id: catalog keys have no hyphens (`md-table` is `mdTable`). */
export const catalogId = (id: string): string =>
  id.replace(/-([a-z0-9])/g, (_, c: string) => c.toUpperCase());

// ---- UTL-DEV-001 (utl-dev): developer tools, dice, notes, timers. Sizes are the popped-out window. ----
const devSize: ToolSize = { width: 420, height: 480, minWidth: 280, minHeight: 240 };
const small: ToolSize = { width: 340, height: 380, minWidth: 240, minHeight: 200 };
const devTool = (
  id: string,
  icon: IconName,
  group: ToolGroup,
  aliases: readonly string[],
  size: ToolSize,
  load: ToolEntry['load'],
): ToolEntry => ({
  id,
  nameKey: `tools.${catalogId(id)}.name`,
  descriptionKey: `tools.${catalogId(id)}.description`,
  icon,
  group,
  aliases,
  size,
  load,
});

const utlDev: ToolEntry[] = [
  devTool(
    'json',
    'list',
    'dev',
    ['json', 'format', 'validate', 'minify', 'prettify'],
    devSize,
    () => import('./dev/JsonTool'),
  ),
  devTool(
    'regex',
    'search',
    'dev',
    ['regex', 'regexp', 'pattern', 'match'],
    devSize,
    () => import('./dev/RegexTool'),
  ),
  devTool(
    'base64',
    'lock',
    'dev',
    ['base64', 'b64', 'encode', 'decode'],
    devSize,
    () => import('./dev/Base64Tool'),
  ),
  devTool(
    'url',
    'link',
    'dev',
    ['url', 'uri', 'percent', 'query', 'encode'],
    devSize,
    () => import('./dev/UrlTool'),
  ),
  devTool(
    'jwt',
    'shield',
    'dev',
    ['jwt', 'token', 'bearer', 'jws'],
    devSize,
    () => import('./dev/JwtTool'),
  ),
  devTool(
    'md-table',
    'grid',
    'text',
    ['markdown', 'table', 'csv', 'tsv'],
    devSize,
    () => import('./dev/MdTableTool'),
  ),
  devTool(
    'counter',
    'edit',
    'text',
    ['count', 'words', 'characters', 'length'],
    small,
    () => import('./dev/CounterTool'),
  ),
  devTool(
    'hash',
    'hash',
    'dev',
    ['hash', 'md5', 'sha', 'sha256', 'crc32', 'checksum'],
    devSize,
    () => import('./dev/HashTool'),
  ),
  devTool(
    'diff',
    'filter',
    'dev',
    ['diff', 'compare', 'difference', 'changes'],
    { ...devSize, width: 520 },
    () => import('./dev/DiffTool'),
  ),
  devTool(
    'dice',
    'sparkle',
    'fun',
    ['dice', 'roll', 'd20', 'rpg', 'random'],
    small,
    () => import('./dice/DiceTool'),
  ),
  devTool(
    'coin',
    'star',
    'fun',
    ['coin', 'flip', 'heads', 'tails'],
    small,
    () => import('./dice/CoinTool'),
  ),
  devTool(
    'wheel',
    'compass',
    'fun',
    ['wheel', 'spin', 'picker', 'raffle', 'choose'],
    small,
    () => import('./dice/WheelTool'),
  ),
  devTool(
    'loot',
    'heart',
    'fun',
    ['loot', 'table', 'drop', 'weighted'],
    devSize,
    () => import('./dice/LootTool'),
  ),
  devTool(
    'notes',
    'bookmark',
    'collab',
    ['notes', 'scratchpad', 'sticky', 'memo', 'jot'],
    devSize,
    () => import('./notes/NotesTool'),
  ),
  devTool(
    'pomodoro',
    'clock',
    'time',
    ['pomodoro', 'focus', 'timer', 'study'],
    small,
    () => import('./timers/PomodoroTool'),
  ),
  devTool(
    'stopwatch',
    'clock',
    'time',
    ['stopwatch', 'lap', 'timer'],
    small,
    () => import('./timers/StopwatchTool'),
  ),
  devTool(
    'countdown',
    'clock',
    'time',
    ['countdown', 'timer', 'alarm'],
    small,
    () => import('./timers/CountdownTool'),
  ),
];

/** Append new entries after the last one. */
export const TOOLS: ToolEntry[] = [
  guide,
  ...utlDev,
  // UTL-COL-001 (utl-collab): the collaborative tier list and poll builder.
  {
    id: 'tierlist',
    nameKey: 'tools.tierlist.name',
    descriptionKey: 'tools.tierlist.description',
    icon: 'list',
    group: 'collab',
    aliases: ['tier', 'tiers', 'rank', 'ranking', 'tier maker'],
    size: { width: 520, height: 560, minWidth: 320, minHeight: 280 },
    load: () => import('./tierlist/TierListTool'),
  },
  {
    id: 'poll',
    nameKey: 'tools.poll.name',
    descriptionKey: 'tools.poll.description',
    icon: 'grid',
    group: 'collab',
    aliases: ['poll', 'vote', 'survey', 'polls'],
    size: { width: 420, height: 520, minWidth: 300, minHeight: 280 },
    load: () => import('./poll/PollBuilderTool'),
  },
];

const ID = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

/** Problems with the registry (duplicate or malformed ids); the unit test asserts this list is empty. */
export function registryProblems(list: readonly ToolEntry[] = TOOLS): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const e of list) {
    if (!ID.test(e.id)) out.push(`bad id: ${e.id}`);
    if (seen.has(e.id)) out.push(`duplicate id: ${e.id}`);
    seen.add(e.id);
    if (e.nameKey !== `tools.${catalogId(e.id)}.name`) out.push(`${e.id}: nameKey`);
    if (e.descriptionKey !== `tools.${catalogId(e.id)}.description`)
      out.push(`${e.id}: descriptionKey`);
    if (e.size.minWidth > e.size.width || e.size.minHeight > e.size.height)
      out.push(`${e.id}: size below its minimum`);
  }
  return out;
}

export const getTool = (id: string, list: readonly ToolEntry[] = TOOLS): ToolEntry | undefined =>
  list.find((e) => e.id === id);
