// Per-edge options in the arranger: the edges two displays share, and the
// "Block crossing in corners" choice for each (Story 5).

import { show } from './dom.js';

const HIT_PX = 10;
const keyOf = (value) => JSON.stringify(value);
const sameSide = (a, b) => keyOf(a.display_id) === keyOf(b.display_id) && a.side === b.side;

// The span two ranges share, or null when they only touch or miss.
function shared(aStart, aEnd, bStart, bEnd) {
  const start = Math.max(aStart, bStart);
  const end = Math.min(aEnd, bEnd);
  return end > start ? [start, end] : null;
}

function edgeBetween(p, q) {
  const side = (a, b, first, second) => ({ first: { display_id: a.display_id, side: first }, second: { display_id: b.display_id, side: second } });
  const rows = shared(p.y, p.y + p.h, q.y, q.y + q.h);
  const columns = shared(p.x, p.x + p.w, q.x, q.x + q.w);
  if (rows && p.x + p.w === q.x) return { ...side(p, q, 'right', 'left'), x: q.x, y: rows[0], w: 0, h: rows[1] - rows[0] };
  if (rows && q.x + q.w === p.x) return { ...side(q, p, 'right', 'left'), x: p.x, y: rows[0], w: 0, h: rows[1] - rows[0] };
  if (columns && p.y + p.h === q.y) return { ...side(p, q, 'bottom', 'top'), x: columns[0], y: q.y, w: columns[1] - columns[0], h: 0 };
  if (columns && q.y + q.h === p.y) return { ...side(q, p, 'bottom', 'top'), x: columns[0], y: p.y, w: columns[1] - columns[0], h: 0 };
  return null;
}

// Every edge shared by two rectangles ({display_id, x, y, w, h}), once each:
// { first, second: {display_id, side}, x, y, w, h } in desk millimetres.
export function findEdges(rects) {
  return rects.flatMap((p, i) => rects.slice(i + 1).map((q) => edgeBetween(p, q)).filter(Boolean));
}

// Corners are blocked unless an entry for that display side says otherwise.
export function blocksCorners(options, target) {
  return options.find((o) => sameSide(o, target))?.block_corners ?? true;
}

// `options` with both sides of `edge` set; the default needs no entry.
export function setCorners(options, edge, block) {
  const rest = options.filter((o) => !sameSide(o, edge.first) && !sameSide(o, edge.second));
  if (block) return rest;
  return [...rest, { ...edge.first, block_corners: false }, { ...edge.second, block_corners: false }];
}

// Wire the edge handles on `canvas` and the options panel. `change(edits)`
// sends a layout change; `named(display_id)` is the display's label.
export function mountEdges({ doc, canvas, change, named }) {
  const panel = doc.getElementById('edge-options');
  const corners = doc.getElementById('block-corners');
  const state = { selected: null, edges: [], options: [] };
  const identity = (edge) => keyOf([edge.first, edge.second]);

  function showPanel() {
    const edge = state.edges.find((e) => identity(e) === state.selected);
    show(panel, Boolean(edge));
    if (!edge) return;
    doc.getElementById('edge-title').textContent = `Edge between ${named(edge.first.display_id)} and ${named(edge.second.display_id)}`;
    corners.checked = blocksCorners(state.options, edge.first);
    for (const handle of canvas.querySelectorAll('.edge')) {
      handle.classList.toggle('selected', handle.getAttribute('data-edge') === state.selected);
    }
  }

  corners.addEventListener('change', () => {
    const edge = state.edges.find((e) => identity(e) === state.selected);
    return change({ edge_options: setCorners(state.options, edge, corners.checked) });
  });

  return {
    // Draw handles for the edges among `rects`, positioned like the tiles.
    draw(rects, view, options) {
      const { scale, minX, minY, pad } = view;
      state.options = options;
      state.edges = findEdges(rects);
      for (const edge of state.edges) {
        const handle = doc.createElement('div');
        handle.className = 'edge';
        handle.setAttribute('data-edge', identity(edge));
        handle.setAttribute('role', 'button');
        handle.setAttribute('tabindex', '0');
        handle.setAttribute('aria-label', 'Edge options');
        handle.style.left = `${(edge.x - minX + pad) * scale - (edge.w === 0 ? HIT_PX / 2 : 0)}px`;
        handle.style.top = `${(edge.y - minY + pad) * scale - (edge.h === 0 ? HIT_PX / 2 : 0)}px`;
        handle.style.width = `${edge.w === 0 ? HIT_PX : edge.w * scale}px`;
        handle.style.height = `${edge.h === 0 ? HIT_PX : edge.h * scale}px`;
        handle.addEventListener('click', () => {
          state.selected = identity(edge);
          showPanel();
        });
        canvas.append(handle);
      }
      showPanel();
    },
    // Open the first edge (the fix of a "crossing blocked" notification).
    openFirst() {
      if (state.edges.length === 0) return;
      state.selected = identity(state.edges[0]);
      showPanel();
    },
  };
}
