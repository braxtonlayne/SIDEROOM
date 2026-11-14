// SEC-AUD-242..245 (wave 70, D-728, D-722, D-723, D-731): the preview cut never splits markup (the cases the
// server runs too, `crates/sr-chat/src/preview_cases.json`), plain text keeps blocks apart, a restored message
// counts in its thread, and a timestamp in one-line text is a date.
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { MdNode } from '@sideroom/markdown-react';
import { loadMarkdown } from '@sideroom/wasm';
import { beforeEach, describe, expect, it } from 'vitest';
import {
  currentMarkdownEngine,
  parseMarkdown,
  setMarkdownEngine,
  testFallbackEngine,
} from '../markdown';
import { excerpt, excerptPlain } from '../msg/excerpt';
import { singleLineText } from '../msg/SingleLine';
import { applyThreadDispatch, COUNT_CAP, useThreads } from '../threads/threadStore';
import { applyRequestPreview } from '../previewLive';
import { notificationText } from '../shell/notifyText';
import type { DmObject } from '../shell/api';

type Part = string | { repeat: string; times: number };
interface Case {
  name: string;
  input: Part[];
  want: Part[] | null;
}
const join = (parts: Part[]): string =>
  parts.map((p) => (typeof p === 'string' ? p : p.repeat.repeat(p.times))).join('');

const casesPath = resolve(process.cwd(), '../../crates/sr-chat/src/preview_cases.json');
const cases = (JSON.parse(readFileSync(casesPath, 'utf8')) as { cases: Case[] }).cases;
const wasmPath = resolve(process.cwd(), '../../packages/wasm/dist/markdown/markdown_bg.wasm');

const line = (text: string) => singleLineText(parseMarkdown(text), undefined, '[spoiler]');

describe('the preview cut never splits markup (D-728), the same cases as the server', () => {
  it.each(cases.map((c) => [c.name, c] as const))('%s (test stand-in parser)', (_n, c) => {
    // The stand-in has no custom emoji: its cases are the ones it can read.
    if (c.name.includes('custom emoji')) return;
    expect(excerpt(join(c.input))).toBe(c.want === null ? null : join(c.want));
  });

  describe.skipIf(!existsSync(wasmPath))('with the real sr-markdown parser', () => {
    const prev = currentMarkdownEngine();
    beforeEach(async () => {
      const md = await loadMarkdown(readFileSync(wasmPath) as unknown as URL);
      setMarkdownEngine({
        parse: (t) => md.parse(t) as unknown as MdNode[],
        isJumbo: (t) => md.isJumbo(t),
        expandShortcodes: (t) => md.expandShortcodes(t),
      });
      return () => setMarkdownEngine(prev ?? testFallbackEngine);
    });
    it.each(cases.map((c) => [c.name, c] as const))('%s', (_n, c) => {
      expect(excerpt(join(c.input))).toBe(c.want === null ? null : join(c.want));
    });
  });

  it('confirm (b): the one-line text of a cut spoiler never shows the hidden letters', () => {
    const got = excerpt(`a ||${'x'.repeat(200)}||`)!;
    expect(line(got)).not.toContain('xxx');
  });

  it('confirm (c): a mention cut in the middle is not shown as half an id', () => {
    const got = excerpt(`${'x'.repeat(130)} <@123456789012345678> tail`)!;
    expect(got).not.toContain('<@');
  });

  it('a text of 140 characters or fewer never needs the parser', () => {
    setMarkdownEngine(null);
    try {
      expect(excerpt('a ||secret||')).toBe('a ||secret||');
      // A cut is needed and there is no parser: no text, never the raw cut.
      expect(excerpt(`a ||${'x'.repeat(200)}||`)).toBeNull();
    } finally {
      setMarkdownEngine(testFallbackEngine);
    }
  });

  it('the plain cut (for a notification) does not look at markup', () => {
    expect(excerptPlain(`a ||${'x'.repeat(200)}||`)).toBe(`a ||${'x'.repeat(136)}…`);
    expect(excerptPlain('  ')).toBeNull();
  });

  it('a notification still cuts its own plain line', () => {
    const t = notificationText(`${'x'.repeat(130)} **bold words here** tail`, undefined, 'spoiler');
    expect(t).toBe(`${'x'.repeat(130)} bold word…`);
  });

  it('a live request preview carries no hidden words', () => {
    const reqs = [{ id: 'dm', type: 1 }] as unknown as DmObject[];
    const out = applyRequestPreview(
      reqs,
      'MESSAGE_CREATE',
      {
        id: '9',
        type: 0,
        channel_id: 'dm',
        content: `look ||${'hidden'.repeat(40)}||`,
        author: { id: '2' },
        attachments: [],
      },
      '1',
    );
    const text = out[0]!.sr_request_preview!.content;
    expect(text).toBe('look…');
  });
});

describe('plain text keeps blocks apart (D-722)', () => {
  const t = (type: string, extra: Record<string, unknown> = {}) => ({ type, ...extra }) as MdNode;
  const text = (s: string): MdNode => ({ type: 'text', text: s });
  const list: MdNode = { type: 'list', ordered: false, start: null, items: [[text('a')]] };
  const code: MdNode = { type: 'code_block', lang: null, code: 'x' };
  const quote = t('quote', { children: [text('q')] });
  const heading = t('heading', { level: 1, children: [text('h')] });
  const sub = t('subtext', { children: [text('s')] });
  const one = (nodes: MdNode[]) => singleLineText(nodes, undefined, 'spoiler');

  it.each([
    ['text then list', [text('two'), list], 'two a'],
    ['text then code block', [text('two'), code], 'two x'],
    ['code block then text', [code, text('two')], 'x two'],
    ['text then quote', [text('two'), quote], 'two q'],
    ['quote then text', [quote, text('two')], 'q two'],
    ['text then heading', [text('two'), heading], 'two h'],
    ['heading then text', [heading, text('two')], 'h two'],
    ['text then subtext', [text('two'), sub], 'two s'],
    ['list then text', [list, text('two')], 'a two'],
    ['two lists', [list, list], 'a a'],
    ['a list inside a quote', [t('quote', { children: [text('two'), list] })], 'two a'],
    ['no block, no separator', [text('two'), text('three')], 'twothree'],
  ] as [string, MdNode[], string][])('%s', (_n, nodes, want) => {
    expect(one(nodes)).toBe(want);
  });

  it('a notification needs no work-around for a list', () => {
    const nodes: MdNode[] = [text('two'), list];
    expect(one(nodes)).toBe('two a');
  });
});

describe('a timestamp is a date (D-731)', () => {
  const ts = (style: Extract<MdNode, { type: 'timestamp' }>['style']): MdNode[] => [
    { type: 'text', text: 'at ' },
    { type: 'timestamp', unix: 1700000000, style },
  ];
  const zone = { locale: 'en-US', timeZone: 'UTC' };

  it('reads as the message renderer shows it, in the given zone', () => {
    expect(singleLineText(ts('short_time'), undefined, 's', zone)).toBe('at 10:13 PM');
    expect(singleLineText(ts('long_date'), undefined, 's', zone)).toBe('at November 14, 2023');
    expect(singleLineText(ts(null), undefined, 's', zone)).toContain('November 14, 2023');
  });

  it('is not dropped any more', () => {
    expect(singleLineText(ts(null), undefined, 's', zone)).not.toBe('at');
  });

  it('an unreadable value shows what the renderer shows', () => {
    const bad: MdNode[] = [{ type: 'timestamp', unix: 1e20, style: null }];
    expect(singleLineText(bad, undefined, 's', zone)).toBe('Invalid Date');
  });
});

describe('a restored message counts in its thread (D-723)', () => {
  beforeEach(() => useThreads.getState().reset());
  const wire = (n: number, last: string | null) => ({
    id: 't1',
    type: 11,
    guild_id: 'g1',
    parent_id: 'c1',
    name: 'thread',
    owner_id: '2',
    message_count: n,
    member_count: 1,
    last_message_id: last,
    thread_metadata: { archived: false, locked: false, archive_timestamp: '2026-01-01T00:00:00Z' },
  });
  const hold = (n: number, last: string | null) => useThreads.getState().upsert([wire(n, last)]);
  const info = () => useThreads.getState().byId['t1']!;
  const create = (id: string, restored?: boolean) =>
    applyThreadDispatch('MESSAGE_CREATE', {
      id,
      channel_id: 't1',
      type: 0,
      ...(restored === undefined ? {} : { restored }),
    });

  it('counts an old restored message and leaves the last message where it was', () => {
    hold(3, '100');
    create('50', true);
    expect(info().messageCount).toBe(4);
    expect(info().lastMessageId).toBe('100');
  });

  it('moves the last message when the restored one is the newest', () => {
    hold(3, '100');
    create('105', true);
    expect(info().messageCount).toBe(4);
    expect(info().lastMessageId).toBe('105');
  });

  it('moves nothing at the cap', () => {
    hold(COUNT_CAP, '100');
    create('50', true);
    expect(info().messageCount).toBe(COUNT_CAP);
  });

  it('an ordinary replay of an old id is still not counted', () => {
    hold(3, '100');
    create('50');
    create('100');
    create('50', false);
    expect(info().messageCount).toBe(3);
  });
});
