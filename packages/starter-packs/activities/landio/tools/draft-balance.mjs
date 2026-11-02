// Draft Night balance runs (ACT-LND-002): bots play whole runs and this prints how the families, set bonuses
// and keepers did. Used to tune shared/modes/draft.js; the numbers it printed are in REPORT-46A.md.
//   node tools/draft-balance.mjs [runs=20] [roundSeconds=150]
import { CARDS } from '../shared/cards.js';
import { FAMILIES, familyCounts, setBonuses } from '../shared/modes/draft.js';
import { RESULTS_TICKS, createState, step } from '../shared/rules.js';

const runs = Number(process.argv[2] ?? 20);
const roundTicks = Number(process.argv[3] ?? 150) * 7;
const fams = Object.keys(FAMILIES);

/** Which family a player leans on: the one with most cards held (ties: none). @param {any} p */
const lean = (p) => {
  const c = familyCounts(p);
  const top = fams.filter((f) => c[f] === Math.max(...fams.map((g) => c[g])) && c[f] > 0);
  return top.length === 1 ? top[0] : 'none';
};

const wins = Object.fromEntries([...fams, 'none'].map((f) => [f, 0]));
const seen = Object.fromEntries([...fams, 'none'].map((f) => [f, 0]));
const bonusWin = { with: 0, without: 0, heldWith: 0, heldWithout: 0 };
const keeperRarity = { lead: [0, 0], trail: [0, 0] };
const champPts = [];
/** Mean score (share of the round's best) by lean and tier. @type {Record<string, number[]>} */
const share = {};
let rounds = 0;

for (let r = 0; r < runs; r++) {
  const s = createState({ w: 80, h: 80, targetHeads: 10, mode: 'draft', roundTicks }, `balance-${r}`, { fill: true });
  const total = (roundTicks + RESULTS_TICKS) * 3;
  for (let t = 0; t < total; t++) {
    const before = s.round.phase;
    step(s);
    s.events.length = 0;
    // Keeper rarity by run standing at the moment a keeper offer is made.
    for (const p of s.players) {
      const o = p.offers[0];
      if (!o?.keep || p.fx.seenKeep === o.until) continue;
      p.fx.seenKeep = o.until;
      const md = /** @type {any} */ (s.md);
      if (md.leg === 1) continue;
      const i = md.keys.indexOf(`b:${p.name}`);
      const place = 1 + md.pts.filter((v) => v > md.pts[i]).length;
      const side = place <= md.pts.length / 2 ? 'lead' : 'trail';
      keeperRarity[side][0] += o.cards.filter((id) => CARDS[id].rarity !== 'common').length;
      keeperRarity[side][1] += o.cards.length;
    }
    if (before === 'play' && s.round.phase === 'over') {
      rounds++;
      const res = s.round.result;
      const alive = s.players.filter((p) => p.alive);
      const best = Math.max(1, ...res.players.map((l) => l.score));
      for (const p of alive) {
        const tier = Math.max(...Object.values(setBonuses(p)));
        const key = `${lean(p)}:${tier}`;
        (share[key] ??= []).push((res.players.find((l) => l.id === p.id)?.score ?? 0) / best);
        seen[lean(p)]++;
        const b = Object.values(setBonuses(p)).some((v) => v > 0);
        if (b) bonusWin.heldWith++;
        else bonusWin.heldWithout++;
      }
      const w = s.players.find((p) => p.id === res.players[0]?.id);
      if (w && w.alive) {
        wins[lean(w)]++;
        if (Object.values(setBonuses(w)).some((v) => v > 0)) bonusWin.with++;
        else bonusWin.without++;
      }
      if (res.run?.final) champPts.push(res.run.lines[0]?.pts ?? 0);
    }
  }
}

console.log(`${runs} runs, ${rounds} rounds of ${roundTicks / 7} s`);
console.log('round winners by the family they lean on (wins / alive at round end with that lean):');
for (const f of [...fams, 'none']) console.log(`  ${f.padEnd(9)} ${wins[f]} / ${seen[f]}  (${seen[f] ? ((100 * wins[f]) / seen[f]).toFixed(1) : '-'}% win per holder)`);
console.log(`winners with a set bonus: ${bonusWin.with}, without: ${bonusWin.without}; holders with: ${bonusWin.heldWith}, without: ${bonusWin.heldWithout}`);
console.log(`keeper offers, share of rare+epic: leading half ${(keeperRarity.lead[0] / keeperRarity.lead[1]).toFixed(2)}, trailing half ${(keeperRarity.trail[0] / keeperRarity.trail[1]).toFixed(2)}`);
for (const k of Object.keys(share).sort()) console.log(`  score share ${k.padEnd(12)} ${(share[k].reduce((a, b) => a + b, 0) / share[k].length).toFixed(3)} (n=${share[k].length})`);
console.log(`champion points: ${champPts.join(' ')}`);
