import assert from 'node:assert/strict';
import { test } from 'node:test';
import { toJunit } from '../lib/junit.mjs';
import { checkNames } from '../lib/steps.mjs';

test('a failed case carries an escaped message and the counts add up', () => {
  const xml = toJunit({
    library: 'discord.js',
    version: '14.27.0',
    cases: [
      { name: 'ready', ok: true, seconds: 0.5 },
      { name: 'content "gate"', ok: false, message: 'saw <b>&</b>\u0001', seconds: 1 },
    ],
  });
  assert.match(xml, /<testsuites name="sdk-compat" tests="2" failures="1" time="1.500">/);
  assert.match(xml, /<testsuite name="discord.js 14.27.0" tests="2" failures="1"/);
  assert.match(xml, /name="content &quot;gate&quot;"/);
  assert.match(xml, /saw &lt;b&gt;&amp;&lt;\/b&gt;\?/);
  assert.doesNotMatch(xml, /\u0001/);
});

test('the check list is the tier-2 list of docs/dev/sdk-compat.md', () => {
  assert.deepEqual(checkNames, [
    'ready',
    'guild-create',
    'command-registered',
    'message-content-gated',
    'forbidden-delete',
    'slash-command-round-trip',
    'message-content-delivered',
  ]);
});

test('a failed report becomes one issue titled with the library and version', async () => {
  const { issueFor, summarise } = await import('../open-issue.mjs');
  const xml = toJunit({
    library: 'serenity',
    version: '0.12.5',
    cases: [
      { name: 'ready', ok: true },
      { name: 'forbidden-delete', ok: false, message: 'delete answered 200' },
    ],
  });
  const s = summarise(xml);
  assert.deepEqual([s.name, s.tests, s.failed], ['serenity 0.12.5', 2, 1]);
  const issue = issueFor(s, 'https://example.test/run/1');
  assert.equal(issue.title, 'SDK compat: serenity 0.12.5');
  assert.match(issue.body, /`forbidden-delete`: delete answered 200/);
});
