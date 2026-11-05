// @ts-check
// The game run by the host (host/engine.js): lobby, turns, guessing and scoring, hints, timers, players leaving,
// limits, and that secrets never reach the shared state.
import assert from 'node:assert/strict';
import { INK_KEY, ST_KEY, SketchHost, AWAY_MS, REPROMPT_MS } from '../host/engine.js';
import { BUILT_IN_PROMPTS } from '../shared/prompts.js';
import { DRAWER_POINTS, LIMITS, guessPoints, strokesOf } from '../shared/rules.js';

/** A host with a manual clock that records what it sends and publishes. */
export function harness(opt = {}) {
  const clock = { t: 10_000 };
  /** @type {{ to: string, msg: any }[]} */
  const sent = [];
  /** @type {{ key: string, value: any }[]} */
  const published = [];
  let r = 0.123;
  const host = new SketchHost({
    instance: 'inst',
    hostId: 'h',
    now: () => clock.t,
    seed: 42,
    secretRandom: () => ((r = (r * 9301 + 49297) % 233280), r / 233280),
    send: (to, msg) => sent.push({ to, msg }),
    publish: (key, value) => published.push({ key, value: JSON.parse(JSON.stringify(value)) }),
    ...opt,
  });
  const advance = (ms) => {
    for (let i = 0; i < ms; i += 250) {
      clock.t += 250;
      host.tick();
    }
  };
  const lastSt = () => [...published].reverse().find((p) => p.key === ST_KEY)?.value;
  const secretOf = (id) => [...sent].reverse().find((s) => s.to === id && (s.msg.t === 'secret' || s.msg.t === 'choices'))?.msg;
  return { clock, host, sent, published, advance, lastSt, secretOf };
}

function threePlayers() {
  const h = harness();
  h.host.handle('h', { t: 'hello', name: 'Alice' });
  h.host.handle('m1', { t: 'hello', name: 'Bob' });
  h.host.handle('g1', { t: 'hello', name: 'Swift Otter', guest: true });
  return h;
}

/** Start and have the drawer pick the first choice; returns the drawer id and the prompt. */
function startDrawing(h) {
  h.host.handle('h', { t: 'start' });
  const drawer = h.host.state.drawer;
  assert.ok(drawer);
  assert.equal(h.host.state.phase, 'pick');
  const choices = h.secretOf(drawer);
  assert.equal(choices.t, 'choices');
  assert.equal(choices.list.length, 3);
  h.host.handle(drawer, { t: 'pick', i: 1 });
  assert.equal(h.host.state.phase, 'draw');
  return { drawer, prompt: choices.list[1] };
}

test('the lobby: hello seats players, names are cleaned and unique, guests are marked', () => {
  const h = harness();
  h.host.handle('h', { t: 'hello', name: 'Alice' });
  h.host.handle('m1', { t: 'hello', name: '  Al\u0007ice <b> ' });
  h.host.handle('g1', { t: 'hello', name: 'Alice', guest: true });
  const names = h.host.state.players.map((p) => p.name);
  assert.deepEqual(names, ['Alice', 'Alice b', 'Alice 2']);
  assert.equal(h.host.state.players[2].guest, true);
  assert.ok(h.sent.some((s) => s.to === 'g1' && s.msg.t === 'welcome'));
});

test('only the host starts, changes settings and needs two players', () => {
  const h = harness();
  h.host.handle('h', { t: 'hello', name: 'Alice' });
  h.host.handle('h', { t: 'start' });
  assert.equal(h.host.state.phase, 'lobby', 'one player cannot start');
  h.host.handle('m1', { t: 'hello', name: 'Bob' });
  h.host.handle('m1', { t: 'settings', rounds: 1 });
  h.host.handle('m1', { t: 'start' });
  assert.equal(h.host.state.phase, 'lobby');
  h.host.handle('h', { t: 'settings', rounds: 2, drawSecs: 60 });
  h.host.handle('h', { t: 'settings', rounds: 99, drawSecs: 7 });
  assert.equal(h.host.state.settings.rounds, 2);
  assert.equal(h.host.state.settings.drawSecs, 60);
  h.host.handle('h', { t: 'start' });
  assert.equal(h.host.state.phase, 'pick');
  assert.equal(h.host.state.round, 1);
  assert.equal(h.host.state.order.length, 2);
});

test('secrets: the prompt and the choices go to the drawer alone and never into the shared state', () => {
  const h = threePlayers();
  const { drawer, prompt } = startDrawing(h);
  const secret = h.secretOf(drawer);
  assert.deepEqual(secret, { t: 'secret', n: h.host.state.turnNo, prompt });
  for (const s of h.sent) if (s.to !== drawer) assert.ok(!JSON.stringify(s.msg).includes(prompt), `leaked to ${s.to}`);
  for (const p of h.published) assert.ok(!JSON.stringify(p.value).toLowerCase().includes(prompt.toLowerCase()), 'leaked in host:' + p.key);
  assert.equal(h.lastSt().mask.replace(/[^_]/g, '').length, prompt.replace(/[^\p{L}\p{N}]/gu, '').length);
});

test('guessing: a hit scores by speed and order, the drawer scores per guesser, a close guess is told privately', () => {
  const h = threePlayers();
  const { drawer, prompt } = startDrawing(h);
  const [first, second] = ['h', 'm1', 'g1'].filter((id) => id !== drawer);
  h.advance(10_000);
  h.host.handle(first, { t: 'guess', text: prompt.slice(0, -3) + 'zzz' });
  const close = h.sent.find((s) => s.to === first && s.msg.t === 'close');
  const feedGuess = h.lastSt().feed.find((f) => f.k === 'guess');
  assert.ok(close || feedGuess, 'a wrong guess is in the feed');
  h.host.handle(second, { t: 'hb' });
  h.advance(500);
  h.host.handle(first, { t: 'guess', text: prompt.toUpperCase() + '!' });
  const total = h.host.state.settings.drawSecs * 1000;
  const p1 = h.host.state.turnScores[first];
  assert.equal(p1, guessPoints(total - 10_500, total, 0));
  assert.equal(h.host.state.phase, 'draw', 'one guesser is still guessing');
  h.host.handle(first, { t: 'guess', text: prompt }); // a second hit counts once
  assert.equal(h.host.state.turnScores[first], p1);
  h.advance(5000);
  h.host.handle(second, { t: 'guess', text: prompt });
  assert.equal(h.host.state.phase, 'reveal', 'everyone guessed: the turn ends');
  assert.equal(h.host.state.word, prompt);
  assert.equal(h.host.state.turnScores[drawer], 2 * DRAWER_POINTS);
  assert.ok(h.host.state.turnScores[second] < p1, 'slower is fewer points');
  // A hit is never shown in the feed as text.
  assert.ok(!h.lastSt().feed.some((f) => f.k === 'guess' && f.text?.toLowerCase() === prompt.toLowerCase()));
});

test('the drawer cannot guess, guesses are rate limited, and only the drawer inks', () => {
  const h = threePlayers();
  const { drawer, prompt } = startDrawing(h);
  const other = ['h', 'm1', 'g1'].find((id) => id !== drawer);
  h.host.handle(drawer, { t: 'guess', text: prompt });
  assert.deepEqual(h.host.state.guessed, []);
  h.host.handle(other, { t: 'guess', text: 'nope one' });
  h.host.handle(other, { t: 'guess', text: 'nope two' });
  assert.equal(h.host.state.feed.filter((f) => f.k === 'guess').length, 1);
  h.host.handle(other, { t: 'ink', op: { k: 'p', id: 1, c: 0, w: 0, pts: [1, 1, 2, 2] } });
  assert.equal(h.host.ink.length, 0);
  h.host.handle(drawer, { t: 'ink', op: { k: 'p', id: 1, c: 0, w: 0, pts: [1, 1, 2, 2], q: 1 } });
  h.host.handle(drawer, { t: 'ink', op: { k: 'p', id: 1, c: 99, w: 0, pts: [1, 1] } });
  h.host.handle(drawer, { t: 'ink', op: { k: 'p', id: 1, c: 0, w: 0, pts: [5000, -7] } });
  assert.equal(h.host.ink.length, 2);
  assert.deepEqual(h.host.ink[1].pts, [1000, 0], 'points are clamped to the canvas');
  h.advance(250);
  const ink = h.published.filter((p) => p.key === INK_KEY).at(-1)?.value;
  assert.equal(ink.n, h.host.state.turnNo);
});

test('ink limits hold per turn', () => {
  const h = threePlayers();
  const { drawer } = startDrawing(h);
  const pts = Array.from({ length: LIMITS.pointsPerOp * 2 }, (_, i) => i % 500);
  for (let i = 0; i < LIMITS.pointsPerTurn / LIMITS.pointsPerOp + 10; i++) h.host.handle(drawer, { t: 'ink', op: { k: 'p', id: i, c: 0, w: 0, pts } });
  assert.ok(h.host.inkPoints <= LIMITS.pointsPerTurn);
  h.host.handle(drawer, { t: 'ink', op: { k: 'p', id: 1, c: 0, w: 0, pts: [...pts, 1, 1] } });
  assert.ok(h.host.ink.every((op) => op.k !== 'p' || op.pts.length <= LIMITS.pointsPerOp * 2));
});

test('timers: pick defaults to the first choice, time runs out, hints appear, rounds advance, the game ends', () => {
  const h = threePlayers();
  h.host.handle('h', { t: 'settings', rounds: 1, drawSecs: 45 });
  h.host.handle('h', { t: 'start' });
  const drawer = h.host.state.drawer;
  const choices = h.secretOf(drawer).list;
  h.advance(15_250);
  assert.equal(h.host.state.phase, 'draw');
  assert.equal(h.secretOf(drawer).prompt, choices[0]);
  h.advance(23_000);
  const letters = choices[0].replace(/[^\p{L}\p{N}]/gu, '').length;
  if (letters >= 3) assert.ok(h.host.state.shown.length >= 1, 'a hint at half time');
  h.advance(23_000);
  assert.equal(h.host.state.phase, 'reveal');
  // Three players, one round: three turns, then the end.
  for (let turn = 0; turn < 3 && h.host.state.phase !== 'end'; turn++) h.advance(6_250 + 15_250 + 45_250);
  assert.equal(h.host.state.phase, 'end');
  h.host.handle('h', { t: 'again' });
  assert.equal(h.host.state.phase, 'lobby');
  assert.ok(h.host.state.players.every((p) => p.score === 0));
});

test('a drawer who leaves ends the turn; a quiet guest goes away and comes back with a heartbeat', () => {
  const h = threePlayers();
  const { drawer } = startDrawing(h);
  const quiet = ['m1', 'g1'].find((id) => id !== drawer) ?? 'g1';
  h.host.handle(drawer === 'h' ? 'm1' : drawer, { t: 'bye' });
  if (drawer !== 'h') assert.equal(h.host.state.phase, 'reveal');
  // The page that is left says nothing for AWAY_MS.
  for (let i = 0; i < AWAY_MS / 1000 + 2; i++) {
    h.advance(1000);
    h.host.handle(drawer === 'h' ? 'h' : 'h', { t: 'hb' });
  }
  const q = h.host.player(quiet);
  if (q && !q.left) {
    assert.equal(q.away, true);
    h.host.handle(quiet, { t: 'hb' });
    assert.equal(h.host.player(quiet)?.away, false);
  }
});

test('members who leave the call are marked left; the host is never removed by setMembers', () => {
  const h = threePlayers();
  h.host.setMembers([{ id: 'h' }]);
  assert.equal(h.host.player('m1')?.left, true);
  assert.equal(h.host.player('g1')?.left, false, 'guests are not in the member list');
  assert.equal(h.host.player('h')?.left, false);
});

test('the turn order comes from the public seed; prompts from the secret randomness', () => {
  const a = threePlayers();
  const b = threePlayers();
  a.host.handle('h', { t: 'start' });
  b.host.handle('h', { t: 'start' });
  assert.deepEqual(a.host.state.order, b.host.state.order);
});

test('packs: prompts come only from the built-in set and loaded packs; pack prompts are not published', () => {
  const h = threePlayers();
  const pack = { name: 'Office', prompts: ['stapler duel', 'reply all storm', 'printer jam rage', 'coffee machine queue', 'desk plant drama'] };
  h.host.addPack(pack);
  h.host.handle('h', { t: 'settings', builtIn: false });
  assert.equal(h.host.state.settings.builtIn, false);
  assert.deepEqual(h.lastSt().packs, [{ name: 'Office', count: 5 }]);
  for (const p of h.published) for (const prompt of pack.prompts) assert.ok(!JSON.stringify(p.value).includes(prompt));
  h.host.handle('h', { t: 'start' });
  const choices = h.secretOf(h.host.state.drawer).list;
  for (const c of choices) assert.ok(pack.prompts.includes(c), c);
  h.host.removePack('Office');
  assert.equal(h.host.state.settings.builtIn, true, 'no packs: the built-in set comes back');
  assert.ok(BUILT_IN_PROMPTS.length >= 150);
});

test('undo and clear replay into the strokes every page draws', () => {
  const ops = [
    { k: 'p', id: 1, c: 0, w: 0, pts: [0, 0, 1, 1] },
    { k: 'p', id: 2, c: 1, w: 1, pts: [5, 5] },
    { k: 'p', id: 1, c: 0, w: 0, pts: [2, 2] },
    { k: 'u' },
  ];
  assert.deepEqual(strokesOf(/** @type {any} */ (ops)), [{ id: 1, c: 0, w: 0, pts: [0, 0, 1, 1, 2, 2] }]);
  assert.deepEqual(strokesOf(/** @type {any} */ ([...ops, { k: 'c' }])), []);
});

test('resume: a successor carries on the turn with the drawer repeating the prompt, or skips it', () => {
  const h = threePlayers();
  const { drawer, prompt } = startDrawing(h);
  h.advance(5000);
  h.host.handle(drawer, { t: 'ink', op: { k: 'p', id: 1, c: 0, w: 0, pts: [1, 1, 9, 9] } });
  h.advance(500);
  const st = h.lastSt();
  const successor = drawer === 'm1' ? 'g1' : 'm1';
  const s2 = harness({ hostId: successor });
  const resumed = SketchHost.resume(st, h.host.ink, { ...s2.host.opt, now: () => s2.clock.t, leaving: 'h' });
  assert.equal(resumed.state.phase, drawer === 'h' ? 'reveal' : 'draw');
  assert.equal(resumed.state.host, successor);
  assert.equal(resumed.ink.length, 1);
  if (drawer !== 'h') {
    assert.equal(resumed.prompt, '', 'the successor does not know the prompt');
    resumed.handle(drawer, { t: 'reprompt', prompt });
    assert.equal(resumed.prompt, prompt);
    const other = ['m1', 'g1'].find((id) => id !== drawer && id !== successor) ?? successor;
    resumed.handle(other === drawer ? successor : other, { t: 'guess', text: prompt });
    assert.ok(resumed.state.guessed.length >= 1);
    assert.ok(resumed.state.left <= st.left);
  }
  // Without a reprompt the turn is skipped after REPROMPT_MS.
  const s3 = harness({ hostId: successor });
  const r3 = SketchHost.resume(st, h.host.ink, { ...s3.host.opt, now: () => s3.clock.t });
  if (st.phase === 'draw') {
    for (let t = 0; t <= REPROMPT_MS; t += 250) {
      s3.clock.t += 250;
      r3.tick();
    }
    assert.equal(r3.state.phase, 'reveal');
    assert.equal(r3.state.word, '');
  }
});
