// @ts-check
// HUD and menus, plain DOM. Text goes in with textContent only (names come from other people). Panels update
// when their numbers change, not every frame: update() and cards() run at most 4 times a second, the rest is
// event-driven (kills, captures, denied actions, toasts, tips), so an idle game does no HUD work at all.

import { lookColor } from '../shared/look.js';
import { CARDS, OFFER_TICKS, ACT } from '../shared/cards.js';
import { firstKey, hintText } from './input.js';
import { MODE_BY_ID, TEAMS } from '../shared/modes/index.js';
import { MAP_BY_ID } from '../shared/maps/index.js';
import { MODE_VIEWS } from './modes/index.js';

/** @typedef {import('./world.js').World} World */

const FEED_MS = 6000;
const FEED_MAX = 5;
/** Land changes smaller than this (percent) are not worth a popup. */
const LAND_EPS = 0.05;
const DELTA_MS = 1100;
const DENIED_MS = 300;
const READY_MS = 600;
const TOAST_MAX = 3;
const TOAST_MS = 2500;
/** A toast fades for the last part of its time on screen. */
const TOAST_FADE_MS = 300;
const TIP_MS = 5000;
const LB_ROWS = 5;

/** @param {string} id */
const $ = (id) => /** @type {HTMLElement} */ (document.getElementById(id));

/**
 * Restart a one-shot class on an element: remove, force a reflow so the CSS animation replays, add, and take
 * it off after `ms`. A repeat while it is on restarts the timer.
 * @type {WeakMap<HTMLElement, Record<string, ReturnType<typeof setTimeout>>>}
 */
const pulseTimers = new WeakMap();
/** @param {HTMLElement} el @param {string} cls @param {number} ms */
function pulse(el, cls, ms) {
  let timers = pulseTimers.get(el);
  if (!timers) pulseTimers.set(el, (timers = {}));
  clearTimeout(timers[cls]);
  el.classList.remove(cls);
  void el.offsetWidth;
  el.classList.add(cls);
  timers[cls] = setTimeout(() => el.classList.remove(cls), ms);
}

/** Seconds to one decimal under 10 s, whole seconds above. @param {number} s */
const secs = (s) => (s < 10 ? s.toFixed(1) : String(Math.ceil(s)));

/** @param {number} ticks @param {number} hz */
export function clock(ticks, hz) {
  const s = Math.max(0, Math.floor(ticks / hz));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** @param {number} land @param {number} tiles */
export const pct = (land, tiles) => `${((100 * land) / Math.max(1, tiles)).toFixed(land && land * 1000 < tiles ? 2 : 1)}%`;

const CAUSES = /** @type {Record<string, (k: string | null) => string>} */ ({
  trail: (k) => `${k ?? 'Someone'} ran over your trail.`,
  self: () => 'You crossed your own trail.',
  head: (k) => `Head-on crash${k ? ` with ${k}` : ''}.`,
  border: () => 'You left the map.',
  wall: () => 'You ran into a wall.',
  enclosed: (k) => `${k ?? 'Someone'} closed a loop around you.`,
  landless: (k) => `${k ?? 'Someone'} took all your land.`,
  removed: () => 'You left the arena.',
});

/** @param {string} cls @param {string} text */
function span(cls, text) {
  const el = document.createElement('span');
  el.className = cls;
  el.textContent = text;
  return el;
}

/** A small colour square. @param {string} color */
function swatch(color) {
  const el = document.createElement('span');
  el.className = 'sw';
  el.style.background = color;
  return el;
}

/** Whether the one-time tip `key` has been shown in this browser. A storage that throws counts as not shown. @param {string} key */
function tipSeen(key) {
  try {
    return localStorage.getItem(key) !== null;
  } catch {
    return false;
  }
}

/** @param {string} key */
function markTipSeen(key) {
  try {
    localStorage.setItem(key, '1');
  } catch {
    /* a private window: the tip may come back there, which is harmless */
  }
}

export class Hud {
  constructor() {
    this.root = $('hud');
    this.lb = $('lb-list');
    this.feed = $('killfeed');
    /** What the team totals and the results card show now (to skip redundant DOM work). */
    this.teamSig = '';
    this.resSig = '';
    this.mm = /** @type {HTMLCanvasElement} */ ($('minimap'));
    this.mmCtx = /** @type {CanvasRenderingContext2D} */ (this.mm.getContext('2d'));
    /** @type {ImageData | null} */
    this.mmImg = null;
    this.mmCanvas = document.createElement('canvas');
    this.lbSig = '';
    this.statSig = '';
    this.mmVer = -1;
    this.offerSig = '';
    this.invSig = '';
    /** The last world and tick rate from update(), for kill names and colours and the ability timer. @type {World | null} */
    this.world = null;
    this.hz = 7;
    /** Leaderboard cells for the top rows, in order, updated in place. @type {HTMLElement[]} */
    this.lbCells = [];
    /** The extra row for the local player when outside the top rows. @type {{li: HTMLElement, name: HTMLElement, pc: HTMLElement} | null} */
    this.lbExtra = null;
    /** The ability slot's recharge state, to see the moment it becomes ready. @type {{id: string, charging: boolean} | null} */
    this.abState = null;
    /** @type {{id: string, text: string}[]} */
    this.tipQueue = [];
    this.tipShowing = false;
    /** Tip ids already queued or shown this session. @type {Set<string>} */
    this.tipIds = new Set();
  }

  show() {
    this.root.hidden = false;
  }

  /**
   * The draft strip and the inventory bar, rebuilt only when the hand or the offer changes; the timer and
   * recharge overlays update in place.
   * @param {NonNullable<import('./net/transport.js').PollReply['you']>['hand'] | null} hand
   * @param {number} tick server tick now (fractional)
   * @param {(code: number) => void} act
   */
  cards(hand, tick, act) {
    const offer = hand?.offers[0] ?? null;
    const draft = $('draft');
    const offerSig = offer ? offer.cards.join(',') + '@' + offer.until : '';
    if (offerSig !== this.offerSig) {
      this.offerSig = offerSig;
      draft.hidden = !offer;
      const box = $('draft-cards');
      box.replaceChildren(
        ...(offer?.cards ?? []).map((id, i) => {
          const c = CARDS[id];
          const b = document.createElement('button');
          b.type = 'button';
          b.tabIndex = -1; // the keyboard stays with the game: 1/2/3 pick
          b.className = `card-btn ${c?.rarity ?? ''}`;
          const kind =
            c?.kind === 'item' ? `Item · ${firstKey('item0')}/${firstKey('item1')}` : c?.kind === 'active' ? `Ability · ${firstKey('ability')}` : 'Passive';
          const top = document.createElement('div');
          top.className = 'top';
          const num = document.createElement('span');
          num.className = 'num';
          num.textContent = String(i + 1);
          const k = document.createElement('span');
          k.className = 'kind';
          k.textContent = kind;
          top.append(num, k);
          const name = document.createElement('div');
          name.className = 'name';
          name.textContent = c?.name ?? id;
          const rar = document.createElement('div');
          rar.className = 'rar';
          rar.textContent = c?.rarity ?? '';
          const text = document.createElement('div');
          text.className = 'text';
          text.textContent = c?.text ?? '';
          b.append(top, name, rar, text);
          b.addEventListener('mousedown', (e) => e.preventDefault());
          b.addEventListener('click', () => act(ACT.choose0 - i));
          return b;
        }),
      );
    }
    if (offer) {
      const left = Math.max(0, Math.min(1, (offer.until - tick) / OFFER_TICKS));
      $('draft-bar').style.transform = `scaleX(${left})`;
    }

    const items = hand?.items ?? [];
    const active = hand?.active ?? null;
    const passives = hand?.passives ?? [];
    const invSig = JSON.stringify([items, active?.slice(0, 2), passives]);
    const inv = $('inv');
    if (invSig !== this.invSig) {
      this.invSig = invSig;
      /** @param {string} key @param {[string, number] | null} card @param {number} code @param {string} cls */
      const slot = (key, card, code, cls) => {
        const b = document.createElement('button');
        b.type = 'button';
        b.tabIndex = -1;
        const c = card ? CARDS[card[0]] : null;
        b.className = `slot ${cls} ${c ? c.rarity : 'empty'}`;
        const k = document.createElement('span');
        k.className = 'key';
        k.textContent = key;
        const nm = document.createElement('span');
        nm.className = 'nm';
        nm.textContent = c ? c.name + (card && card[1] > 1 ? ' II' : '') : '';
        b.append(k, nm);
        if (c) {
          b.title = c.text;
          b.addEventListener('mousedown', (e) => e.preventDefault());
          b.addEventListener('click', () => act(code));
        }
        return b;
      };
      const kids = [
        slot(firstKey('item0'), items[0] ?? null, ACT.item0, ''),
        slot(firstKey('item1'), items[1] ?? null, ACT.item1, ''),
        slot(firstKey('ability'), active ? [active[0], active[1]] : null, ACT.active, 'ability'),
      ];
      for (const p of passives) {
        const c = CARDS[p[0]];
        const chip = document.createElement('span');
        chip.className = `slot passive ${c?.rarity ?? ''}`;
        chip.textContent = (c?.name ?? p[0]) + (p[1] > 1 ? ' II' : '');
        chip.title = c?.text ?? '';
        kids.push(/** @type {any} */ (chip));
      }
      inv.replaceChildren(...kids);
      inv.hidden = !hand;
    }
    // The ability's recharge, shown as a shade that shrinks, with the seconds left beside it.
    const ab = /** @type {HTMLElement | null} */ (inv.querySelector('.ability'));
    if (ab && active) {
      const def = CARDS[active[0]];
      const total = def?.cooldown ? def.cooldown(active[1]) : 1;
      const left = Math.max(0, Math.min(1, (active[2] - tick) / total));
      let cd = /** @type {HTMLElement | null} */ (ab.querySelector('.cd'));
      if (!cd) {
        cd = document.createElement('span');
        cd.className = 'cd';
        ab.prepend(cd);
      }
      cd.style.transform = `scaleY(${left})`;

      const ticksLeft = active[2] - tick;
      const charging = ticksLeft > 0;
      const prev = this.abState;
      // Only a real finish: the same card was recharging and now is not (a new card is ready from the start).
      if (prev && prev.id === active[0] && prev.charging && !charging) pulse(ab, 'ready', READY_MS);
      this.abState = { id: active[0], charging };
      let text = /** @type {HTMLElement | null} */ (ab.querySelector('.cd-text'));
      if (!text) {
        text = document.createElement('span');
        text.className = 'cd-text';
        ab.append(text);
      }
      const want = charging ? secs(ticksLeft / this.hz) : '';
      if (text.textContent !== want) text.textContent = want;
    } else this.abState = null;
  }

  /** Land gained (or lost, negative) by the local player just now, in tiles. @param {number} n @param {number} tiles total tiles */
  landDelta(n, tiles) {
    const p = (100 * n) / Math.max(1, tiles);
    if (Math.abs(p) < LAND_EPS) return;
    const el = $('st-land-delta');
    // U+2212 for the minus sign, so it reads as a sign and not a hyphen.
    el.textContent = `${p > 0 ? '+' : '−'}${Math.abs(p).toFixed(1)}%`;
    pulse(el, 'show', DELTA_MS);
  }

  /** A card action that did nothing (recharging, EMP'd, nothing to claim). @param {number} code ACT code */
  denied(code) {
    const inv = $('inv');
    /** @type {Element | null} */
    let el = null;
    if (code === ACT.item0) el = inv.children[0];
    else if (code === ACT.item1) el = inv.children[1];
    else if (code === ACT.active) el = inv.querySelector('.ability');
    if (el) pulse(/** @type {HTMLElement} */ (el), 'denied', DENIED_MS);
  }

  /** A short message near the top. @param {string} text @param {'info' | 'good' | 'bad'} [kind] */
  toast(text, kind = 'info') {
    const list = $('toasts');
    const li = document.createElement('li');
    li.className = `toast toast-${kind}`;
    li.textContent = text;
    list.append(li);
    while (list.children.length > TOAST_MAX) list.firstElementChild?.remove();
    setTimeout(() => li.classList.add('leaving'), TOAST_MS - TOAST_FADE_MS);
    setTimeout(() => li.remove(), TOAST_MS);
  }

  /**
   * A one-time onboarding tip (shown once per browser, by id). Tips show one at a time, each for a few seconds,
   * in the order they were asked for.
   * @param {string} id @param {string} text
   */
  tip(id, text) {
    if (this.tipIds.has(id)) return;
    this.tipIds.add(id);
    this.tipQueue.push({ id, text });
    this.nextTip();
  }

  /** Show the next tip not seen before, if none is up. */
  nextTip() {
    if (this.tipShowing) return;
    let item = this.tipQueue.shift();
    while (item && tipSeen(`landio.tip.${item.id}`)) item = this.tipQueue.shift();
    if (!item) return;
    const el = $('tip');
    this.tipShowing = true;
    markTipSeen(`landio.tip.${item.id}`);
    el.textContent = item.text;
    el.hidden = false;
    setTimeout(() => {
      el.hidden = true;
      el.textContent = '';
      this.tipShowing = false;
      this.nextTip();
    }, TIP_MS);
  }

  /** @param {boolean} down */
  netDown(down) {
    $('net').hidden = !down;
  }

  /** @param {string} name */
  showStart(name) {
    $('start-who').textContent = `Playing as ${name}`;
    $('start').hidden = false;
    $('summary').hidden = true;
    $('play').focus();
  }

  /** Say why Play did not work (on both the start and summary cards); '' clears it. @param {string} text */
  playMessage(text) {
    for (const el of document.querySelectorAll('.play-msg')) {
      el.textContent = text;
      /** @type {HTMLElement} */ (el).hidden = !text;
    }
  }

  hideOverlays() {
    $('start').hidden = true;
    $('summary').hidden = true;
    $('boot').hidden = true;
    this.hideResults();
  }

  /**
   * @param {import('../shared/types.js').DeathInfo} d @param {number} tiles @param {number} hz
   * @param {{killer: {name: string, look: number} | null, cards: string[]}} [extra] who got you (null for none)
   *   and the card ids you held at the end
   */
  showSummary(d, tiles, hz, extra) {
    $('sum-title').textContent =
      d.cause === 'removed' ? 'You left' : d.cause === 'border' || d.cause === 'wall' || d.cause === 'self' ? 'You crashed' : 'You were caught';
    // In Shrinking Arena (and Last Land Standing's sudden death) a wall death is often the border closing in.
    const md = this.world?.round?.md;
    const closed = d.cause === 'wall' && Array.isArray(md?.zone) && Number(md?.k) > 0;
    $('sum-cause').textContent = closed ? 'The border closed in on you.' : (CAUSES[d.cause] ?? (() => ''))(d.killer);

    const killerEl = $('sum-killer');
    const killer = extra?.killer ?? null;
    if (killer) {
      const sw = document.createElement('span');
      sw.className = 'sw';
      sw.style.background = lookColor(killer.look);
      killerEl.replaceChildren(sw, document.createTextNode(` Caught by ${killer.name}`));
    } else killerEl.replaceChildren();
    killerEl.hidden = !killer;

    const ids = extra?.cards ?? [];
    $('sum-cards').replaceChildren(
      ...ids.map((id) => {
        const c = CARDS[id];
        const li = document.createElement('li');
        li.className = `card-chip ${c?.rarity ?? ''}`;
        li.textContent = c?.name ?? id;
        return li;
      }),
    );
    $('sum-cards').hidden = ids.length === 0;

    $('sum-land').textContent = pct(d.land, tiles);
    $('sum-max').textContent = pct(d.maxLand, tiles);
    $('sum-kills').textContent = String(d.kills);
    $('sum-time').textContent = clock(d.ticksAlive, hz);
    $('summary').hidden = false;
    $('again').focus();
  }

  /**
   * Leaderboard (top 5 by land, plus the local player's own row when they are further down) and own stats.
   * @param {World} world @param {number} me @param {number} aliveTicks @param {number} hz
   */
  update(world, me, aliveTicks, hz) {
    this.world = world;
    this.hz = hz;
    const tiles = world.w * world.h;
    // Living players by the mode's score, ties by id: the same order the rows and the local player's rank use.
    const ranked = [...world.stats.entries()]
      .filter(([, s]) => s.alive)
      .sort((a, b) => b[1].score - a[1].score || a[0] - b[0]);
    const mode = MODE_BY_ID[world.round?.mode ?? 'classic'];
    const points = mode?.scoreLabel === 'points';
    const show = (/** @type {number} */ v) => (points ? String(v) : pct(v, tiles));
    this.teamTotals(world, me, mode?.teams ?? 0, show);
    const rows = ranked.slice(0, LB_ROWS);
    const myIndex = ranked.findIndex(([id]) => id === me);
    const extraRank = myIndex >= LB_ROWS ? myIndex + 1 : 0;
    const mine = world.stats.get(me);

    // Rebuild the rows only when who is listed changes; land numbers change constantly and are updated in place.
    const sig = rows.map(([id]) => `${id}:${world.roster.get(id)?.name}`).join('|') + `#${me}` + (extraRank ? '+' : '');
    if (sig !== this.lbSig) {
      this.lbSig = sig;
      /** @type {HTMLElement[]} */
      const cells = [];
      /** @type {HTMLElement[]} */
      const lis = rows.map(([id]) => {
        const info = world.roster.get(id);
        const li = document.createElement('li');
        if (id === me) li.className = 'me';
        const sw = document.createElement('span');
        sw.className = 'sw';
        sw.style.background = lookColor(info?.look ?? id);
        const nm = document.createElement('span');
        nm.className = 'nm';
        nm.textContent = info?.name ?? '?';
        if (info?.bot) {
          const b = document.createElement('span');
          b.className = 'bot';
          b.textContent = ' bot';
          nm.append(b);
        }
        const pc = document.createElement('span');
        pc.className = 'pc';
        cells.push(pc);
        li.append(sw, nm, pc);
        return li;
      });
      this.lbCells = cells;
      this.lbExtra = null;
      if (extraRank) {
        const li = document.createElement('li');
        li.className = 'lb-me-extra';
        const name = document.createElement('span');
        name.className = 'nm';
        const pc = document.createElement('span');
        pc.className = 'pc';
        li.append(name, pc);
        lis.push(li);
        this.lbExtra = { li, name, pc };
      }
      this.lb.replaceChildren(...lis);
    }
    rows.forEach(([, s], k) => {
      const cell = this.lbCells[k];
      const text = show(s.score);
      if (cell && cell.textContent !== text) cell.textContent = text;
    });
    if (this.lbExtra && mine) {
      const text = show(mine.score);
      const label = `#${extraRank} you`;
      if (this.lbExtra.pc.textContent !== text) this.lbExtra.pc.textContent = text;
      if (this.lbExtra.name.textContent !== label) this.lbExtra.name.textContent = label;
    }

    const st = `${mine?.land ?? 0}:${mine?.kills ?? 0}:${clock(aliveTicks, hz)}`;
    if (st !== this.statSig) {
      this.statSig = st;
      $('st-land').textContent = pct(mine?.land ?? 0, tiles);
      $('st-kills').textContent = String(mine?.kills ?? 0);
      $('st-time').textContent = clock(aliveTicks, hz);
    }
  }

  /**
   * Team totals above the leaderboard (team modes only). @param {World} world @param {number} me @param {number} teams
   * @param {(v: number) => string} show
   */
  teamTotals(world, me, teams, show) {
    const box = $('lb-teams');
    if (!teams) {
      if (!box.hidden) {
        box.hidden = true;
        box.replaceChildren();
        this.teamSig = '';
      }
      return;
    }
    /** @type {number[]} */
    const totals = Array(teams + 1).fill(0);
    for (const [id, st] of world.stats) {
      const t = world.roster.get(id)?.team ?? 0;
      if (t > 0 && t <= teams) totals[t] += st.score;
    }
    const myTeam = world.roster.get(me)?.team ?? 0;
    // Only teams that have someone (a mode may allow up to 4 but play with fewer).
    const present = new Set();
    for (const id of world.stats.keys()) present.add(world.roster.get(id)?.team ?? 0);
    const order = [...Array(teams).keys()]
      .map((k) => k + 1)
      .filter((t) => present.has(t))
      .sort((a, b) => totals[b] - totals[a] || a - b);
    const sig = order.map((t) => `${t}:${show(totals[t])}`).join('|') + '#' + myTeam;
    if (sig === this.teamSig) return;
    this.teamSig = sig;
    box.hidden = false;
    box.replaceChildren(
      ...order.map((t) => {
        const row = document.createElement('div');
        row.className = 'tm' + (t === myTeam ? ' mine' : '');
        const sw = document.createElement('span');
        sw.className = 'sw';
        sw.style.background = lookColor(TEAMS[t - 1].color);
        const nm = document.createElement('span');
        nm.className = 'nm';
        nm.textContent = TEAMS[t - 1].name;
        const pc = document.createElement('span');
        pc.className = 'pc';
        pc.textContent = show(totals[t]);
        row.append(sw, nm, pc);
        return row;
      }),
    );
  }

  /**
   * The round chip: the mode, the time left (or the mode's own note), urgent in the last 30 s.
   * @param {World} world @param {number} me @param {number} hz @param {number} tick the server tick now
   */
  roundChip(world, me, hz, tick) {
    const r = world.round;
    const chip = $('round');
    if (!r) {
      chip.hidden = true;
      return;
    }
    const mode = MODE_BY_ID[r.mode];
    const parts = [];
    let urgent = false;
    if (r.phase === 'over') parts.push('Round over');
    else if (r.end) {
      const left = Math.max(0, r.end - tick);
      parts.push(clock(left, hz));
      urgent = left / hz <= 30;
    }
    const extra = MODE_VIEWS[r.mode]?.chip?.(world, me, hz) ?? '';
    if (extra) parts.push(extra);
    // Classic with nothing to say: no chip at all.
    if (!parts.length && r.mode === 'classic') {
      chip.hidden = true;
      return;
    }
    const name = mode?.name ?? r.mode;
    const info = parts.join(' · ');
    if ($('round-mode').textContent !== name) $('round-mode').textContent = name;
    if ($('round-info').textContent !== info) $('round-info').textContent = info;
    chip.classList.toggle('urgent', urgent);
    chip.hidden = false;
  }

  /**
   * The room: "Room ABCDEF" on the cards, and "Room ABCDEF · Ann, Bob" in the people line (who is in this room,
   * from the server). @param {World} world
   */
  roomLine(world) {
    const g = world.game;
    const code = g?.code ?? '';
    const cards = code ? `Room ${code}` : '';
    for (const el of document.querySelectorAll('.room-now')) if (el.textContent !== cards) el.textContent = cards;
    const line = code ? `Room ${code}${g?.people?.length ? ` · ${g.people.join(', ')}` : ''}` : '';
    if ($('people').textContent !== line) $('people').textContent = line;
  }

  /** "Timed Rounds on Islands" on every card. @param {World} world */
  modeLine(world) {
    const r = world.round;
    const text = r ? `${MODE_BY_ID[r.mode]?.name ?? r.mode} on ${MAP_BY_ID[r.map]?.name ?? r.map}` : '';
    for (const el of document.querySelectorAll('.mode-now')) if (el.textContent !== text) el.textContent = text;
  }

  /**
   * The results between rounds. @param {World} world @param {number} me @param {number} hz @param {number} tick
   */
  showResults(world, me, hz, tick) {
    const r = world.round;
    const res = r?.result;
    if (!r || !res) return;
    const mode = MODE_BY_ID[r.mode];
    const tiles = world.w * world.h;
    const show = (/** @type {number} */ v) => (mode?.scoreLabel === 'points' ? `${v} pts` : pct(v, tiles));
    const sig = `${r.n}`;
    if (this.resSig !== sig) {
      this.resSig = sig;
      let title = 'Round over';
      if (res.winner < 0) title = `${TEAMS[-res.winner - 1]?.name ?? 'A'} team wins`;
      else if (res.winner > 0) {
        const w = res.players.find((p) => p.id === res.winner);
        title = w ? (w.id === me ? 'You win!' : `${w.name} wins`) : title;
      }
      $('res-title').textContent = title;
      const teamsEl = $('res-teams');
      teamsEl.hidden = !res.teams.length;
      teamsEl.replaceChildren(
        ...res.teams.map((t, k) => {
          const li = document.createElement('li');
          if (k === 0) li.className = 'first';
          li.append(span('rk', String(k + 1)), swatch(lookColor(TEAMS[t.team - 1]?.color ?? 0)), span('nm', `${TEAMS[t.team - 1]?.name ?? '?'} team`), span('pc', show(t.score)));
          return li;
        }),
      );
      const top = res.players.slice(0, 5);
      $('res-list').replaceChildren(
        ...top.map((p, k) => {
          const li = document.createElement('li');
          li.className = (k === 0 && !res.teams.length ? 'first ' : '') + (p.id === me ? 'me' : '');
          li.append(span('rk', String(k + 1)), swatch(lookColor(p.look)), span('nm', p.name + (p.bot ? ' (bot)' : '')), span('pc', show(p.score)));
          return li;
        }),
      );
      const mineAt = res.players.findIndex((p) => p.id === me);
      $('res-you').textContent = mineAt >= 0 ? `You placed #${mineAt + 1} of ${res.players.length} with ${show(res.players[mineAt].score)}.` : '';
      $('start').hidden = true;
      $('summary').hidden = true;
      $('results').hidden = false;
    }
    const left = Math.max(0, Math.ceil((r.next - tick) / hz));
    const next = `Next round in ${left} s`;
    if ($('res-next').textContent !== next) $('res-next').textContent = next;
  }

  hideResults() {
    $('results').hidden = true;
    this.resSig = '';
  }

  /** The keys were rebound: redraw what names them. */
  keysChanged() {
    this.invSig = '';
    this.offerSig = '';
    $('hint').textContent = hintText();
  }

  /** A kill event, as it plays out. @param {any} e @param {number} me */
  kill(e, me) {
    if (e.c === 'removed') return;
    const li = document.createElement('li');
    if (e.id === me || e.v === me) li.className = 'mine';
    /** @param {string} t */
    const b = (t) => {
      const el = document.createElement('b');
      el.textContent = t;
      return el;
    };
    /** A name with its player's colour swatch before it. @param {number} id @param {string} t */
    const who = (id, t) => {
      const sw = document.createElement('span');
      sw.className = 'sw';
      sw.style.background = lookColor(this.world?.roster.get(id)?.look ?? id);
      return [sw, b(t)];
    };
    const victim = e.v === me ? 'you' : (e.vn ?? '?');
    const killer = e.id === me ? 'You' : (e.name ?? '?');
    if (e.c === 'self') li.append(...who(e.v, victim), ' crossed their own trail');
    else if (e.c === 'border') li.append(...who(e.v, victim), ' left the map');
    else if (e.c === 'head') li.append(...who(e.id, killer), ' and ', ...who(e.v, victim), ' crashed');
    else if (e.c === 'enclosed') li.append(...who(e.id, killer), ' enclosed ', ...who(e.v, victim));
    else if (e.c === 'landless') li.append(...who(e.id, killer), ' took all of ', ...who(e.v, victim), '’s land');
    else li.append(...who(e.id, killer), ' cut ', ...who(e.v, victim));
    if (e.n > 0) li.append(` · +${e.n} land`);
    this.feed.prepend(li);
    while (this.feed.children.length > FEED_MAX) this.feed.lastElementChild?.remove();
    setTimeout(() => li.classList.add('old'), FEED_MS);
    setTimeout(() => li.remove(), FEED_MS + 700);
  }

  /**
   * Minimap: one pixel per tile, heads as dots, the view as a box.
   * @param {World} world @param {number} me @param {number} r @param {{x0: number, y0: number, x1: number, y1: number} | undefined} view
   */
  minimap(world, me, r, view) {
    const { w, h } = world;
    if (!w) return;
    if (!this.mmImg || this.mmImg.width !== w || this.mmImg.height !== h) {
      this.mmImg = new ImageData(w, h);
      this.mmCanvas.width = w;
      this.mmCanvas.height = h;
    }
    // The land picture changes only when tiles do: rebuild it then; heads and the view box are drawn every time.
    if (this.mmVer !== world.tilesVer) {
      this.mmVer = world.tilesVer;
      this.paintMinimapLand(world);
    }
    this.drawMinimap(world, me, r, view);
  }

  /** @param {World} world */
  paintMinimapLand(world) {
    const { w, h } = world;
    const img = /** @type {ImageData} */ (this.mmImg);
    const px = img.data;
    /** @type {Map<number, number[]>} */
    const rgb = new Map();
    const colorOf = (/** @type {number} */ id) => {
      let c = rgb.get(id);
      if (!c) {
        const n = parseInt(lookColor(world.roster.get(id)?.look ?? id).slice(1), 16);
        c = [n >> 16, (n >> 8) & 255, n & 255];
        rgb.set(id, c);
      }
      return c;
    };
    for (let i = 0; i < w * h; i++) {
      const b = world.base[i];
      const t = world.trail[i];
      const o = i * 4;
      const ter = world.terrain[i];
      if (ter === 1) {
        px[o] = 6;
        px[o + 1] = 8;
        px[o + 2] = 14;
      } else if (b || t) {
        const c = colorOf(b || t);
        const f = b ? 1 : 0.6;
        px[o] = c[0] * f;
        px[o + 1] = c[1] * f;
        px[o + 2] = c[2] * f;
      } else if (ter === 2) {
        px[o] = 90;
        px[o + 1] = 72;
        px[o + 2] = 22;
      } else {
        px[o] = 18;
        px[o + 1] = 24;
        px[o + 2] = 40;
      }
      px[o + 3] = 255;
    }
    const g = /** @type {CanvasRenderingContext2D} */ (this.mmCanvas.getContext('2d'));
    g.putImageData(img, 0, 0);
  }

  /**
   * @param {World} world @param {number} me @param {number} r @param {{x0: number, y0: number, x1: number, y1: number} | undefined} view
   */
  drawMinimap(world, me, r, view) {
    const { w, h } = world;
    const ctx = this.mmCtx;
    const S = this.mm.width;
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(this.mmCanvas, 0, 0, S, S * (h / w));
    const k = S / w;
    for (const id of world.headIds(r)) {
      const hd = world.headAt(id, r);
      if (!hd) continue;
      ctx.fillStyle = id === me ? '#ffffff' : '#111111';
      const s = id === me ? 6 : 4;
      ctx.fillRect(hd.x * k - s / 2 + k / 2, hd.y * k - s / 2 + k / 2, s, s);
    }
    if (view) {
      ctx.strokeStyle = 'rgba(255,255,255,0.8)';
      ctx.lineWidth = 1;
      ctx.strokeRect(view.x0 * k, view.y0 * k, (view.x1 - view.x0 + 1) * k, (view.y1 - view.y0 + 1) * k);
    }
  }
}
