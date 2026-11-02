// @ts-check
// The heads-up display: score, who is playing, renderer and frame rate, and an optional network line. DOM text
// only (textContent, never innerHTML), refreshed a few times a second.

import { SLOT_COLORS } from './scene.js';

/**
 * @typedef {object} HudState
 * @property {'host' | 'peer'} role
 * @property {string} status
 * @property {number} score
 * @property {{ slot: number, name: string, bot: boolean }[]} roster
 * @property {number} mySlot
 * @property {string} renderer
 * @property {number} fps
 * @property {Record<string, number> | null} net
 * @property {boolean} showNet
 */

export class Hud {
  /** @param {Document} doc */
  constructor(doc) {
    this.doc = doc;
    /** @param {string} id */
    const el = (id) => /** @type {HTMLElement} */ (doc.getElementById(id));
    this.score = el('hud-score');
    this.players = el('hud-players');
    this.info = el('hud-info');
    this.net = el('hud-net');
    this.at = 0;
  }

  /** @param {HudState} s */
  update(s) {
    const now = performance.now();
    if (now - this.at < 250) return;
    this.at = now;
    this.score.textContent = `Boxes on the pad: ${s.score}`;
    const items = s.roster.map((r) => {
      const li = this.doc.createElement('li');
      const dot = this.doc.createElement('span');
      dot.className = 'dot';
      const [cr, cg, cb] = SLOT_COLORS[r.slot % SLOT_COLORS.length].map((v) => Math.round(Math.pow(v, 1 / 2.2) * 255));
      dot.style.background = `rgb(${cr}, ${cg}, ${cb})`;
      li.append(dot, `${r.name}${r.slot === s.mySlot ? ' (you)' : ''}${r.slot === 0 ? ' · host' : ''}`);
      return li;
    });
    this.players.replaceChildren(...items);
    this.info.textContent = `${s.renderer === 'webgpu' ? 'WebGPU' : 'WebGL2'} · ${Math.round(s.fps)} fps · ${s.role === 'host' ? 'you host' : s.status}`;
    this.net.hidden = !s.showNet || !s.net;
    if (s.net) this.net.textContent = Object.entries(s.net).map(([k, v]) => `${k} ${Number.isInteger(v) ? v : v.toFixed(3)}`).join(' · ');
  }
}
