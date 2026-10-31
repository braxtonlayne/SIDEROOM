// @ts-check
// The settings menu (D-55): Game (the arena's mode, map and rules, and the host's tools), Me (this device's
// sound and display) and Controls. Opened with the gear button or Esc; the game keeps running behind it.

import { MODES, MODE_BY_ID } from '../shared/modes/index.js';
import { MAPS } from '../shared/maps/index.js';
import { savePrefs } from './prefs.js';
import { normalizeCode } from '../shared/room-code.js';
import { ACTIONS, DEFAULT_BINDINGS, RESERVED_KEYS, applyBindings, bindings, keyLabel, rebind } from './input.js';

/** @typedef {import('./world.js').World} World */
/** @typedef {import('./net/transport.js').Transport} Transport */
/** @typedef {import('./net/transport.js').GameSettings} GameSettings */
/** @typedef {import('./prefs.js').Prefs} Prefs */

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id));
/** @param {string} id */
const input = (id) => /** @type {HTMLInputElement} */ (document.getElementById(id));
/** @param {string} id */
const select = (id) => /** @type {HTMLSelectElement} */ (document.getElementById(id));

const SIZES = [
  [60, 'Small'],
  [80, 'Cozy'],
  [100, 'Medium'],
  [150, 'Large'],
  [200, 'Huge'],
];
const ROUND_MINUTES = [0, 3, 5, 8, 12];
const SHIELD_SECONDS = [0, 3, 5];
/** Keys that cannot be rebound. */
const FIXED_KEYS = [
  ['M', 'Sound on or off'],
  ['Esc', 'This menu'],
  ['Enter', 'Play (on the start and summary cards)'],
];

/**
 * @param {string} name @param {{value: string, label: string}[]} options @param {string} current
 * @returns {HTMLElement[]}
 */
function cards(name, options, current) {
  return options.map((o) => {
    const label = document.createElement('label');
    const radio = document.createElement('input');
    radio.type = 'radio';
    radio.name = name;
    radio.value = o.value;
    radio.checked = o.value === current;
    const nm = document.createElement('span');
    nm.className = 'pk-name';
    nm.textContent = o.label;
    label.append(radio, nm);
    return label;
  });
}

export class Menu {
  /**
   * @param {{transport: Transport, prefs: Prefs, onPrefs: (p: Prefs) => void, audio: {volume: number, muted: boolean,
   *   setVolume(v: number): void, toggleMute(): boolean, play(name: string): void}, toast: (text: string) => void,
   *   onKeys: () => void, onJoinRoom: (code: string) => Promise<string>, onBackToChannel: () => void}} opt
   *   onJoinRoom: switch to the room with that code; resolves '' or an error message
   */
  constructor(opt) {
    this.opt = opt;
    this.isOpen = false;
    this.tab = 'game';
    /** @type {World | null} */
    this.world = null;
    /** The server's settings the form was last filled from, and whether the player has changed the form since. */
    this.formSig = '';
    this.dirty = false;
    /** The person a first press of Remove picked (the second press removes them). */
    this.kickArmed = '';
    /** While a key button waits for a key: stops waiting. @type {(() => void) | null} */
    this.stopListening = null;
    /** The code a second Join press would confirm, and what the people list shows now. */
    this.confirmJoin = '';
    this.peopleSig = '';
    this.build();
  }

  /** Fill in the static parts and wire the controls (once). */
  build() {
    select('g-size').replaceChildren(
      ...SIZES.map(([n, label]) => new Option(`${label} (${n}×${n})`, String(n))),
    );
    select('g-round').replaceChildren(
      ...ROUND_MINUTES.map((m) => new Option(m ? `${m} minutes` : "The mode's own", String(m))),
    );
    select('g-shield').replaceChildren(
      ...SHIELD_SECONDS.map((sec) => new Option(sec ? `${sec} seconds` : 'Off', String(sec))),
    );
    this.renderKeys();
    $('keys-reset').addEventListener('click', () => {
      applyBindings(structuredClone(DEFAULT_BINDINGS));
      this.renderKeys();
      this.opt.onKeys();
      this.opt.toast('Keys reset');
    });
    $('keys-list').replaceChildren(
      ...FIXED_KEYS.flatMap(([k, what]) => {
        const dt = document.createElement('dt');
        const kbd = document.createElement('kbd');
        kbd.textContent = k;
        dt.append(kbd);
        const dd = document.createElement('dd');
        dd.textContent = what;
        return [dt, dd];
      }),
    );
    for (const b of document.querySelectorAll('#menu [data-tab]'))
      b.addEventListener('click', () => this.show(/** @type {HTMLElement} */ (b).dataset.tab ?? 'game'));
    $('menu-close').addEventListener('click', () => this.close());
    $('menu').addEventListener('click', (e) => {
      if (e.target === $('menu')) this.close();
    });
    // Room tab.
    $('r-copy').addEventListener('click', () => this.copy('r-code', 'Code copied'));
    $('r-copy-link').addEventListener('click', () => this.copy('r-link', 'Link copied'));
    for (const id of ['r-code', 'r-link']) $(id).addEventListener('focus', () => input(id).select());
    $('r-join-form').addEventListener('submit', (e) => {
      e.preventDefault();
      this.joinRoom();
    });
    $('r-join').addEventListener('input', () => {
      this.confirmJoin = '';
      $('r-join-go').textContent = 'Join';
      this.roomMessage('');
    });
    $('r-back').addEventListener('click', () => {
      this.close();
      this.opt.onBackToChannel();
    });
    // Game tab.
    const gameForm = $('panel-game');
    gameForm.addEventListener('input', () => {
      this.dirty = true;
      this.refreshApply();
    });
    input('g-bots').addEventListener('input', () => ($('g-bots-out').textContent = input('g-bots').value));
    $('g-apply').addEventListener('click', () => this.apply());
    $('g-restart').addEventListener('click', () => this.hostAction('restart'));
    $('g-end').addEventListener('click', () => this.hostAction('end'));
    $('g-give').addEventListener('click', () => this.hostAction('host'));
    $('g-kick-go').addEventListener('click', () => this.kick());
    // Me tab: every change applies at once.
    const p = this.opt.prefs;
    input('m-volume').addEventListener('input', () => this.opt.audio.setVolume(Number(input('m-volume').value) / 100));
    input('m-mute').addEventListener('change', () => {
      if (input('m-mute').checked !== this.opt.audio.muted) this.opt.audio.toggleMute();
    });
    /** @param {string} id @param {(el: HTMLInputElement & HTMLSelectElement) => void} set */
    const pref = (id, set) =>
      $(id).addEventListener('change', () => {
        set(/** @type {HTMLInputElement & HTMLSelectElement} */ ($(id)));
        savePrefs(p);
        this.opt.onPrefs(p);
      });
    pref('m-shake', (el) => (p.shake = el.checked));
    pref('m-names', (el) => (p.names = el.checked));
    pref('m-minimap', (el) => (p.minimap = el.checked));
    pref('m-motion', (el) => (p.motion = /** @type {Prefs['motion']} */ (el.value)));
    pref('m-quality', (el) => (p.quality = /** @type {Prefs['quality']} */ (el.value)));
  }

  /** The Controls tab's table: each action with its two key buttons. */
  renderKeys() {
    const cells = [];
    for (const a of ACTIONS) {
      const label = document.createElement('span');
      label.className = 'act';
      label.textContent = a.label;
      cells.push(label);
      for (const slot of /** @type {(0 | 1)[]} */ ([0, 1])) {
        const code = bindings[a.id]?.[slot] ?? '';
        const b = document.createElement('button');
        b.type = 'button';
        b.className = 'key-btn' + (code ? '' : ' empty');
        b.textContent = code ? keyLabel(code) : 'none';
        b.setAttribute('aria-label', `${a.label}, key ${slot + 1}: ${code ? keyLabel(code) : 'none'}. Change`);
        b.addEventListener('click', () => this.listen(b, a.id, slot));
        cells.push(b);
      }
    }
    $('keys-table').replaceChildren(...cells);
  }

  /**
   * Wait for the next key and bind it. Esc cancels, Backspace or Delete clears the slot; the game's own keys
   * (Esc, M, Enter, Tab) cannot be bound. @param {HTMLButtonElement} button @param {string} id @param {0 | 1} slot
   */
  listen(button, id, slot) {
    this.stopListening?.();
    button.classList.add('listening');
    button.textContent = 'Press a key…';
    /** @param {KeyboardEvent} e */
    const onKey = (e) => {
      // Ahead of every other handler, and nothing else sees this key.
      e.preventDefault();
      e.stopImmediatePropagation();
      if (e.code === 'Escape') return done();
      if (e.code === 'Backspace' || e.code === 'Delete') {
        rebind(id, slot, '');
        return done();
      }
      if (RESERVED_KEYS.has(e.code)) {
        this.opt.toast(`${keyLabel(e.code)} is kept for the game`);
        return;
      }
      const moved = rebind(id, slot, e.code);
      if (moved) this.opt.toast(`${keyLabel(e.code)} moved from ${moved}`);
      done();
    };
    const done = () => {
      window.removeEventListener('keydown', onKey, true);
      this.stopListening = null;
      this.renderKeys();
      this.opt.onKeys();
    };
    window.addEventListener('keydown', onKey, true);
    /** @type {(() => void) | null} */
    this.stopListening = done;
  }

  /**
   * Copy a field's text. The clipboard API is refused inside Activities (their Permissions-Policy), so fall back to
   * selecting the field and the old copy command, and failing that, ask for Ctrl+C. @param {string} id @param {string} done
   */
  async copy(id, done) {
    const field = input(id);
    let ok = false;
    try {
      await navigator.clipboard.writeText(field.value);
      ok = true;
    } catch {
      field.focus();
      field.select();
      try {
        ok = document.execCommand('copy');
      } catch {
        ok = false;
      }
    }
    $('r-copied').textContent = ok ? done : 'Selected: press Ctrl+C (or Cmd+C) to copy';
  }

  /** Join the room in the code field (the first press asks to confirm: it leaves this room). */
  async joinRoom() {
    const typed = input('r-join').value;
    const code = normalizeCode(typed);
    if (!code) {
      this.roomMessage('That is not a room code: 6 letters and digits, never O, 0, I, L or 1.');
      return;
    }
    if (code === this.world?.game?.code) {
      this.roomMessage('You are in that room already.');
      return;
    }
    if (this.confirmJoin !== code) {
      this.confirmJoin = code;
      $('r-join-go').textContent = `Leave and join ${code}`;
      return;
    }
    this.confirmJoin = '';
    $('r-join-go').textContent = 'Join';
    const err = await this.opt.onJoinRoom(code);
    if (err) this.roomMessage(err);
    else {
      input('r-join').value = '';
      this.close();
    }
  }

  /** @param {string} text */
  roomMessage(text) {
    $('r-msg').textContent = text;
    $('r-msg').hidden = !text;
  }

  /**
   * What the Room tab shows. @param {World} world
   * @param {{url?: string, instance?: boolean, canGoBack?: boolean, leave?: boolean} | null} room
   *   canGoBack: there is somewhere to go back to (the channel's room, or the lobby); leave: that is the lobby
   */
  roomInfo(world, room) {
    const g = world.game;
    if (!g?.code) return;
    if (input('r-code').value !== g.code) input('r-code').value = g.code;
    $('r-link-row').hidden = !room?.url;
    if (room?.url && input('r-link').value !== room.url) input('r-link').value = room.url;
    const sig = JSON.stringify([g.people, g.host]);
    if (sig !== this.peopleSig) {
      this.peopleSig = sig;
      $('r-people').replaceChildren(
        ...(g.people ?? []).map((n) => {
          const li = document.createElement('li');
          li.textContent = n;
          if (n === g.host) li.className = 'host';
          return li;
        }),
      );
    }
    $('r-back').hidden = !room?.canGoBack;
    const back = room?.leave ? 'Leave room' : "Back to this channel's room";
    if ($('r-back').textContent !== back) $('r-back').textContent = back;
  }

  /** @param {string} [tab] */
  open(tab = this.tab) {
    this.isOpen = true;
    $('menu').hidden = false;
    this.dirty = false;
    this.formSig = '';
    this.show(tab);
    this.fillMe();
    if (this.world) this.update(this.world);
  }

  close() {
    this.stopListening?.();
    this.isOpen = false;
    $('menu').hidden = true;
  }

  toggle() {
    if (this.isOpen) this.close();
    else this.open();
  }

  /** @param {string} tab */
  show(tab) {
    this.tab = tab;
    for (const b of document.querySelectorAll('#menu [data-tab]')) {
      const on = /** @type {HTMLElement} */ (b).dataset.tab === tab;
      b.setAttribute('aria-selected', String(on));
      b.classList.toggle('on', on);
    }
    for (const panel of document.querySelectorAll('#menu [data-panel]'))
      /** @type {HTMLElement} */ (panel).hidden = /** @type {HTMLElement} */ (panel).dataset.panel !== tab;
    /** @type {HTMLElement | null} */ (document.querySelector(`#menu [data-tab="${tab}"]`))?.focus();
  }

  fillMe() {
    const p = this.opt.prefs;
    input('m-volume').value = String(Math.round(this.opt.audio.volume * 100));
    input('m-mute').checked = this.opt.audio.muted;
    input('m-shake').checked = p.shake;
    input('m-names').checked = p.names;
    input('m-minimap').checked = p.minimap;
    select('m-motion').value = p.motion;
    select('m-quality').value = p.quality;
  }

  /** The arena's settings changed (or the menu just opened): refill the Game tab unless the player is editing. @param {World} world */
  update(world) {
    this.world = world;
    if (!this.isOpen) return;
    const g = world.game;
    const r = world.round;
    if (!g || !r) return;
    const me = world.you;
    const canEdit = !g.hostOnly || !!me?.host;
    // Who hosts, and whether this player may change things.
    const hostText = me?.host
      ? 'You are the host.'
      : `Host: ${g.host || 'nobody'}.${g.hostOnly ? ' Only the host can change these settings.' : ' Anyone can change these settings.'}`;
    if ($('g-host').textContent !== hostText) $('g-host').textContent = hostText;
    for (const el of $('panel-game').querySelectorAll('input, select'))
      /** @type {HTMLInputElement} */ (el).disabled = !canEdit && el.id !== 'g-newhost' && el.id !== 'g-kick';
    $('g-apply').hidden = !canEdit;
    $('g-host-tools').hidden = !me?.host;
    $('g-hostonly-row').hidden = !me?.host;
    const sig = JSON.stringify([g, r.mode, r.map]);
    if (sig === this.formSig || this.dirty) {
      this.refreshApply();
      return;
    }
    this.formSig = sig;
    $('g-modes').replaceChildren(...cards('g-mode', MODES.map((m) => ({ value: m.id, label: m.name })), r.mode));
    $('g-maps').replaceChildren(...cards('g-map', MAPS.map((m) => ({ value: m.id, label: m.name })), r.map));
    const size = select('g-size');
    if (![...size.options].some((o) => o.value === String(g.size))) size.add(new Option(`Custom (${g.size}×${g.size})`, String(g.size)));
    size.value = String(g.size);
    input('g-bots').value = String(g.bots);
    $('g-bots-out').textContent = String(g.bots);
    select('g-skill').value = g.skill ?? 'normal';
    const round = select('g-round');
    if (![...round.options].some((o) => o.value === String(g.roundMin))) round.add(new Option(`${g.roundMin} minutes`, String(g.roundMin)));
    round.value = String(g.roundMin ?? 0);
    select('g-shield').value = String(Math.round(g.shield ?? 0));
    input('g-cards').checked = !!g.cards;
    input('g-hostonly').checked = !!g.hostOnly;
    // Everyone else seated in the room: who the host may hand the role to, or remove (D-61).
    const others = (g.seats ?? []).filter((x) => !x[2]);
    for (const id of ['g-newhost', 'g-kick']) select(id).replaceChildren(...others.map(([ref, name]) => new Option(name, ref)));
    $('g-give').toggleAttribute('disabled', !others.length);
    $('g-kick-go').toggleAttribute('disabled', !others.length);
    this.kickArmed = '';
    $('g-kick-go').textContent = 'Remove';
    this.refreshApply();
  }

  /** What the form says, as settings. @returns {GameSettings} */
  formValues() {
    const checked = (/** @type {string} */ name) => /** @type {HTMLInputElement | null} */ (document.querySelector(`input[name="${name}"]:checked`))?.value;
    return {
      mode: checked('g-mode'),
      map: checked('g-map'),
      size: Number(select('g-size').value),
      bots: Number(input('g-bots').value),
      skill: /** @type {GameSettings['skill']} */ (select('g-skill').value),
      roundMin: Number(select('g-round').value),
      cards: input('g-cards').checked,
      shield: Number(select('g-shield').value),
      hostOnly: input('g-hostonly').checked,
    };
  }

  /** The Apply button says whether applying starts a new round. */
  refreshApply() {
    const r = this.world?.round;
    const g = this.world?.game;
    if (!r || !g) return;
    const f = this.formValues();
    const restart = f.mode !== r.mode || f.map !== r.map || f.size !== g.size;
    const label = restart ? 'Apply and start a new round' : 'Apply';
    if ($('g-apply').textContent !== label) $('g-apply').textContent = label;
    const mode = MODE_BY_ID[f.mode ?? r.mode];
    // Round length means nothing in an endless mode.
    select('g-round').disabled = select('g-round').disabled || !(mode && mode.roundTicks > 0);
  }

  async apply() {
    this.opt.audio.play('uiClick');
    const r = await this.opt.transport.settings(this.formValues()).catch(() => ({ ok: false, error: 'network' }));
    if (r.ok) {
      this.dirty = false;
      this.formSig = '';
      this.message('');
      this.opt.toast('Settings applied');
    } else
      this.message(
        r.error === 'not-host'
          ? 'Only the host can change these settings.'
          : r.error === 'too-soon'
            ? `A new round just started. Try again in ${Math.ceil((/** @type {any} */ (r).wait ?? 1000) / 1000)} s.`
            : 'Could not apply them. Try again.',
      );
  }

  /** @param {'restart' | 'end' | 'host'} action */
  async hostAction(action) {
    this.opt.audio.play('uiClick');
    const to = select('g-newhost').value;
    const r = await this.opt.transport.host({ action, ...(action === 'host' ? { to } : {}) }).catch(() => ({ ok: false, error: 'network' }));
    if (!r.ok)
      this.message(
        r.error === 'not-host'
          ? 'Only the host can do that.'
          : r.error === 'too-soon'
            ? `A new round just started. Try again in ${Math.ceil((/** @type {any} */ (r).wait ?? 1000) / 1000)} s.`
            : 'That did not work. Try again.',
      );
    else {
      this.message('');
      if (action !== 'host') this.close();
    }
  }

  /** Remove the chosen person: the first press asks, the second does it (they cannot come back to this room). */
  async kick() {
    const sel = select('g-kick');
    const ref = sel.value;
    if (!ref) return;
    const name = sel.selectedOptions[0]?.text ?? 'them';
    if (this.kickArmed !== ref) {
      this.kickArmed = ref;
      $('g-kick-go').textContent = `Remove ${name} for good?`;
      return;
    }
    this.kickArmed = '';
    $('g-kick-go').textContent = 'Remove';
    this.opt.audio.play('uiClick');
    const r = await this.opt.transport.host({ action: 'kick', to: ref }).catch(() => ({ ok: false, error: 'network' }));
    if (!r.ok) this.message(r.error === 'not-host' ? 'Only the host can do that.' : r.error === 'no-such-player' ? `${name} already left.` : 'That did not work. Try again.');
    else {
      this.message('');
      this.opt.toast(`Removed ${name}`);
    }
  }

  /** @param {string} text */
  message(text) {
    $('g-msg').textContent = text;
    $('g-msg').hidden = !text;
  }
}
