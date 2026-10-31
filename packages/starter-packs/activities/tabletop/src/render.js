// Drawing the table on a canvas. `draw` is a pure function of the model and the view; it holds no state.
// model = { map, image (HTMLImageElement | null), fog, tokens, tracker, rulers, isHost, me, names, thumbs,
//           drag, preview, measure, fogPreview }
// view  = { scale, tx, ty }   image pixel (x, y) is drawn at (x * scale + tx, y * scale + ty)

import { cellCenter, cellCorners, gridDims, measure } from './grid.js';

const css = (name, fallback) =>
  (typeof getComputedStyle === 'function'
    ? getComputedStyle(document.documentElement).getPropertyValue(name).trim()
    : '') || fallback;

function pathCell(ctx, map, c, r, view) {
  const pts = cellCorners(map, c, r);
  ctx.moveTo(pts[0].x * view.scale + view.tx, pts[0].y * view.scale + view.ty);
  for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x * view.scale + view.tx, pts[i].y * view.scale + view.ty);
  ctx.closePath();
}

export function toScreen(view, x, y) {
  return { x: x * view.scale + view.tx, y: y * view.scale + view.ty };
}
export function toImage(view, x, y) {
  return { x: (x - view.tx) / view.scale, y: (y - view.ty) / view.scale };
}

/** The view that fits the whole image in a canvas of the given size. */
export function fitView(map, width, height) {
  const scale = Math.min(width / map.width, height / map.height, 4);
  return { scale, tx: (width - map.width * scale) / 2, ty: (height - map.height * scale) / 2 };
}

/** Whether `viewer` sees a token: the host sees all; others do not see hidden tokens or tokens in fog. */
export function tokenVisible(token, fog, isHost) {
  if (isHost) return true;
  if (token.hidden) return false;
  return !(fog && fog.isHidden(token.c, token.r));
}

function drawToken(ctx, m, view, token, pos, opts) {
  const { map } = m;
  const radius = (map.size / 2) * 0.88 * (token.size ?? 1) * view.scale;
  const p = toScreen(view, pos.x, pos.y);
  ctx.save();
  ctx.globalAlpha = opts.faded ? 0.45 : 1;
  ctx.beginPath();
  ctx.arc(p.x, p.y, radius, 0, Math.PI * 2);
  ctx.fillStyle = token.color || '#888';
  ctx.fill();
  const thumb = m.thumbs.get(token.img);
  if (thumb) {
    ctx.save();
    ctx.clip();
    ctx.drawImage(thumb, p.x - radius, p.y - radius, radius * 2, radius * 2);
    ctx.restore();
  } else {
    ctx.fillStyle = '#fff';
    ctx.font = `600 ${Math.max(radius, 8)}px system-ui, sans-serif`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText([...String(token.name || '?')][0]?.toUpperCase() ?? '?', p.x, p.y + radius * 0.05);
  }
  ctx.lineWidth = opts.ring ? 4 : 2;
  ctx.strokeStyle = opts.ring ? css('--accent', '#f2c14e') : 'rgba(0,0,0,0.65)';
  ctx.beginPath();
  ctx.arc(p.x, p.y, radius, 0, Math.PI * 2);
  ctx.stroke();
  if (opts.mine) {
    ctx.lineWidth = 2;
    ctx.strokeStyle = '#fff';
    ctx.beginPath();
    ctx.arc(p.x, p.y, radius + 3, 0, Math.PI * 2);
    ctx.stroke();
  }
  ctx.restore();
}

export function draw(ctx, w, h, m, view) {
  ctx.clearRect(0, 0, w, h);
  ctx.fillStyle = css('--canvas', '#14161a');
  ctx.fillRect(0, 0, w, h);
  const { map } = m;
  if (!map) return;

  // Image.
  if (m.image) {
    ctx.drawImage(m.image, view.tx, view.ty, map.width * view.scale, map.height * view.scale);
  } else {
    ctx.fillStyle = css('--map-blank', '#2a2f38');
    ctx.fillRect(view.tx, view.ty, map.width * view.scale, map.height * view.scale);
  }

  // Grid.
  const { cols, rows } = gridDims(map);
  ctx.beginPath();
  for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) pathCell(ctx, map, c, r, view);
  ctx.lineWidth = 1;
  ctx.strokeStyle = 'rgba(255,255,255,0.28)';
  ctx.stroke();

  // Fog: opaque for players, see-through for the host (who paints it).
  const fog = m.fogPreview ?? m.fog;
  if (fog) {
    ctx.beginPath();
    for (let r = 0; r < rows; r++)
      for (let c = 0; c < cols; c++) if (fog.isHidden(c, r)) pathCell(ctx, map, c, r, view);
    ctx.fillStyle = m.isHost ? 'rgba(8,10,16,0.55)' : 'rgb(8,10,16)';
    ctx.fill();
  }

  // Tokens (the one being dragged is drawn at the pointer).
  const turnEntry = m.tracker.entries.find((e) => e.id === m.tracker.turn);
  for (const token of m.tokens) {
    if (!tokenVisible(token, fog, m.isHost)) continue;
    const dragged = m.drag && m.drag.id === token.id;
    const pos = dragged ? m.drag.at : cellCenter(map, token.c, token.r);
    drawToken(ctx, m, view, token, pos, {
      faded: token.hidden || dragged,
      ring: turnEntry?.tokenId === token.id,
      mine: token.owner === m.me,
    });
  }
  if (m.preview) {
    ctx.save();
    ctx.lineWidth = 3;
    ctx.setLineDash([6, 4]);
    ctx.strokeStyle = '#fff';
    ctx.beginPath();
    pathCell(ctx, map, m.preview.c, m.preview.r, view);
    ctx.stroke();
    ctx.restore();
  }

  // Rulers: everyone's, each in the member's colour.
  const rulers = { ...m.rulers };
  if (m.measure) rulers[m.me] = m.measure;
  for (const [uid, ru] of Object.entries(rulers)) {
    if (!ru?.a || !ru?.b) continue;
    const a = toScreen(view, ...Object.values(cellCenter(map, ru.a.c, ru.a.r)));
    const b = toScreen(view, ...Object.values(cellCenter(map, ru.b.c, ru.b.r)));
    ctx.save();
    ctx.lineWidth = 3;
    ctx.strokeStyle = uid === m.me ? '#ffe066' : '#66d9ff';
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.stroke();
    for (const e of [a, b]) {
      ctx.beginPath();
      ctx.arc(e.x, e.y, 5, 0, Math.PI * 2);
      ctx.fillStyle = ctx.strokeStyle;
      ctx.fill();
    }
    const d = measure(map, ru.a, ru.b);
    const label = `${d.value} ${d.unitName}`.trim();
    ctx.font = '600 14px system-ui, sans-serif';
    const tw = ctx.measureText(label).width;
    const mx = (a.x + b.x) / 2;
    const my = (a.y + b.y) / 2;
    ctx.fillStyle = 'rgba(0,0,0,0.75)';
    ctx.fillRect(mx - tw / 2 - 5, my - 11, tw + 10, 22);
    ctx.fillStyle = '#fff';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(label, mx, my);
    ctx.restore();
  }
}
