// The tabletop UI: a canvas for the map and a side column of sections (map, fog, tokens, initiative, dice).
// It reads only through the Transport and changes the table only by sending intents (see table.js).

import { t } from './strings.js';
import { Fog } from './fog.js';
import { cellCenter, gridDims, inBounds, measure, pixelToCell, cellDistance } from './grid.js';
import { K, view as readView, safeImageUrl, splitChunks } from './table.js';
import * as init from './initiative.js';
import { DiceError, Rng, describeRecord, rollExpr, seedFromHex, seedToHex, verifyRecord } from './dice.js';
import { loadImage, mapFromFile, newId, thumbFromAvatar, thumbFromFile } from './images.js';
import { draw, fitView, toImage, tokenVisible } from './render.js';

/** Build an element: h('div', { class: 'x', onclick }, child, 'text'). */
export function h(tag, props = {}, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'class') el.className = v;
    else if (k === 'dataset') Object.assign(el.dataset, v);
    else if (k in el && k !== 'list') el[k] = v;
    else el.setAttribute(k, v === true ? '' : v);
  }
  for (const kid of kids.flat()) if (kid !== null && kid !== undefined && kid !== false) el.append(kid);
  return el;
}

const field = (label, input) => h('label', { class: 'field' }, h('span', {}, label), input);
const tokenIdFor = (uid) => 'm' + String(uid).replace(/[^A-Za-z0-9_-]/g, '').slice(0, 60);

export function startApp(root, transport, opts = {}) {
  const me = transport.me.id;
  const isHost = transport.isHost;
  const nameOf = (id) => {
    if (id === me) return transport.me.name;
    return transport.people().find((p) => p.id === id)?.name ?? String(id);
  };

  // --- model -------------------------------------------------------------------------------------------
  let v = readView(transport.doc());
  let mapImg = null;
  let mapImgUrl = null;
  const thumbs = new Map();
  const ui = {
    tool: 'move',
    brush: 'round',
    radius: 1,
    drag: null,
    preview: null,
    measure: null,
    fogDraft: null,
    view: { scale: 1, tx: 0, ty: 0 },
    fitted: false,
    pan: null,
    status: '',
    statusTimer: null,
  };

  const act = (msg) => transport.send(msg);
  const say = (text) => {
    ui.status = text;
    statusEl.textContent = text;
    clearTimeout(ui.statusTimer);
    ui.statusTimer = setTimeout(() => (statusEl.textContent = ''), 6000);
  };
  // The host's rule engine reports refusals of intents (its own included) through this hook.
  if (opts.hooks) {
    opts.hooks.onRefuse = (from, _msg, code) => {
      if (from === me) say(t('refused', { code }));
    };
  }

  // --- layout ------------------------------------------------------------------------------------------
  const canvas = h('canvas', { class: 'board', tabIndex: 0, dataset: { testid: 'board' } });
  const emptyEl = h('div', { class: 'empty' });
  const statusEl = h('div', { class: 'status', role: 'status', 'aria-live': 'polite' });
  const toolbar = h('div', { class: 'toolbar', role: 'toolbar', 'aria-label': t('tools') });
  const stage = h('div', { class: 'stage' }, canvas, emptyEl);
  const side = h('div', { class: 'side' });
  root.replaceChildren(
    h(
      'header',
      { class: 'top' },
      h('h1', {}, t('title')),
      h(
        'span',
        { class: 'badge', dataset: { testid: 'role' } },
        isHost ? t('hostBadge') : t('guestBadge', { name: nameOf(transport.hostId) }),
      ),
      statusEl,
    ),
    h('main', { class: 'layout' }, h('div', { class: 'left' }, toolbar, stage), side),
  );

  // --- toolbar -----------------------------------------------------------------------------------------
  function buildToolbar() {
    const tools = [
      ['move', t('toolMove')],
      ['measure', t('toolMeasure')],
      ...(isHost ? [['hide', t('toolHide')], ['reveal', t('toolReveal')]] : []),
    ];
    toolbar.replaceChildren(
      ...[
      ...tools.map(([id, label]) =>
        h('button', {
          type: 'button',
          class: 'tool' + (ui.tool === id ? ' on' : ''),
          'aria-pressed': String(ui.tool === id),
          dataset: { tool: id },
          textContent: label,
          onclick: () => {
            ui.tool = id;
            buildToolbar();
          },
        }),
      ),
      h('button', { type: 'button', class: 'tool', textContent: t('fit'), onclick: () => fit() }),
      isHost && (ui.tool === 'hide' || ui.tool === 'reveal')
        ? h(
            'span',
            { class: 'brush' },
            h(
              'select',
              {
                'aria-label': t('brush'),
                onchange: (e) => (ui.brush = e.target.value),
              },
              h('option', { value: 'round', selected: ui.brush === 'round', textContent: t('brushRound') }),
              h('option', { value: 'rect', selected: ui.brush === 'rect', textContent: t('brushRect') }),
            ),
            h('input', {
              type: 'number',
              min: 0,
              max: 20,
              value: ui.radius,
              'aria-label': t('brush'),
              onchange: (e) => (ui.radius = Math.max(0, Math.min(20, Number(e.target.value) || 0))),
            }),
          )
        : null,
      ].filter(Boolean),
    );
  }

  // --- canvas ------------------------------------------------------------------------------------------
  const ctx2d = canvas.getContext('2d');
  let cw = 0;
  let ch = 0;
  function resize() {
    const r = stage.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    cw = Math.max(Math.floor(r.width), 100);
    ch = Math.max(Math.floor(r.height), 100);
    canvas.width = cw * dpr;
    canvas.height = ch * dpr;
    canvas.style.width = `${cw}px`;
    canvas.style.height = `${ch}px`;
    ctx2d.setTransform(dpr, 0, 0, dpr, 0, 0);
    if (!ui.fitted && v.map) fit();
    paint();
  }
  function fit() {
    if (!v.map) return;
    ui.view = fitView(v.map, cw, ch);
    ui.fitted = true;
    paint();
  }
  let raf = 0;
  function paint() {
    if (raf) return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      publishLayout();
      draw(
        ctx2d,
        cw,
        ch,
        {
          map: v.map,
          image: mapImg,
          fog: v.fog,
          fogPreview: ui.fogDraft,
          tokens: v.tokens,
          tracker: v.tracker,
          rulers: Object.fromEntries(Object.entries(v.rulers).filter(([u]) => u !== me)),
          isHost,
          me,
          thumbs,
          drag: ui.drag,
          preview: ui.preview,
          measure: ui.measure,
        },
        ui.view,
      );
    });
  }

  // Where things are on the canvas, for the live spec (and anyone driving the page from outside): token
  // centres and the cell size in canvas pixels, as JSON in a data attribute.
  function publishLayout() {
    if (!v.map) return;
    canvas.dataset.cell = String(v.map.size * ui.view.scale);
    canvas.dataset.tokens = JSON.stringify(
      v.tokens.map((k) => {
        const p = cellCenter(v.map, k.c, k.r);
        return { id: k.id, c: k.c, r: k.r, x: p.x * ui.view.scale + ui.view.tx, y: p.y * ui.view.scale + ui.view.ty };
      }),
    );
  }

  const pointerCell = (e) => {
    const r = canvas.getBoundingClientRect();
    const p = toImage(ui.view, e.clientX - r.left, e.clientY - r.top);
    return { p, cell: pixelToCell(v.map, p.x, p.y) };
  };
  const canMove = (tok) => isHost || tok.owner === me;
  const tokenAt = (cell) => {
    // The topmost (last drawn) visible token whose footprint contains the cell.
    for (let i = v.tokens.length - 1; i >= 0; i--) {
      const k = v.tokens[i];
      if (!tokenVisible(k, v.fog, isHost)) continue;
      const reach = (k.size ?? 1) - 1;
      if (Math.abs(k.c - cell.c) <= reach / 2 + 0.01 && Math.abs(k.r - cell.r) <= reach / 2 + 0.01)
        return k;
    }
    return null;
  };

  let lastRulerKey = '';
  function publishRuler(ru) {
    const key = ru ? `${ru.a.c},${ru.a.r}:${ru.b.c},${ru.b.r}` : '';
    if (key === lastRulerKey) return;
    lastRulerKey = key;
    try {
      transport.set(K.ruler(me), ru);
    } catch {
      /* a transport that cannot take it just keeps the ruler local */
    }
  }
  let rulerTimer = 0;
  function clearRuler() {
    clearTimeout(rulerTimer);
    ui.measure = null;
    publishRuler(null);
    paint();
  }

  function fogBase() {
    const d = gridDims(v.map);
    return v.fog ? v.fog.clone() : new Fog(d.cols, d.rows, false);
  }
  function paintFog(base, from, to, hidden) {
    if (ui.brush === 'rect') base.paintRect(from.c, from.r, to.c, to.r, hidden);
    else base.paintBrush(v.map, to.c, to.r, ui.radius, hidden);
  }

  canvas.addEventListener('pointerdown', (e) => {
    if (!v.map) return;
    canvas.setPointerCapture(e.pointerId);
    canvas.focus();
    const { p, cell } = pointerCell(e);
    if (e.button === 1 || e.button === 2) {
      ui.pan = { x: e.clientX, y: e.clientY, tx: ui.view.tx, ty: ui.view.ty };
      return;
    }
    if (ui.tool === 'move') {
      const tok = tokenAt(cell);
      if (tok && canMove(tok)) {
        ui.drag = { id: tok.id, at: { x: p.x, y: p.y } };
        ui.preview = cell;
      } else ui.pan = { x: e.clientX, y: e.clientY, tx: ui.view.tx, ty: ui.view.ty };
    } else if (ui.tool === 'measure') {
      if (!inBounds(v.map, cell.c, cell.r)) return;
      clearTimeout(rulerTimer);
      ui.measure = { a: cell, b: cell };
    } else if (isHost && (ui.tool === 'hide' || ui.tool === 'reveal')) {
      ui.fogDraft = fogBase();
      ui.fogStart = cell;
      paintFog(ui.fogDraft, cell, cell, ui.tool === 'hide');
    }
    paint();
  });
  canvas.addEventListener('pointermove', (e) => {
    if (!v.map) return;
    if (ui.pan) {
      ui.view = { ...ui.view, tx: ui.pan.tx + (e.clientX - ui.pan.x), ty: ui.pan.ty + (e.clientY - ui.pan.y) };
      return paint();
    }
    const { p, cell } = pointerCell(e);
    if (ui.drag) {
      ui.drag.at = { x: p.x, y: p.y };
      ui.preview = inBounds(v.map, cell.c, cell.r) ? cell : null;
      paint();
    } else if (ui.measure) {
      if (inBounds(v.map, cell.c, cell.r)) {
        ui.measure = { a: ui.measure.a, b: cell };
        publishRuler(ui.measure);
      }
      paint();
    } else if (ui.fogDraft) {
      const base = ui.brush === 'rect' ? fogBase() : ui.fogDraft;
      ui.fogDraft = base;
      paintFog(base, ui.fogStart, cell, ui.tool === 'hide');
      paint();
    }
  });
  const end = (e) => {
    if (ui.pan) {
      ui.pan = null;
      return;
    }
    if (ui.drag) {
      const { cell } = pointerCell(e);
      const id = ui.drag.id;
      ui.drag = null;
      ui.preview = null;
      if (inBounds(v.map, cell.c, cell.r)) act({ t: 'move', id, c: cell.c, r: cell.r });
      paint();
    } else if (ui.measure) {
      publishRuler(ui.measure);
      rulerTimer = setTimeout(clearRuler, 10_000);
      const d = measure(v.map, ui.measure.a, ui.measure.b);
      say(t('measureResult', { value: d.value, unit: d.unitName, cells: d.cells }));
    } else if (ui.fogDraft) {
      const f = ui.fogDraft;
      ui.fogDraft = null;
      act({ t: 'fog', enc: f.encode() });
      paint();
    }
  };
  canvas.addEventListener('pointerup', end);
  canvas.addEventListener('pointercancel', () => {
    ui.drag = null;
    ui.preview = null;
    ui.pan = null;
    ui.fogDraft = null;
    paint();
  });
  canvas.addEventListener('contextmenu', (e) => e.preventDefault());
  canvas.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      ui.drag = null;
      ui.preview = null;
      ui.fogDraft = null;
      clearRuler();
    }
  });
  canvas.addEventListener(
    'wheel',
    (e) => {
      if (!v.map) return;
      e.preventDefault();
      const r = canvas.getBoundingClientRect();
      const x = e.clientX - r.left;
      const y = e.clientY - r.top;
      const k = Math.exp(-e.deltaY * 0.0015);
      const scale = Math.min(Math.max(ui.view.scale * k, 0.1), 8);
      const f = scale / ui.view.scale;
      ui.view = { scale, tx: x - (x - ui.view.tx) * f, ty: y - (y - ui.view.ty) * f };
      paint();
    },
    { passive: false },
  );

  // --- side sections -----------------------------------------------------------------------------------
  // Each section rebuilds only when its signature changes, so typing in a field is never interrupted.
  const sections = [];
  function section(id, title, sig, build) {
    const body = h('div', { class: 'body' });
    const el = h('section', { class: 'card', dataset: { section: id } }, h('h2', {}, title), body);
    side.append(el);
    sections.push({ sig, build, body, last: null, el });
  }

  // Map and fog (host controls; everyone sees the summary).
  const mapForm = { kind: 'square', size: 50, ox: 0, oy: 0, unit: 5, unitName: 'ft', diagonal: 'chebyshev', url: '', file: null };
  section(
    'map',
    t('mapSection'),
    () => JSON.stringify([v.map, !!v.fog, isHost]),
    (body) => {
      if (!isHost) {
        body.replaceChildren(h('p', {}, v.map ? v.map.name || t('mapSection') : t('noMap')));
        return;
      }
      if (v.map) Object.assign(mapForm, { kind: v.map.kind, size: v.map.size, ox: v.map.ox, oy: v.map.oy, unit: v.map.unit, unitName: v.map.unitName, diagonal: v.map.diagonal });
      const sel = (key, opts2) =>
        h(
          'select',
          { onchange: (e) => (mapForm[key] = e.target.value) },
          opts2.map(([val, label]) => h('option', { value: val, selected: mapForm[key] === val, textContent: label })),
        );
      const num = (key, min = 0) =>
        h('input', { type: 'number', min, value: mapForm[key], onchange: (e) => (mapForm[key] = Number(e.target.value)) });
      body.replaceChildren(
        field(
          t('mapImage'),
          h('input', {
            type: 'file',
            accept: 'image/png,image/jpeg,image/webp',
            dataset: { testid: 'map-file' },
            onchange: (e) => (mapForm.file = e.target.files?.[0] ?? null),
          }),
        ),
        h('p', { class: 'hint' }, t('mapImageHelp')),
        field(
          t('mapUrl'),
          h('input', { type: 'url', placeholder: 'https://', value: mapForm.url, onchange: (e) => (mapForm.url = e.target.value.trim()) }),
        ),
        field(t('gridKind'), sel('kind', [['square', t('gridSquare')], ['hex', t('gridHex')]])),
        field(t('cellSize'), num('size', 8)),
        field(t('offsetX'), num('ox')),
        field(t('offsetY'), num('oy')),
        field(t('unit'), num('unit', 0)),
        field(t('unitName'), h('input', { type: 'text', maxLength: 12, value: mapForm.unitName, onchange: (e) => (mapForm.unitName = e.target.value) })),
        field(t('diagonal'), sel('diagonal', [['chebyshev', t('diagChebyshev')], ['alternate', t('diagAlternate')], ['euclid', t('diagEuclid')]])),
        h('button', { type: 'button', class: 'primary', dataset: { testid: 'map-apply' }, textContent: t('apply'), onclick: applyMap }),
        h('h3', {}, t('fogSection')),
        h(
          'div',
          { class: 'row' },
          h('button', { type: 'button', disabled: !v.map, textContent: t('fogCoverAll'), onclick: () => v.map && act({ t: 'fog', enc: new Fog(gridDims(v.map).cols, gridDims(v.map).rows, true).encode() }) }),
          h('button', { type: 'button', disabled: !v.map, textContent: t('fogClearAll'), onclick: () => v.map && act({ t: 'fog', enc: new Fog(gridDims(v.map).cols, gridDims(v.map).rows, false).encode() }) }),
          h('button', { type: 'button', disabled: !v.fog, textContent: t('fogOff'), onclick: () => act({ t: 'fog', enc: null }) }),
        ),
        h('p', { class: 'hint' }, t('fogHostView')),
        h('p', { class: 'hint' }, t('fogNote')),
      );
    },
  );

  async function applyMap() {
    const base = { kind: mapForm.kind, size: mapForm.size, ox: mapForm.ox, oy: mapForm.oy, unit: mapForm.unit, unitName: mapForm.unitName, diagonal: mapForm.diagonal };
    try {
      let map;
      if (mapForm.file) {
        const img = await mapFromFile(mapForm.file);
        const id = newId('i');
        const chunks = splitChunks(img.url);
        // Old chunks go; the new ones are written before the map points at them.
        for (const key of Object.keys(transport.doc())) if (key.startsWith('host:img:')) transport.set(key, null);
        chunks.forEach((c, i) => transport.set(K.img(id, i), c));
        map = { ...base, width: img.width, height: img.height, imgId: id, imgChunks: chunks.length, name: mapForm.file.name.slice(0, 64) };
        mapForm.file = null;
        ui.fitted = false;
      } else if (/^https?:\/\//i.test(mapForm.url) && safeImageUrl(mapForm.url)) {
        const im = await loadImage(mapForm.url);
        if (!im) return say(t('imageFailed'));
        map = { ...base, width: im.naturalWidth, height: im.naturalHeight, src: mapForm.url, name: '' };
        ui.fitted = false;
      } else if (v.map) {
        map = { ...v.map, ...base };
      } else return say(t('imageFailed'));
      act({ t: 'map', map });
    } catch (e) {
      say(e instanceof Error && e.message === 'too-big' ? t('imageTooBig') : t('imageFailed'));
    }
  }

  // Tokens.
  const tokForm = { name: '', hidden: false, size: 1, file: null };
  section(
    'tokens',
    t('tokensSection'),
    () => JSON.stringify([v.tokens, !!v.map, v.tracker.entries.map((e) => e.tokenId)]),
    (body) => {
      const mine = v.tokens.find((k) => k.owner === me);
      const rows = v.tokens.map((k) =>
        h(
          'li',
          { class: 'tokrow', dataset: { token: k.id } },
          h('span', { class: 'swatch', style: `background:${k.color}` }),
          h('span', { class: 'grow' }, k.name + (k.owner === me ? ` (${t('you')})` : '') + (k.hidden ? ` [${t('hidden')}]` : '')),
          isHost
            ? [
                h('button', { type: 'button', textContent: k.hidden ? t('show') : t('hide'), onclick: () => act({ t: 'token-set', id: k.id, patch: { hidden: !k.hidden } }) }),
                h('button', { type: 'button', textContent: `${t('size')} ${k.size}`, onclick: () => act({ t: 'token-set', id: k.id, patch: { size: (k.size % 3) + 1 } }) }),
                !v.tracker.entries.some((e) => e.tokenId === k.id) &&
                  h('button', { type: 'button', textContent: t('initAddToken'), onclick: () => act({ t: 'init', op: 'add', entry: { id: `e${k.id}`, name: k.name, init: 0, tokenId: k.id, owner: k.owner } }) }),
              ]
            : null,
          (isHost || k.owner === me) && h('button', { type: 'button', textContent: t('remove'), onclick: () => act({ t: 'token-del', id: k.id }) }),
        ),
      );
      body.replaceChildren(
        h('ul', { class: 'list' }, rows),
        v.map && !mine
          ? h(
              'div',
              { class: 'row' },
              h('button', { type: 'button', class: 'primary', dataset: { testid: 'add-mine' }, textContent: t('addMine'), onclick: addMine }),
              h('label', { class: 'filebtn' }, t('myPicture'), h('input', { type: 'file', accept: 'image/*', onchange: (e) => e.target.files?.[0] && addMine(e.target.files[0]) })),
            )
          : null,
        isHost && v.map
          ? h(
              'div',
              { class: 'stack' },
              h('h3', {}, t('addProp')),
              field(t('tokenName'), h('input', { type: 'text', maxLength: 48, value: tokForm.name, dataset: { testid: 'prop-name' }, oninput: (e) => (tokForm.name = e.target.value) })),
              h('label', { class: 'check' }, h('input', { type: 'checkbox', checked: tokForm.hidden, onchange: (e) => (tokForm.hidden = e.target.checked) }), t('hidden')),
              h('input', { type: 'file', accept: 'image/*', onchange: (e) => (tokForm.file = e.target.files?.[0] ?? null) }),
              h('button', { type: 'button', dataset: { testid: 'add-prop' }, textContent: t('initAdd'), onclick: addProp }),
            )
          : null,
      );
    },
  );

  function freeCell() {
    const taken = new Set(v.tokens.map((k) => `${k.c},${k.r}`));
    const d = gridDims(v.map);
    for (let r = 0; r < d.rows; r++) for (let c = 0; c < d.cols; c++) if (!taken.has(`${c},${r}`)) return { c, r };
    return { c: 0, r: 0 };
  }
  async function addMine(file) {
    if (!v.map) return;
    let img = '';
    if (file instanceof File) img = await thumbFromFile(file).catch(() => '');
    else img = await thumbFromAvatar(transport.me.avatarUrl);
    act({ t: 'token-add', token: { id: tokenIdFor(me), name: transport.me.name, img, ...freeCell() } });
  }
  async function addProp() {
    if (!v.map) return;
    const img = tokForm.file ? await thumbFromFile(tokForm.file).catch(() => '') : '';
    const id = newId('p');
    act({ t: 'token-add', token: { id, name: tokForm.name.trim() || '?', img, hidden: tokForm.hidden, owner: null, ...freeCell() } });
    tokForm.name = '';
    tokForm.file = null;
    tokForm.hidden = false;
  }

  // Initiative.
  section(
    'initiative',
    t('initSection'),
    () => JSON.stringify([v.tracker, isHost]),
    (body) => {
      const tr = v.tracker;
      const order = init.ordered(tr);
      const myTurn = tr.turn !== null && tr.entries.find((e) => e.id === tr.turn)?.owner === me;
      const rows = order.map((e) =>
        h(
          'li',
          { class: 'initrow' + (e.id === tr.turn ? ' turn' : '') + (e.out ? ' out' : ''), dataset: { entry: e.id } },
          h('span', { class: 'grow' }, e.name),
          h('input', {
            type: 'number',
            class: 'score',
            value: e.init,
            'aria-label': t('initScore'),
            disabled: !(isHost || e.owner === me),
            onchange: (ev) => act({ t: 'init', op: 'update', entry: { id: e.id, init: Number(ev.target.value) } }),
          }),
          (isHost || e.owner === me) &&
            h('button', { type: 'button', textContent: t('initRollOne'), title: 'd20', onclick: () => act({ t: 'roll', expr: 'd20', label: t('initSection'), initFor: e.id }) }),
          isHost && h('button', { type: 'button', textContent: t('initOut'), 'aria-pressed': String(e.out), onclick: () => act({ t: 'init', op: 'update', entry: { id: e.id, out: !e.out } }) }),
          isHost && h('button', { type: 'button', textContent: '×', 'aria-label': t('remove'), onclick: () => act({ t: 'init', op: 'remove', entry: { id: e.id } }) }),
        ),
      );
      const addName = h('input', { type: 'text', maxLength: 48, 'aria-label': t('tokenName') });
      body.replaceChildren(
        tr.round > 0 ? h('p', { class: 'round', dataset: { testid: 'round' } }, t('initRound', { n: tr.round }), ' - ', t('turnOf', { name: tr.entries.find((e) => e.id === tr.turn)?.name ?? '' })) : null,
        order.length ? h('ul', { class: 'list' }, rows) : h('p', { class: 'hint' }, t('initEmpty')),
        isHost
          ? h(
              'div',
              { class: 'row' },
              h('button', { type: 'button', dataset: { testid: 'init-start' }, textContent: t('initStart'), onclick: () => act({ t: 'init', op: 'start' }) }),
              h('button', { type: 'button', textContent: t('initPrev'), onclick: () => act({ t: 'init', op: 'prev' }) }),
              h('button', { type: 'button', dataset: { testid: 'init-next' }, textContent: t('initNext'), onclick: () => act({ t: 'init', op: 'next' }) }),
              h('button', { type: 'button', textContent: t('initStop'), onclick: () => act({ t: 'init', op: 'stop' }) }),
            )
          : myTurn
            ? h('button', { type: 'button', class: 'primary', textContent: t('initEnd'), onclick: () => act({ t: 'init', op: 'next' }) })
            : null,
        isHost
          ? h('div', { class: 'row' }, addName, h('button', { type: 'button', textContent: t('initAdd'), onclick: () => { const nm = addName.value.trim(); if (nm) act({ t: 'init', op: 'add', entry: { id: newId('e'), name: nm, init: 0 } }); } }))
          : null,
      );
    },
  );

  // Dice tray: the form is built once; only the log rebuilds.
  const dice = { expr: '', label: '' };
  const exprInput = h('input', { type: 'text', maxLength: 200, placeholder: '2d20kh1+5', 'aria-label': t('diceExpr'), dataset: { testid: 'dice-expr' }, oninput: (e) => (dice.expr = e.target.value) });
  const labelInput = h('input', { type: 'text', maxLength: 48, 'aria-label': t('diceLabel'), oninput: (e) => (dice.label = e.target.value) });
  const diceErr = h('p', { class: 'error', role: 'alert' });
  const logEl = h('ol', { class: 'rolls', dataset: { testid: 'rolls' } });
  const checkExpr = h('input', { type: 'text', maxLength: 200, 'aria-label': t('diceExpr') });
  const checkSeed = h('input', { type: 'text', maxLength: 16, placeholder: '0000000000000000', 'aria-label': t('rollSeed'), class: 'mono' });
  const checkOut = h('p', { class: 'mono' });

  function addDie(sides) {
    const cur = exprInput.value.trim();
    const m = /^(\d*)d(\d+)$/.exec(cur);
    if (cur === '') exprInput.value = `1d${sides}`;
    else if (m && Number(m[2]) === sides) exprInput.value = `${Number(m[1] || 1) + 1}d${sides}`;
    else exprInput.value = `${cur}+1d${sides}`;
    dice.expr = exprInput.value;
  }
  function doRoll() {
    const expr = exprInput.value.trim() || 'd20';
    try {
      rollExpr(expr, new Rng(0n));
    } catch (e) {
      diceErr.textContent = t('diceBadRoll', { code: e instanceof DiceError ? e.code : 'error' });
      return;
    }
    diceErr.textContent = '';
    act({ t: 'roll', expr, label: dice.label });
  }
  exprInput.addEventListener('keydown', (e) => e.key === 'Enter' && doRoll());
  section('dice', t('diceSection'), () => 'static', (body) => {
    body.replaceChildren(
      h('div', { class: 'row wrap' }, [4, 6, 8, 10, 12, 20, 100].map((n) => h('button', { type: 'button', class: 'die', textContent: `d${n}`, dataset: { die: n }, onclick: () => addDie(n) }))),
      field(t('diceExpr'), exprInput),
      field(t('diceLabel'), labelInput),
      h('button', { type: 'button', class: 'primary', dataset: { testid: 'dice-roll' }, textContent: t('diceRoll'), onclick: doRoll }),
      diceErr,
      logEl,
      h(
        'details',
        {},
        h('summary', {}, t('rollCheck')),
        h('p', { class: 'hint' }, t('rollCheckHelp')),
        checkExpr,
        checkSeed,
        h('button', { type: 'button', textContent: t('rollCheckGo'), onclick: () => {
          try {
            const r = rollExpr(checkExpr.value, new Rng(seedFromHex(checkSeed.value.trim().toLowerCase())));
            checkOut.textContent = describeRecord({ ...r, seed: checkSeed.value });
          } catch (e) {
            checkOut.textContent = t('diceBadRoll', { code: e instanceof DiceError ? e.code : 'error' });
          }
        } }),
        checkOut,
      ),
    );
  });

  let lastRolls = '';
  function renderRolls() {
    const sig = JSON.stringify(v.rolls.map((r) => r.seq));
    if (sig === lastRolls) return;
    lastRolls = sig;
    if (!v.rolls.length) return logEl.replaceChildren(h('li', { class: 'hint' }, t('rollsEmpty')));
    logEl.replaceChildren(
      ...[...v.rolls].reverse().map((r) => {
        const ok = verifyRecord(r);
        return h(
          'li',
          { class: 'roll' + (ok ? '' : ' bad'), dataset: { seq: r.seq } },
          h('div', { class: 'who' }, t('rollBy', { name: nameOf(r.by) }), r.label ? ` - ${r.label}` : ''),
          h('div', { class: 'big' }, h('span', { class: 'total', dataset: { testid: 'roll-total' } }, String(r.total)), ' ', h('span', { class: 'expr' }, r.expression)),
          h('div', { class: 'faces' }, r.groups.map((g) => h('span', { class: 'group' }, `${g.notation}: `, g.dice.map((d) => h('span', { class: 'face' + (d.dropped ? ' dropped' : '') + (d.exploded ? ' exploded' : '') }, String(d.value)))))),
          h('div', { class: 'seed mono' }, `${t('rollSeed')} `, h('span', { dataset: { testid: 'roll-seed' } }, r.seed), ' ', h('span', { class: ok ? 'good' : 'badtxt' }, ok ? `✓ ${t('rollVerified')}` : t('rollMismatch'))),
        );
      }),
    );
  }

  // --- refresh -----------------------------------------------------------------------------------------
  function ensureImages() {
    const url = v.image;
    if (url !== mapImgUrl) {
      mapImgUrl = url;
      mapImg = null;
      if (url) loadImage(url).then((img) => { if (mapImgUrl === url) { mapImg = img; paint(); } });
    }
    for (const k of v.tokens) {
      if (k.img && !thumbs.has(k.img)) {
        thumbs.set(k.img, null);
        loadImage(k.img).then((img) => { if (img) { thumbs.set(k.img, img); paint(); } });
      }
    }
  }
  function refresh() {
    v = readView(transport.doc());
    ensureImages();
    emptyEl.textContent = v.map ? '' : isHost ? t('noMapHost') : t('noMap');
    emptyEl.hidden = !!v.map;
    for (const s of sections) {
      const sig = s.sig();
      if (sig !== s.last) {
        s.last = sig;
        s.build(s.body);
      }
    }
    renderRolls();
    if (v.map && !ui.fitted && cw) fit();
    else paint();
  }

  buildToolbar();
  const off = transport.onChange(refresh);
  const ro = new ResizeObserver(resize);
  ro.observe(stage);
  resize();
  refresh();

  return {
    refresh,
    destroy() {
      off();
      ro.disconnect();
    },
    // For tests and the live spec: where a token is on screen.
    screenOf(tokenId) {
      const k = v.tokens.find((x) => x.id === tokenId);
      if (!k || !v.map) return null;
      const c = cellCenter(v.map, k.c, k.r);
      const r = canvas.getBoundingClientRect();
      return { x: r.left + c.x * ui.view.scale + ui.view.tx, y: r.top + c.y * ui.view.scale + ui.view.ty };
    },
    cellScreen(c, r) {
      const p = cellCenter(v.map, c, r);
      const b = canvas.getBoundingClientRect();
      return { x: b.left + p.x * ui.view.scale + ui.view.tx, y: b.top + p.y * ui.view.scale + ui.view.ty };
    },
    distance: (a, b) => cellDistance(v.map, a, b),
    seedHex: seedToHex,
  };
}
