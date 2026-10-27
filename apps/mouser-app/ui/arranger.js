// The display arranger: every display of the desk, to scale; computers (with
// all their displays, which keep the OS arrangement) are draggable into the
// shape of the physical desk. Desk space is in millimetres.

import { button, el, guarded, show } from './dom.js';
import { mountEdges, parseBlockedEdge } from './arranger_edges.js';

const CANVAS_WIDTH = 560;
const MAX_SCALE = 0.5;
const MIN_PAD = 200;
// A drop within this many millimetres of an edge snaps to it.
const SNAP_MM = 40;
// Pointer travel under this many pixels is a click, not a drag.
const CLICK_PX = 3;
const MM_PER_96DPI_PX = 25.4 / 96;

const keyOf = (displayId) => JSON.stringify(displayId);

// A tile narrower or lower than this (in pixels) cannot hold its names.
const MIN_TILE_PX = [110, 48];
const LEADER_PX = 14;

const fullName = (display) => `${display.computer_name}: ${display.name}`;

// A display's size on the desk: its physical size, else its logical pixels at 96 dpi.
export function deskSize(display) {
  const [w, h] = display.physical_size_mm ?? [
    display.logical_size[0] * MM_PER_96DPI_PX,
    display.logical_size[1] * MM_PER_96DPI_PX,
  ];
  return [Math.round(w), Math.round(h)];
}

// How the rectangles map onto the canvas: padded so there is room to drag outward.
export function fitView(rects) {
  const minX = Math.min(...rects.map((r) => r.x));
  const minY = Math.min(...rects.map((r) => r.y));
  const spanX = Math.max(...rects.map((r) => r.x + r.w)) - minX;
  const spanY = Math.max(...rects.map((r) => r.y + r.h)) - minY;
  const pad = Math.max(MIN_PAD, Math.max(...rects.map((r) => Math.max(r.w, r.h))) / 2);
  const scale = Math.min(MAX_SCALE, CANVAS_WIDTH / (spanX + 2 * pad));
  return { minX, minY, pad, scale, width: (spanX + 2 * pad) * scale, height: (spanY + 2 * pad) * scale };
}

const overlaps = (a, b) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
const clamp = (value, low, high) => Math.min(Math.max(value, low), high);

// Where along a shared edge to sit: aligned with the other display's start or
// end when close, else wherever the drop was (kept touching).
function along(start, length, otherStart, otherLength) {
  const free = clamp(start, otherStart - length + 1, otherStart + otherLength - 1);
  if (Math.abs(free - otherStart) <= SNAP_MM) return otherStart;
  const end = otherStart + otherLength - length;
  return Math.abs(free - end) <= SNAP_MM ? end : free;
}

// The position `rect` snaps to among `others`: flush against one of them, or
// why it cannot: { reason: 'overlap' | 'disconnected' }.
export function snapPlacement(rect, others) {
  if (others.length === 0) return { x: rect.x, y: rect.y };
  const { w, h } = rect;
  const candidates = others.flatMap((o) => {
    const y = along(rect.y, h, o.y, o.h);
    const x = along(rect.x, w, o.x, o.w);
    return [
      { x: o.x + o.w, y, gap: Math.abs(rect.x - (o.x + o.w)), slide: Math.abs(rect.y - y) },
      { x: o.x - w, y, gap: Math.abs(rect.x - (o.x - w)), slide: Math.abs(rect.y - y) },
      { x, y: o.y + o.h, gap: Math.abs(rect.y - (o.y + o.h)), slide: Math.abs(rect.x - x) },
      { x, y: o.y - h, gap: Math.abs(rect.y - (o.y - h)), slide: Math.abs(rect.x - x) },
    ];
  });
  const near = candidates.filter((c) => c.gap <= SNAP_MM).sort((a, b) => a.gap + a.slide - (b.gap + b.slide));
  const free = near.find((c) => !others.some((o) => overlaps({ x: c.x, y: c.y, w, h }, o)));
  if (free) return { x: free.x, y: free.y };
  return { reason: near.length > 0 || others.some((o) => overlaps(rect, o)) ? 'overlap' : 'disconnected' };
}

// Where the whole group of rects `group` (a computer's displays) lands when it
// is dragged by (dx, dy) among `others`: moved as one, with one of its
// members snapped flush against another computer's display, or why it cannot
// be: { reason: 'overlap' | 'disconnected' }.
export function snapGroup(group, dx, dy, others) {
  const clear = (shift) =>
    !group.some((m) => others.some((o) => overlaps({ ...m, x: m.x + shift.x, y: m.y + shift.y }, o)));
  const options = group.map((m) => {
    const moved = { ...m, x: m.x + dx, y: m.y + dy };
    const snapped = snapPlacement(moved, others);
    return snapped.reason ? snapped : { x: snapped.x - m.x, y: snapped.y - m.y };
  });
  const free = options
    .filter((o) => !o.reason && clear(o))
    .sort((a, b) => Math.hypot(a.x - dx, a.y - dy) - Math.hypot(b.x - dx, b.y - dy));
  if (free.length > 0) return { x: free[0].x, y: free[0].y };
  return { reason: options.some((o) => o.reason !== 'disconnected') ? 'overlap' : 'disconnected' };
}

// What the details panel says about a display.
export function detailLines(display) {
  const [width, height] = display.pixel_size;
  return [
    display.name,
    `On ${display.computer_name}`,
    `${width} × ${height} pixels`,
    `Scale ${display.scale}×`,
    display.online ? 'Online' : display.last_seen ? `Offline, last seen ${display.last_seen}` : 'Offline',
  ];
}

const REFUSALS = {
  overlap: (name) => `${name} would overlap another display, so it went back.`,
  disconnected: (name) => `${name} has to touch another display along an edge, so it went back.`,
};

// The `layout.set` params for a layout as `layout.get` returned it, with edits.
function layoutParams(layout, edits = {}) {
  const { placements, edge_options, equalize_dimensions, block_fullscreen } = layout;
  return { placements, edge_options, equalize_dimensions, block_fullscreen, ...edits };
}

export async function init({ doc, api, root = doc, onLayout = () => {} }) {
  const errorLine = root.querySelector('#error');
  const refusal = root.querySelector('#refusal');
  const canvas = root.querySelector('#canvas');
  const details = root.querySelector('#details');
  const state = { layout: null, view: null, selected: null, undo: null, drag: null };

  const edges = mountEdges({
    doc,
    root,
    canvas,
    change: guarded(errorLine, (edits) => save(layoutParams(state.layout, edits))),
    named: (displayId) => state.layout.displays.find((d) => keyOf(d.display_id) === keyOf(displayId)).name,
  });
  const includedDisplays = () => state.layout.displays.filter((d) => d.included);
  const shown = () => includedDisplays().find((d) => keyOf(d.display_id) === state.selected);
  // A computer's displays move together: the OS decides how they sit.
  const groupOf = (display) =>
    includedDisplays().filter((d) => d.display_id.computer === display.display_id.computer);
  const rectOf = (display) => {
    const at = state.layout.placements.find((p) => keyOf(p.display_id) === keyOf(display.display_id));
    const [w, h] = deskSize(display);
    return { x: at.x, y: at.y, w, h };
  };

  function showDetails(display) {
    if (!display) {
      details.replaceChildren(el(doc, 'p', { class: 'hint' }, 'Point at a display to see its details.'));
      return;
    }
    const [name, ...rest] = detailLines(display);
    details.replaceChildren(
      el(doc, 'h3', {}, name),
      ...rest.map((line) => el(doc, 'p', {}, line)),
      button(doc, 'Remove this display', () => removeDisplay(display), { class: 'danger' }),
    );
  }

  function select(display) {
    state.selected = keyOf(display.display_id);
    for (const tile of canvas.children) {
      tile.classList.toggle('selected', tile.getAttribute('data-key') === state.selected);
    }
    showDetails(display);
  }

  // The names of a tile too small to hold them, beside it with a line leading to it.
  function externalLabel(display, tile) {
    const label = el(doc, 'div', { class: 'external-label', title: fullName(display) }, el(doc, 'span', { class: 'leader' }), fullName(display));
    label.style.left = `${Number.parseFloat(tile.style.left) + Number.parseFloat(tile.style.width) + LEADER_PX}px`;
    label.style.top = tile.style.top;
    return label;
  }

  function renderTile(display) {
    const rect = rectOf(display);
    const { scale, minX, minY, pad } = state.view;
    const key = keyOf(display.display_id);
    const tile = el(
      doc,
      'div',
      {
        class: 'tile',
        tabindex: '0',
        'data-key': key,
        'aria-label': fullName(display),
        title: fullName(display),
      },
      el(doc, 'strong', {}, display.computer_name),
      el(doc, 'span', {}, display.name),
    );
    if (display.needs_review) tile.classList.add('review');
    if (key === state.selected) tile.classList.add('selected');
    tile.style.left = `${(rect.x - minX + pad) * scale}px`;
    tile.style.top = `${(rect.y - minY + pad) * scale}px`;
    tile.style.width = `${rect.w * scale}px`;
    tile.style.height = `${rect.h * scale}px`;
    const small = rect.w * scale < MIN_TILE_PX[0] || rect.h * scale < MIN_TILE_PX[1];
    tile.classList.toggle('small', small);
    const hover = () => showDetails(display);
    tile.addEventListener('mouseenter', hover);
    tile.addEventListener('focus', hover);
    tile.addEventListener('mouseleave', () => showDetails(shown()));
    tile.addEventListener('pointerdown', (event) => {
      state.drag = {
        display,
        rect,
        originX: event.clientX,
        originY: event.clientY,
        tiles: groupOf(display).map((d) => {
          const own = [...canvas.children].find((t) => t.getAttribute('data-key') === keyOf(d.display_id));
          return { tile: own, left: Number.parseFloat(own.style.left), top: Number.parseFloat(own.style.top) };
        }),
      };
    });
    return small ? [tile, externalLabel(display, tile)] : [tile];
  }

  function render() {
    onLayout(state.layout);
    const displays = includedDisplays();
    show(root.querySelector('#empty'), displays.length === 0);
    show(canvas, displays.length > 0);
    show(root.querySelector('#undo-bar'), state.undo !== null);
    root.querySelector('#equalize').checked = state.layout.equalize_dimensions;
    root.querySelector('#block-fullscreen').checked = state.layout.block_fullscreen;
    if (displays.length === 0) {
      canvas.replaceChildren();
      showDetails(null);
      return;
    }
    state.view = fitView(displays.map(rectOf));
    canvas.style.width = `${state.view.width}px`;
    canvas.style.height = `${state.view.height}px`;
    canvas.replaceChildren(...displays.flatMap(renderTile));
    edges.draw(
      displays.map((d) => ({ display_id: d.display_id, ...rectOf(d) })),
      state.view,
      state.layout.edge_options,
    );
    showDetails(shown());
  }

  async function load() {
    state.layout = await api.call('layout.get');
    if (!shown()) state.selected = null;
    render();
  }

  // Send a layout; if the engine refuses it, show its sentence and reload what is true.
  async function save(params) {
    refusal.textContent = '';
    try {
      state.layout = await api.call('layout.set', params);
    } catch (error) {
      await load();
      throw error;
    }
    render();
  }

  const removeDisplay = guarded(errorLine, async (display) => {
    const before = layoutParams(state.layout);
    const key = keyOf(display.display_id);
    await save(
      layoutParams(state.layout, {
        placements: before.placements.filter((p) => keyOf(p.display_id) !== key),
        displays: [{ display_id: display.display_id, included: false }],
      }),
    );
    state.undo = { before, display };
    root.querySelector('#undo-text').textContent = `${display.name} was removed.`;
    render();
  });

  root.querySelector('#undo').addEventListener(
    'click',
    guarded(errorLine, async () => {
      const { before, display } = state.undo;
      state.undo = null;
      await save({ ...before, displays: [{ display_id: display.display_id, included: true }] });
    }),
  );

  root.querySelector('#equalize').addEventListener(
    'change',
    guarded(errorLine, (event) => save(layoutParams(state.layout, { equalize_dimensions: event.target.checked }))),
  );

  root.querySelector('#block-fullscreen').addEventListener(
    'change',
    guarded(errorLine, (event) => save(layoutParams(state.layout, { block_fullscreen: event.target.checked }))),
  );

  const drop = guarded(errorLine, async (display, dx, dy) => {
    const group = groupOf(display);
    const others = includedDisplays()
      .filter((d) => d.display_id.computer !== display.display_id.computer)
      .map(rectOf);
    const shift = snapGroup(group.map(rectOf), dx, dy, others);
    if (shift.reason) {
      const name = group.length > 1 ? display.computer_name : display.name;
      refusal.textContent = REFUSALS[shift.reason](name);
      render();
      return;
    }
    if (shift.x === 0 && shift.y === 0) {
      render();
      return;
    }
    state.undo = null;
    const moving = new Set(group.map((d) => keyOf(d.display_id)));
    await save(
      layoutParams(state.layout, {
        placements: state.layout.placements.map((p) =>
          moving.has(keyOf(p.display_id)) ? { ...p, x: p.x + shift.x, y: p.y + shift.y } : p,
        ),
      }),
    );
  });

  doc.addEventListener('pointermove', (event) => {
    const { drag } = state;
    if (!drag) return;
    for (const { tile, left, top } of drag.tiles) {
      tile.style.left = `${left + event.clientX - drag.originX}px`;
      tile.style.top = `${top + event.clientY - drag.originY}px`;
    }
  });
  doc.addEventListener('pointerup', (event) => {
    const { drag } = state;
    if (!drag) return;
    state.drag = null;
    const dx = event.clientX - drag.originX;
    const dy = event.clientY - drag.originY;
    if (Math.hypot(dx, dy) < CLICK_PX) {
      select(drag.display);
      return;
    }
    const { scale } = state.view;
    drop(drag.display, Math.round(dx / scale), Math.round(dy / scale));
  });

  // The fix of a "crossing blocked" notice asks for one edge; the layout is current before it is opened.
  const openAskedEdge = async () => {
    const problemId = await api.takeArrangerEdge();
    if (!problemId) return;
    await load();
    edges.open(parseBlockedEdge(problemId));
  };
  const tick = guarded(
    errorLine,
    async () => {
      if (state.drag) return;
      await load();
      await openAskedEdge();
    },
    { clear: false },
  );
  await tick();
  return { tick };
}
