// SEC-AUD-227 / 229 (D-716, D-712): the text of a desktop notification, and the reaction's read-through cache.
import type { MdNode } from '@sideroom/markdown-react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resetStore, useStore } from '../../store';
import { setMarkdownEngine, testFallbackEngine } from '../markdown';
import {
  notificationText,
  reactionBody,
  resetReactionTextCache,
  storeResolvers,
} from './notifyText';

const apiFetch = vi.fn();
vi.mock('../../api/client', async (orig) => ({
  ...(await orig<typeof import('../../api/client')>()),
  apiFetch: (...a: unknown[]) => apiFetch(...a),
}));

const SPOILER = '[spoiler]';
const names = {
  user: (id: string) => (id === '1' ? { name: 'Maya' } : undefined),
  role: (id: string) => (id === '2' ? { name: 'Mods' } : undefined),
  channel: (id: string) => (id === '3' ? { name: 'general' } : undefined),
};

beforeEach(() => {
  setMarkdownEngine(testFallbackEngine);
  resetStore();
  resetReactionTextCache();
  apiFetch.mockReset();
});
afterEach(() => {
  setMarkdownEngine(testFallbackEngine);
  vi.useRealTimers();
});

describe('notificationText', () => {
  it('hides a spoiler and names a user, a role and a channel', () => {
    const text = notificationText('secret ||the ending|| <@1> <@&2> <#3> <@9>', names, SPOILER);
    expect(text).toBe('secret [spoiler] @Maya @Mods #general @user');
    expect(text).not.toContain('ending');
    expect(text).not.toContain('<@');
  });
  it('writes a custom emoji as :name:, a link as its label, code as its text', () => {
    setMarkdownEngine({
      ...testFallbackEngine,
      parse: (): MdNode[] => [
        { type: 'custom_emoji', id: '77', name: 'blob', animated: false },
        { type: 'text', text: ' ' },
        {
          type: 'link',
          url: 'https://example.com/x',
          kind: 'masked',
          children: [{ type: 'text', text: 'the docs' }],
        },
        { type: 'text', text: ' ' },
        { type: 'inline_code', code: 'a + b' },
      ],
    });
    expect(notificationText('ignored', names, SPOILER)).toBe(':blob: the docs a + b');
  });
  it('keeps a list apart from the text before it', () => {
    setMarkdownEngine({
      ...testFallbackEngine,
      parse: (): MdNode[] => [
        { type: 'text', text: 'two' },
        {
          type: 'list',
          ordered: false,
          start: null,
          items: [[{ type: 'text', text: 'a' }]],
        } as MdNode,
      ],
    });
    expect(notificationText('x', undefined, SPOILER)).toBe('two a');
  });
  it('folds line breaks to one space', () => {
    expect(notificationText('one\n\ntwo\nthree', undefined, SPOILER)).toBe('one two three');
  });
  it('is cut at 140 characters with an ellipsis, and not between a letter and its mark', () => {
    const long = 'x'.repeat(300);
    expect(notificationText(long, undefined, SPOILER)).toBe(`${'x'.repeat(140)}…`);
    // A combining acute accent sits at the edge: the cut takes the letter out with it.
    const edge = `${'x'.repeat(139)}éyyy`;
    const cut = notificationText(edge, undefined, SPOILER);
    expect(cut).toBe(`${'x'.repeat(139)}…`);
    expect(cut.includes('́')).toBe(false);
  });
  it('is empty, never the raw text, when the parser is not loaded', () => {
    setMarkdownEngine(null);
    expect(notificationText('||secret|| <@1>', names, SPOILER)).toBe('');
  });
});

describe('storeResolvers', () => {
  it('reads users from the store and nothing it does not hold', () => {
    useStore
      .getState()
      .applyDispatch('USER_UPDATE', { id: '1', username: 'maya', global_name: 'Maya' });
    const r = storeResolvers('g1');
    expect(r.user?.('1')?.name).toBe('Maya');
    expect(r.user?.('404')).toBeUndefined();
    expect(r.role?.('2')).toBeUndefined();
    expect(r.channel?.('3')).toBeUndefined();
  });
});

describe('reactionBody', () => {
  const msgAt = (id: string, content: string) =>
    useStore.getState().applyDispatch('MESSAGE_CREATE', {
      id,
      channel_id: 'c1',
      guild_id: 'g1',
      author: { id: '1', username: 'me' },
      content,
      timestamp: '2026-01-01T00:00:00Z',
    });

  it('uses a loaded message without a request', async () => {
    msgAt('m1', 'loaded ||hidden||');
    expect(await reactionBody('c1', 'm1', 'g1', SPOILER)).toBe('loaded [spoiler]');
    expect(apiFetch).not.toHaveBeenCalled();
  });
  it('reads an unloaded message once, then answers from the cache', async () => {
    apiFetch.mockResolvedValue({ content: 'older **text**' });
    expect(await reactionBody('c1', 'm9', 'g1', SPOILER)).toBe('older text');
    expect(apiFetch).toHaveBeenCalledTimes(1);
    expect(apiFetch.mock.calls[0]?.[0]).toBe('/channels/c1/messages/m9');
    expect(await reactionBody('c1', 'm9', 'g1', SPOILER)).toBe('older text');
    expect(apiFetch).toHaveBeenCalledTimes(1);
  });
  it('shares one read between reactions that arrive while it is in flight', async () => {
    let answer: (v: unknown) => void = () => {};
    apiFetch.mockReturnValue(new Promise((r) => (answer = r)));
    const a = reactionBody('c1', 'm9', 'g1', SPOILER);
    const b = reactionBody('c1', 'm9', 'g1', SPOILER);
    answer({ content: 'once' });
    expect(await Promise.all([a, b])).toEqual(['once', 'once']);
    expect(apiFetch).toHaveBeenCalledTimes(1);
  });
  it('gives up after the limit, shows no text, and keeps a late answer for the next reaction', async () => {
    vi.useFakeTimers();
    let answer: (v: unknown) => void = () => {};
    apiFetch.mockReturnValue(new Promise((r) => (answer = r)));
    const first = reactionBody('c1', 'm9', 'g1', SPOILER);
    await vi.advanceTimersByTimeAsync(2000);
    expect(await first).toBe('');
    answer({ content: 'late but kept' });
    await vi.advanceTimersByTimeAsync(0);
    expect(await reactionBody('c1', 'm9', 'g1', SPOILER)).toBe('late but kept');
    expect(apiFetch).toHaveBeenCalledTimes(1);
  });
  it('remembers an unreadable message as empty and never asks again', async () => {
    apiFetch.mockRejectedValue(new Error('404'));
    expect(await reactionBody('c1', 'm9', 'g1', SPOILER)).toBe('');
    expect(await reactionBody('c1', 'm9', 'g1', SPOILER)).toBe('');
    expect(apiFetch).toHaveBeenCalledTimes(1);
  });
  it('keeps the last 20 messages: the 21st pushes the first out', async () => {
    apiFetch.mockImplementation(async (path: string) => ({ content: `text of ${path}` }));
    for (let i = 1; i <= 21; i++) await reactionBody('c1', `m${i}`, 'g1', SPOILER);
    expect(apiFetch).toHaveBeenCalledTimes(21);
    await reactionBody('c1', 'm5', 'g1', SPOILER);
    expect(apiFetch).toHaveBeenCalledTimes(21);
    await reactionBody('c1', 'm1', 'g1', SPOILER);
    expect(apiFetch).toHaveBeenCalledTimes(22);
  });
});
