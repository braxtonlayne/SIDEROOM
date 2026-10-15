import { describe, expect, it } from 'vitest';
import {
  buildOptions,
  formErrors,
  indexedCommands,
  leafOptions,
  matchTyped,
  optionError,
  parseTypedArgs,
  pathComplete,
  runsImmediately,
  searchCommands,
  subChoices,
  type AppCommand,
  type CommandIndex,
} from '../commands';

const echo: AppCommand = {
  id: '10',
  application_id: '1',
  type: 1,
  name: 'echo',
  description: 'Echo',
  options: [
    { type: 3, name: 'text', description: 'What', required: true, max_length: 10 },
    { type: 4, name: 'times', description: 'How often', min_value: 1, max_value: 5 },
    { type: 5, name: 'loud', description: 'Shout' },
  ],
};
const admin: AppCommand = {
  id: '11',
  application_id: '1',
  type: 1,
  name: 'admin',
  description: 'Admin',
  options: [
    {
      type: 2,
      name: 'user',
      description: 'Users',
      options: [
        {
          type: 1,
          name: 'ban',
          description: 'Ban',
          options: [
            { type: 6, name: 'who', description: 'Who', required: true },
            { type: 3, name: 'reason', description: 'Why' },
          ],
        },
      ],
    },
    { type: 1, name: 'ping', description: 'Ping' },
  ],
};
const index: CommandIndex = {
  applications: [
    { id: '1', name: 'Helper', icon: null, bot_id: '99' },
    { id: '2', name: 'Other', icon: null, bot_id: '98' },
  ],
  application_commands: [
    echo,
    admin,
    { id: '12', application_id: '2', type: 2, name: 'Profile', description: '' },
    { id: '13', application_id: '3', type: 1, name: 'orphan', description: 'no app' },
  ],
};

describe('command index', () => {
  it('joins commands with their application and filters by kind', () => {
    expect(indexedCommands(index, 1).map((c) => c.command.name)).toEqual(['echo', 'admin']);
    expect(indexedCommands(index, 2).map((c) => c.app.name)).toEqual(['Other']);
    expect(indexedCommands(undefined, 1)).toEqual([]);
  });

  it('ranks prefix before substring and drops non-matches', () => {
    const list = indexedCommands(index, 1);
    expect(searchCommands(list, '').map((c) => c.command.name)).toEqual(['echo', 'admin']);
    expect(searchCommands(list, 'ad').map((c) => c.command.name)).toEqual(['admin']);
    expect(searchCommands(list, 'ch').map((c) => c.command.name)).toEqual(['echo']);
    expect(searchCommands(list, 'zzz')).toEqual([]);
  });

  it('matches a typed command by exact name, ignoring case', () => {
    const list = indexedCommands(index, 1);
    expect(matchTyped('/ECHO hi there', list)?.rest).toBe('hi there');
    expect(matchTyped('/echo', list)?.rest).toBe('');
    expect(matchTyped('/ech hi', list)).toBeNull();
    expect(matchTyped('hello', list)).toBeNull();
  });
});

describe('typed arguments', () => {
  it('gives free text to the first string option', () => {
    expect(parseTypedArgs(echo, 'hello world')).toEqual({
      path: [],
      values: { text: 'hello world' },
    });
  });

  it('reads name:value pairs, quoted values and leading free text', () => {
    expect(parseTypedArgs(echo, 'text:"two words" times:3 loud:true').values).toEqual({
      text: 'two words',
      times: '3',
      loud: 'true',
    });
    expect(parseTypedArgs(echo, 'hi times:2').values).toEqual({ text: 'hi', times: '2' });
    expect(parseTypedArgs(echo, 'bogus:1 text:x').values).toEqual({ text: 'x' });
  });

  it('walks sub-command groups', () => {
    const r = parseTypedArgs(admin, 'user ban who:55 reason:"spam bot"');
    expect(r.path).toEqual(['user', 'ban']);
    expect(r.values).toEqual({ who: '55', reason: 'spam bot' });
    expect(parseTypedArgs(admin, 'ping').path).toEqual(['ping']);
    expect(parseTypedArgs(admin, 'nothing').path).toEqual([]);
  });
});

describe('option tree', () => {
  it('lists choices and leaf options per level', () => {
    expect(subChoices(admin.options, []).map((o) => o.name)).toEqual(['user', 'ping']);
    expect(subChoices(admin.options, ['user']).map((o) => o.name)).toEqual(['ban']);
    expect(pathComplete(admin.options, [])).toBe(false);
    expect(pathComplete(admin.options, ['user', 'ban'])).toBe(true);
    expect(leafOptions(admin.options, ['user', 'ban']).map((o) => o.name)).toEqual([
      'who',
      'reason',
    ]);
    expect(leafOptions(echo.options, []).length).toBe(3);
  });

  it('runs commands without options immediately', () => {
    expect(runsImmediately({ ...echo, options: [] })).toBe(true);
    expect(runsImmediately(echo)).toBe(false);
    expect(runsImmediately({ ...echo, options: undefined })).toBe(true);
  });
});

describe('validation and wire format', () => {
  it('checks required, numbers, bounds and lengths', () => {
    const [text, times] = echo.options!;
    expect(optionError(text!, '')).toBe('required');
    expect(optionError(text!, 'x'.repeat(11))).toBe('maxLength');
    expect(optionError(text!, 'ok')).toBeNull();
    expect(optionError(times!, '')).toBeNull();
    expect(optionError(times!, 'abc')).toBe('number');
    expect(optionError(times!, '1.5')).toBe('integer');
    expect(optionError(times!, '0')).toBe('min');
    expect(optionError(times!, '9')).toBe('max');
    expect(
      optionError(
        { type: 3, name: 'c', description: '', choices: [{ name: 'A', value: 'a' }] },
        'b',
      ),
    ).toBe('choice');
    expect(formErrors(echo, [], {})).toEqual({ text: 'required' });
    expect(
      formErrors(
        { ...echo, options: [{ type: 11, name: 'file', description: '', required: true }] },
        [],
        {},
      ),
    ).toEqual({ file: 'required' });
  });

  it('builds typed values and leaves empty ones out', () => {
    expect(buildOptions(echo, [], { text: 'hi', times: '3', loud: 'true' })).toEqual([
      { type: 3, name: 'text', value: 'hi' },
      { type: 4, name: 'times', value: 3 },
      { type: 5, name: 'loud', value: true },
    ]);
    expect(buildOptions(echo, [], { text: 'hi', times: ' ' })).toEqual([
      { type: 3, name: 'text', value: 'hi' },
    ]);
  });

  it('nests values under their sub-commands', () => {
    expect(buildOptions(admin, ['user', 'ban'], { who: '55' })).toEqual([
      {
        type: 2,
        name: 'user',
        options: [{ type: 1, name: 'ban', options: [{ type: 6, name: 'who', value: '55' }] }],
      },
    ]);
    expect(buildOptions(admin, ['ping'], {})).toEqual([{ type: 1, name: 'ping', options: [] }]);
  });
});
