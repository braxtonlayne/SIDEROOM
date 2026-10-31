// The table on screen. Plain DOM, no framework: every update rebuilds the pieces that changed. Text always goes
// through `textContent` (player names are other people's input).

import type { Card } from './cards';
import { parseCard, SUIT_SYMBOL } from './cards';
import type { PlayerClient } from './client';
import type { HostController } from './host';
import type { PublicHand, PublicTable } from './protocol';
import { S } from './strings';

type Child = Node | string | null | undefined | false;

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, string> = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    node.append(typeof c === 'string' ? document.createTextNode(c) : c);
  }
  return node;
}

function cardEl(card: Card | null, extra = ''): HTMLElement {
  if (!card) return el('span', { class: `card back ${extra}`, 'aria-label': 'Face-down card' });
  const { suit } = parseCard(card);
  const rank = card.charAt(0) === 'T' ? '10' : card.charAt(0);
  const red = suit === 'h' || suit === 'd';
  return el(
    'span',
    { class: `card ${red ? 'red' : 'black'} ${extra}`, 'data-card': card, 'aria-label': `${rank} of ${suit}` },
    el('span', { class: 'rank' }, rank),
    el('span', { class: 'suit' }, SUIT_SYMBOL[suit]),
  );
}

export interface MountOptions {
  client: PlayerClient;
  /** Present on the host's instance only. */
  host: HostController | null;
  defaultName: string;
  /** Milliseconds before the host deals the next hand by itself (when ticked). */
  autoDealMs?: number;
}

export function mountTable(root: HTMLElement, opts: MountOptions): () => void {
  const { client, host } = opts;
  let raiseValue = 0;
  let autoDeal = true;
  let nameDraft = opts.defaultName;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let timerHand = -1;

  const render = () => {
    const snap = client.snapshot();
    const t = snap.table;
    const focusId = (document.activeElement as HTMLElement | null)?.id ?? '';
    root.replaceChildren(
      el(
        'div',
        { class: 'holdem' },
        header(),
        warning(),
        t ? felt(t, snap.hole) : el('p', { class: 'muted' }, S.waitingHost),
        t ? status(t) : null,
        t ? controls(t, snap.hole) : null,
        t ? lobby(t) : null,
        snap.error ? el('p', { class: 'error', role: 'alert' }, snap.error) : null,
        t ? logPanel(t) : null,
      ),
    );
    if (focusId) (root.querySelector(`#${focusId}`) as HTMLElement | null)?.focus();
    scheduleAutoDeal(t);
  };

  const scheduleAutoDeal = (t: PublicTable | null) => {
    if (!host || !t) return;
    const over = !t.hand || t.hand.over;
    if (!over || !autoDeal || !t.canStart || !t.hand) {
      if (!over && timer) {
        clearTimeout(timer);
        timer = null;
      }
      return;
    }
    if (timer && timerHand === t.hand.no) return;
    if (timer) clearTimeout(timer);
    timerHand = t.hand.no;
    timer = setTimeout(() => {
      timer = null;
      host.deal();
    }, opts.autoDealMs ?? 7000);
  };

  // ---- pieces ----

  function header(): HTMLElement {
    return el(
      'header',
      { class: 'bar' },
      el('h1', {}, S.title),
      el('span', { class: 'pill' }, S.playMoney),
      host ? el('span', { class: 'pill host' }, S.hostBadge) : null,
    );
  }

  function warning(): HTMLElement {
    return el(
      'aside',
      { class: 'warn', role: 'note', 'data-testid': 'holdem-warning' },
      el('strong', {}, `${S.notCheatProofTitle}. `),
      S.notCheatProof,
    );
  }

  function felt(t: PublicTable, hole: [Card, Card] | null): HTMLElement {
    const hand = t.hand;
    const me = client.self;
    const mySeat = t.seats.findIndex((s) => s?.id === me);
    const n = t.config.maxSeats;
    const board = el('div', { class: 'board', 'data-testid': 'holdem-board' });
    for (let i = 0; i < 5; i++) board.append(cardEl(hand?.board[i] ?? null, hand?.board[i] ? '' : 'slot'));
    const center = el(
      'div',
      { class: 'center' },
      board,
      el(
        'div',
        { class: 'potline' },
        hand ? `${S.potSize} ${hand.pot}` : '',
        hand ? ` · ${S.handNumber(hand.no)} · ${S.street[hand.street]}` : '',
      ),
    );
    const f = el('div', { class: 'felt' }, center);
    for (let i = 0; i < n; i++) {
      // Rotate so my seat is at the bottom of the table.
      const rel = (i - (mySeat < 0 ? 0 : mySeat) + n) % n;
      const ang = Math.PI / 2 + (rel * 2 * Math.PI) / n;
      const x = 50 + 44 * Math.cos(ang);
      const y = 50 + 40 * Math.sin(ang);
      const s = seatEl(t, i, hole);
      s.style.insetInlineStart = `${x}%`;
      s.style.insetBlockStart = `${y}%`;
      f.append(s);
    }
    return f;
  }

  function seatEl(t: PublicTable, i: number, hole: [Card, Card] | null): HTMLElement {
    const seat = t.seats[i];
    const hand = t.hand;
    if (!seat) return el('div', { class: 'seat empty', 'aria-label': S.emptySeat }, '');
    const p = hand?.players.find((x) => x.id === seat.id);
    const mine = seat.id === client.self;
    const reveal = hand?.result?.reveals[seat.id];
    const cls = ['seat'];
    if (mine) cls.push('mine');
    if (p?.folded) cls.push('folded');
    if (hand && !hand.over && hand.toAct === seat.id) cls.push('acting');
    const tags: Child[] = [];
    if (hand && hand.button === i) tags.push(el('span', { class: 'tag dealer' }, S.dealer));
    if (hand && hand.smallBlindSeat === i) tags.push(el('span', { class: 'tag' }, S.smallBlind));
    if (hand && hand.bigBlindSeat === i) tags.push(el('span', { class: 'tag' }, S.bigBlind));
    if (p?.allIn && !p.folded) tags.push(el('span', { class: 'tag allin' }, S.allInTag));
    if (p?.folded) tags.push(el('span', { class: 'tag' }, S.foldedTag));
    if (seat.leaving) tags.push(el('span', { class: 'tag' }, S.leavingTag));

    const cards = el('div', { class: 'hole' });
    if (p && !p.folded) {
      if (reveal) for (const c of reveal.cards) cards.append(cardEl(c, 'small'));
      else if (mine && hole && hand && !hand.over) for (const c of hole) cards.append(cardEl(c, 'small'));
      else {
        cards.append(cardEl(null, 'small'));
        cards.append(cardEl(null, 'small'));
      }
    }
    return el(
      'div',
      { class: cls.join(' '), 'data-seat': String(i) },
      el('div', { class: 'name' }, seat.name + (mine ? ` (${S.you})` : '')),
      el('div', { class: 'chips' }, String(p ? p.stack : seat.chips)),
      cards,
      reveal ? el('div', { class: 'label' }, reveal.label) : null,
      p && p.bet > 0 ? el('div', { class: 'bet' }, String(p.bet)) : null,
      tags.length ? el('div', { class: 'tags' }, ...tags) : null,
    );
  }

  function resultText(h: PublicHand): string {
    const r = h.result;
    if (!r) return '';
    const parts: string[] = [];
    for (const [id, won] of Object.entries(r.payouts)) {
      const name = h.players.find((p) => p.id === id)?.name ?? id;
      // A bet handed back to its owner is not a win worth announcing on its own.
      const contested = r.awards.some((a) => a.winners.includes(id) && a.eligible.length > 1);
      if (!r.uncontested && !contested) continue;
      parts.push(
        r.uncontested
          ? S.winsUncontested(name, won)
          : S.winsPot(name, won, r.reveals[id]?.label ?? ''),
      );
    }
    return parts.join('. ');
  }

  function status(t: PublicTable): HTMLElement {
    const h = t.hand;
    let text = '';
    if (!h) text = S.waitingHost;
    else if (h.over) text = resultText(h);
    else if (h.toAct === client.self) text = S.yourTurn;
    else {
      const name = h.players.find((p) => p.id === h.toAct)?.name;
      text = name ? S.waitingFor(name) : '';
    }
    return el('p', { class: 'status', role: 'status', 'aria-live': 'polite', 'data-testid': 'holdem-status' }, text);
  }

  function controls(t: PublicTable, hole: [Card, Card] | null): HTMLElement | null {
    const h = t.hand;
    const me = h?.players.find((p) => p.id === client.self);
    if (!h || !me) return null;
    const wrap = el('section', { class: 'mine-panel', 'aria-label': S.yourCards });
    const cards = el('div', { class: 'mycards', 'data-testid': 'holdem-hole' });
    if (hole && client.myCards()) for (const c of hole) cards.append(cardEl(c));
    wrap.append(cards);
    const legal = h.legal;
    if (!h.over && h.toAct === client.self && legal) {
      const row = el('div', { class: 'actions' });
      row.append(
        button(S.fold, 'holdem-fold', () => client.act({ type: 'fold' }), 'danger'),
        legal.canCheck
          ? button(S.check, 'holdem-check-call', () => client.act({ type: 'check' }))
          : button(S.call(legal.toCall), 'holdem-check-call', () => client.act({ type: 'call' })),
      );
      if (legal.canRaise) {
        if (raiseValue < legal.minRaiseTo || raiseValue > legal.maxRaiseTo) raiseValue = legal.minRaiseTo;
        const input = el('input', {
          id: 'raise-input',
          type: 'number',
          min: String(legal.minRaiseTo),
          max: String(legal.maxRaiseTo),
          step: '1',
          value: String(raiseValue),
          'data-testid': 'holdem-raise-input',
          'aria-label': S.raise,
        });
        input.addEventListener('input', () => {
          raiseValue = Number(input.value);
        });
        const clamp = (n: number) => Math.max(legal.minRaiseTo, Math.min(legal.maxRaiseTo, Math.round(n)));
        const set = (n: number) => {
          raiseValue = clamp(n);
          input.value = String(raiseValue);
        };
        const quick = el(
          'div',
          { class: 'quick' },
          button(S.minRaise, 'holdem-q-min', () => set(legal.minRaiseTo), 'ghost'),
          button(S.halfPot, 'holdem-q-half', () => set(h.currentBet + (h.pot + legal.toCall) / 2), 'ghost'),
          button(S.pot, 'holdem-q-pot', () => set(h.currentBet + h.pot + legal.toCall), 'ghost'),
          button(S.allIn, 'holdem-q-allin', () => set(legal.maxRaiseTo), 'ghost'),
        );
        row.append(
          input,
          button(h.currentBet === 0 ? S.bet : S.raise, 'holdem-raise', () => {
            const to = clamp(Number(input.value));
            client.act(to >= legal.maxRaiseTo ? { type: 'allin' } : { type: 'raise', to });
          }),
        );
        wrap.append(row, quick);
        return wrap;
      }
      wrap.append(row);
    }
    return wrap;
  }

  function button(label: string, testId: string, onClick: () => void, cls = ''): HTMLButtonElement {
    const b = el('button', { type: 'button', class: `btn ${cls}`, 'data-testid': testId }, label);
    b.addEventListener('click', onClick);
    return b;
  }

  function lobby(t: PublicTable): HTMLElement {
    const seated = t.seats.find((s) => s?.id === client.self);
    const hand = t.hand;
    const inHand = !!hand && !hand.over && hand.players.some((p) => p.id === client.self);
    const box = el('section', { class: 'lobby' });
    if (!seated) {
      const input = el('input', {
        id: 'name-input',
        type: 'text',
        maxlength: '32',
        value: nameDraft,
        'aria-label': S.yourName,
        'data-testid': 'holdem-name',
      });
      input.addEventListener('input', () => {
        nameDraft = input.value;
      });
      const full = t.seats.every((s) => s !== null);
      const b = button(S.sitDown, 'holdem-sit', () => client.sit(nameDraft.trim() || opts.defaultName));
      b.disabled = full;
      box.append(input, b);
      if (full) box.append(el('span', { class: 'muted' }, S.seatsFull));
    } else {
      if (seated.chips === 0 && !inHand) box.append(button(S.rebuy, 'holdem-rebuy', () => client.rebuy()));
      box.append(button(S.standUp, 'holdem-stand', () => client.stand(), 'ghost'));
    }
    if (host) {
      const idle = !hand || hand.over;
      const deal = button(hand ? S.dealNext : S.deal, 'holdem-deal', () => host.deal());
      deal.disabled = !(idle && t.canStart);
      const auto = el('input', { id: 'auto-deal', type: 'checkbox', 'data-testid': 'holdem-auto' });
      auto.checked = autoDeal;
      auto.addEventListener('change', () => {
        autoDeal = auto.checked;
        if (!autoDeal && timer) {
          clearTimeout(timer);
          timer = null;
        }
      });
      const small = el('input', {
        id: 'sb-input',
        type: 'number',
        min: '1',
        value: String(t.config.smallBlind),
        'aria-label': S.small,
      });
      const big = el('input', {
        id: 'bb-input',
        type: 'number',
        min: '1',
        value: String(t.config.bigBlind),
        'aria-label': S.big,
      });
      const setB = button(S.setBlinds, 'holdem-blinds', () => host.setBlinds(Number(small.value), Number(big.value)), 'ghost');
      setB.disabled = !idle;
      box.append(
        el(
          'div',
          { class: 'hostbar' },
          deal,
          el('label', {}, auto, ` ${S.autoDeal}`),
          el('span', { class: 'blinds' }, `${S.blinds}: `, small, ' / ', big, ' ', setB),
          idle && !t.canStart ? el('span', { class: 'muted' }, S.needTwo) : null,
        ),
      );
    }
    return box;
  }

  function logPanel(t: PublicTable): HTMLElement {
    const ol = el('ol', { class: 'log' });
    for (const line of t.log.slice(-12)) ol.append(el('li', {}, line));
    return el('details', { class: 'logbox' }, el('summary', {}, S.log), ol);
  }

  const off = client.subscribe(render);
  render();
  return () => {
    off();
    if (timer) clearTimeout(timer);
    root.replaceChildren();
  };
}
