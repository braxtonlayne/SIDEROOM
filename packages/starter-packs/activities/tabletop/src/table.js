// The table's shared state and the host's rules. Everything authoritative lives under `host:*` keys, which only
// the instance's host may write (the SDK enforces that); participants send intents with `sdk.input` and the
// host applies them with `handleInput`, which is pure: it reads the document and returns the writes to make.
//
// Keys (values are JSON):
//   host:map            { kind, size, ox, oy, width, height, unit, unitName, diagonal, src, name }
//   host:fog            Fog.encode() string, or absent when the map has no fog
//   host:init           the initiative tracker (initiative.js)
//   host:tok:<id>       { id, name, img, owner, c, r, hidden, color, size }
//   host:rollseq        the number of the latest roll
//   host:roll:<seq>     the roll record (dice.js), only the last MAX_ROLLS_KEPT stay
//   ruler:<userId>      a member's own ruler { a:{c,r}, b:{c,r} } or null (not host-only; each writes their own)

import { gridDims, inBounds, validMap } from './grid.js';
import { Fog } from './fog.js';
import * as init from './initiative.js';
import { makeRollRecord, rollExpr, Rng, DiceError } from './dice.js';

export const MAX_TOKENS = 200;
export const MAX_ROLLS_KEPT = 50;
export const MAX_URL_CHARS = 2048;
/** A token picture or avatar as a data URL (a 64 px thumbnail is a few KB); the map image goes in chunks. */
export const MAX_DATA_URL_CHARS = 16_000;
export const CHUNK_CHARS = 12_000;
export const MAX_IMG_CHUNKS = 60;

export const K = {
  map: 'host:map',
  fog: 'host:fog',
  init: 'host:init',
  tok: (id) => `host:tok:${id}`,
  img: (id, n) => `host:img:${id}:${n}`,
  rollSeq: 'host:rollseq',
  roll: (n) => `host:roll:${n}`,
  ruler: (uid) => `ruler:${uid}`,
};

const COLORS = ['#e4572e', '#29a0b1', '#f2c14e', '#7d5ba6', '#4e9f3d', '#d96c9f', '#3a6ea5', '#c97b2a'];
export const colorFor = (id) => {
  let h = 0;
  for (const ch of String(id)) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return COLORS[h % COLORS.length];
};

const str = (v, n) => String(v ?? '').slice(0, n);
const tokenKeyId = (id) => /^[A-Za-z0-9_-]{1,64}$/.test(String(id));

const DATA_IMG = /^data:image\/(png|jpeg|webp);base64,[A-Za-z0-9+/=]+$/;

/** Only http(s) and small data:image URLs are drawn; anything else is dropped. */
export function safeImageUrl(u) {
  const s = String(u ?? '');
  if (/^https?:\/\//i.test(s)) return s.length <= MAX_URL_CHARS ? s : '';
  if (DATA_IMG.test(s)) return s.length <= MAX_DATA_URL_CHARS ? s : '';
  return '';
}

/** Cut a data URL into chunks that fit one state value each. */
export function splitChunks(dataUrl, size = CHUNK_CHARS) {
  const out = [];
  for (let i = 0; i < dataUrl.length; i += size) out.push(dataUrl.slice(i, i + size));
  return out;
}

/** The map image as a drawable URL: the chunks joined (null while any is missing), or the plain URL. */
export function mapImageUrl(doc, map) {
  if (!map) return null;
  if (map.imgId) {
    const parts = [];
    for (let n = 0; n < map.imgChunks; n++) {
      const p = doc[K.img(map.imgId, n)];
      if (typeof p !== 'string') return null;
      parts.push(p);
    }
    const url = parts.join('');
    return DATA_IMG.test(url) ? url : null;
  }
  return map.src || null;
}

/** The parsed view of the document: map, fog, tokens, tracker, rolls. Anything malformed is skipped. */
export function view(doc) {
  const rawMap = doc[K.map];
  const map = validMap(rawMap) ? rawMap : null;
  let fog = null;
  if (map && typeof doc[K.fog] === 'string') {
    const f = Fog.decode(doc[K.fog]);
    const d = gridDims(map);
    if (f && f.cols === d.cols && f.rows === d.rows) fog = f;
  }
  const tokens = [];
  const rolls = [];
  for (const [key, v] of Object.entries(doc)) {
    if (!v || typeof v !== 'object') continue;
    if (key.startsWith('host:tok:') && map && inBounds(map, v.c, v.r)) tokens.push(v);
    else if (key.startsWith('host:roll:') && key !== K.rollSeq) rolls.push(v);
  }
  tokens.sort((a, b) => String(a.id).localeCompare(String(b.id)));
  rolls.sort((a, b) => a.seq - b.seq);
  const rulers = {};
  for (const [key, v] of Object.entries(doc)) {
    if (key.startsWith('ruler:') && v && typeof v === 'object') rulers[key.slice(6)] = v;
  }
  return {
    map,
    fog,
    tokens,
    tracker: init.sanitize(doc[K.init]),
    rolls,
    rulers,
    image: mapImageUrl(doc, map),
  };
}

/**
 * Apply one intent. `env` = { hostId, now, seed } where `seed()` returns a fresh 64-bit bigint and `now` is a
 * millisecond timestamp. Returns { writes: [{ key, value }] } (a `null` value deletes the key) or
 * { writes: [], error } when the intent is refused.
 */
export function handleInput(doc, from, msg, env) {
  const refuse = (error) => ({ writes: [], error });
  if (!msg || typeof msg !== 'object' || typeof msg.t !== 'string') return refuse('malformed');
  const isHost = from === env.hostId;
  const v = view(doc);

  switch (msg.t) {
    case 'map': {
      if (!isHost) return refuse('host-only');
      const m = msg.map;
      const next = {
        kind: m?.kind,
        size: Number(m?.size),
        ox: Number(m?.ox ?? 0),
        oy: Number(m?.oy ?? 0),
        width: Number(m?.width),
        height: Number(m?.height),
        unit: Number(m?.unit ?? 5),
        unitName: str(m?.unitName ?? 'ft', 12),
        diagonal: ['chebyshev', 'alternate', 'euclid'].includes(m?.diagonal) ? m.diagonal : 'chebyshev',
        src: /^https?:\/\//i.test(String(m?.src ?? '')) ? safeImageUrl(m.src) : '',
        imgId: /^[a-z0-9]{6,16}$/.test(String(m?.imgId)) ? m.imgId : '',
        imgChunks:
          Number.isInteger(m?.imgChunks) && m.imgChunks >= 1 && m.imgChunks <= MAX_IMG_CHUNKS
            ? m.imgChunks
            : 0,
        name: str(m?.name, 64),
      };
      if (!validMap(next)) return refuse('bad-map');
      const writes = [{ key: K.map, value: next }];
      // A different grid invalidates the fog and moves tokens out of range back inside.
      if (doc[K.fog] !== undefined) writes.push({ key: K.fog, value: null });
      const d = gridDims(next);
      for (const t of v.tokens) {
        const c = Math.min(t.c, d.cols - 1);
        const r = Math.min(t.r, d.rows - 1);
        if (c !== t.c || r !== t.r) writes.push({ key: K.tok(t.id), value: { ...t, c, r } });
      }
      return { writes };
    }
    case 'fog': {
      if (!isHost) return refuse('host-only');
      if (!v.map) return refuse('no-map');
      if (msg.enc === null) return { writes: [{ key: K.fog, value: null }] };
      const f = Fog.decode(msg.enc);
      const d = gridDims(v.map);
      if (!f || f.cols !== d.cols || f.rows !== d.rows) return refuse('bad-fog');
      return { writes: [{ key: K.fog, value: f.encode() }] };
    }
    case 'token-add': {
      if (!v.map) return refuse('no-map');
      const t = msg.token ?? {};
      // A member may add a token for themselves; only the host adds others (monsters, props).
      const owner = isHost ? (t.owner ?? null) : from;
      if (!isHost && v.tokens.some((x) => x.owner === from)) return refuse('has-token');
      if (v.tokens.length >= MAX_TOKENS) return refuse('too-many');
      const id = tokenKeyId(t.id) ? String(t.id) : null;
      if (!id || v.tokens.some((x) => x.id === id)) return refuse('bad-id');
      const c = inBounds(v.map, t.c, t.r) ? t.c : 0;
      const r = inBounds(v.map, t.c, t.r) ? t.r : 0;
      const token = {
        id,
        name: str(t.name, 48) || '?',
        img: safeImageUrl(t.img),
        owner: owner === null ? null : str(owner, 64),
        c,
        r,
        hidden: isHost ? t.hidden === true : false,
        color: colorFor(id),
        size: [1, 2, 3].includes(t.size) ? t.size : 1,
      };
      return { writes: [{ key: K.tok(id), value: token }] };
    }
    case 'token-del': {
      const tok = doc[K.tok(msg.id)];
      if (!tok) return refuse('no-token');
      if (!isHost && tok.owner !== from) return refuse('not-yours');
      const writes = [{ key: K.tok(msg.id), value: null }];
      const tr = v.tracker;
      if (tr.entries.some((e) => e.tokenId === msg.id)) {
        let nt = tr;
        for (const e of tr.entries.filter((x) => x.tokenId === msg.id)) nt = init.removeEntry(nt, e.id);
        writes.push({ key: K.init, value: nt });
      }
      return { writes };
    }
    case 'token-set': {
      if (!isHost) return refuse('host-only');
      const tok = doc[K.tok(msg.id)];
      if (!tok) return refuse('no-token');
      const p = msg.patch ?? {};
      const n = { ...tok };
      if (p.name !== undefined) n.name = str(p.name, 48) || tok.name;
      if (p.img !== undefined) n.img = safeImageUrl(p.img);
      if (typeof p.hidden === 'boolean') n.hidden = p.hidden;
      if ([1, 2, 3].includes(p.size)) n.size = p.size;
      return { writes: [{ key: K.tok(msg.id), value: n }] };
    }
    case 'move': {
      const tok = doc[K.tok(msg.id)];
      if (!tok || !v.map) return refuse('no-token');
      if (!isHost && tok.owner !== from) return refuse('not-yours');
      if (!inBounds(v.map, msg.c, msg.r)) return refuse('out-of-bounds');
      if (tok.c === msg.c && tok.r === msg.r) return { writes: [] };
      return { writes: [{ key: K.tok(msg.id), value: { ...tok, c: msg.c, r: msg.r } }] };
    }
    case 'init': {
      const tr = v.tracker;
      const e = msg.entry ?? {};
      const mine = (id) => tr.entries.find((x) => x.id === id)?.owner === from;
      let nt;
      switch (msg.op) {
        case 'add':
          if (!isHost) return refuse('host-only');
          nt = init.addEntry(tr, e);
          break;
        case 'update':
          if (!isHost && !mine(e.id)) return refuse('not-yours');
          // A member may change their own score and name, not "out".
          nt = init.updateEntry(tr, e.id, isHost ? e : { init: e.init, name: e.name });
          break;
        case 'remove':
          if (!isHost) return refuse('host-only');
          nt = init.removeEntry(tr, e.id);
          break;
        case 'start':
          if (!isHost) return refuse('host-only');
          nt = init.start(tr);
          break;
        case 'stop':
          if (!isHost) return refuse('host-only');
          nt = init.stop(tr);
          break;
        case 'next':
          // The host, or the member whose turn it is, ends the turn.
          if (!isHost && !(tr.turn !== null && mine(tr.turn))) return refuse('not-your-turn');
          nt = init.next(tr);
          break;
        case 'prev':
          if (!isHost) return refuse('host-only');
          nt = init.previous(tr);
          break;
        default:
          return refuse('malformed');
      }
      return nt === tr ? { writes: [] } : { writes: [{ key: K.init, value: nt }] };
    }
    case 'roll': {
      const expr = str(msg.expr, 200);
      try {
        rollExpr(expr, new Rng(0n)); // parse and bound-check before spending a sequence number
      } catch (e) {
        return refuse(e instanceof DiceError ? e.code : 'bad-roll');
      }
      const seq = (Number.isInteger(doc[K.rollSeq]) ? doc[K.rollSeq] : 0) + 1;
      const rec = makeRollRecord({
        seq,
        by: from,
        expression: expr,
        seed: env.seed(),
        label: str(msg.label, 48),
        at: env.now,
      });
      const writes = [
        { key: K.roll(seq), value: rec },
        { key: K.rollSeq, value: seq },
      ];
      if (seq > MAX_ROLLS_KEPT) writes.push({ key: K.roll(seq - MAX_ROLLS_KEPT), value: null });
      // "Roll initiative": the total becomes the entry's score (the host for anyone, a member for their own).
      if (typeof msg.initFor === 'string') {
        const entry = v.tracker.entries.find((x) => x.id === msg.initFor);
        if (entry && (isHost || entry.owner === from)) {
          writes.push({ key: K.init, value: init.updateEntry(v.tracker, entry.id, { init: rec.total }) });
        }
      }
      return { writes };
    }
    default:
      return refuse('malformed');
  }
}

/** Apply writes to a document in place (null deletes). */
export function applyWrites(doc, writes) {
  for (const w of writes) {
    if (w.value === null || w.value === undefined) delete doc[w.key];
    else doc[w.key] = w.value;
  }
  return doc;
}
