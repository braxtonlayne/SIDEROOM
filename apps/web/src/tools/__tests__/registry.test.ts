import { describe, expect, it } from 'vitest';
import { en } from '../../i18n/messages/en';
import { TOOLS, getTool, registryProblems, type ToolEntry } from '../registry';
import { editDistance, searchTools, type ToolText } from '../drawer/toolSearch';

describe('the registry', () => {
  it('has no duplicate or malformed ids and consistent keys', () => {
    expect(registryProblems()).toEqual([]);
  });
  it('has an English name and description for every tool', () => {
    for (const e of TOOLS) {
      expect(en[e.nameKey], e.id).toBeTruthy();
      expect(en[e.descriptionKey], e.id).toBeTruthy();
    }
  });
  it('detects problems', () => {
    const base = getTool('guide')!;
    const bad: ToolEntry[] = [base, { ...base }, { ...base, id: 'Bad_Id' }];
    const problems = registryProblems(bad);
    expect(problems).toContain('duplicate id: guide');
    expect(problems).toContain('bad id: Bad_Id');
  });
  it('loads the guide tool lazily', async () => {
    const mod = await getTool('guide')!.load();
    expect(typeof mod.default).toBe('function');
  });
});

const entry = (id: string, aliases: string[]): ToolEntry => ({
  ...getTool('guide')!,
  id,
  aliases,
});
const row = (id: string, name: string, description: string, aliases: string[] = []): ToolText => ({
  entry: entry(id, aliases),
  name,
  description,
});

describe('searchTools', () => {
  const list = [
    row('calc', 'Calculator', 'Do sums and convert units', ['calc', 'math']),
    row('dice', 'Dice roller', 'Roll RPG dice', ['roll', 'd20']),
    row('json', 'JSON tools', 'Format and validate JSON', ['pretty']),
  ];
  const ids = (q: string) => searchTools(list, q).map((r) => r.entry.id);
  it('returns everything for an empty query, in registry order', () => {
    expect(ids('')).toEqual(['calc', 'dice', 'json']);
    expect(ids('   ')).toEqual(['calc', 'dice', 'json']);
  });
  it('ranks a name prefix above an alias above a description word', () => {
    expect(ids('cal')[0]).toBe('calc');
    expect(ids('roll')[0]).toBe('dice');
    expect(ids('sums')).toEqual(['calc']);
  });
  it('finds a one-letter typo and ignores case and diacritics', () => {
    expect(ids('calculater')).toEqual(['calc']);
    expect(ids('DÍCE')).toEqual(['dice']);
  });
  it('does not match unrelated text', () => {
    expect(ids('zebra')).toEqual([]);
    expect(ids('xy')).toEqual([]);
  });
  it('measures a capped edit distance', () => {
    expect(editDistance('kitten', 'kitten')).toBe(0);
    expect(editDistance('kitten', 'sitten')).toBe(1);
    expect(editDistance('kitten', 'sitting')).toBeGreaterThan(1);
    expect(editDistance('a', 'abcd')).toBeGreaterThan(1);
  });
});
