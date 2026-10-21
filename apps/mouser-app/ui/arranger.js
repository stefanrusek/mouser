// The display arranger: every display of the desk, to scale, draggable into
// the shape of the physical desk. Desk space is in millimetres.

import { button, el, guarded, show } from './dom.js';

const CANVAS_WIDTH = 560;
const MAX_SCALE = 0.5;
const MIN_PAD = 200;
// A drop within this many millimetres of an edge snaps to it.
const SNAP_MM = 40;
// Pointer travel under this many pixels is a click, not a drag.
const CLICK_PX = 3;
const MM_PER_96DPI_PX = 25.4 / 96;

const keyOf = (displayId) => JSON.stringify(displayId);

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

export async function init({ doc, api, every, onFocus }) {
  const errorLine = doc.getElementById('error');
  const refusal = doc.getElementById('refusal');
  const canvas = doc.getElementById('canvas');
  const details = doc.getElementById('details');
  const state = { layout: null, view: null, selected: null, undo: null, drag: null };

  const includedDisplays = () => state.layout.displays.filter((d) => d.included);
  const shown = () => includedDisplays().find((d) => keyOf(d.display_id) === state.selected);
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

  function renderTile(display) {
    const rect = rectOf(display);
    const { scale, minX, minY, pad } = state.view;
    const key = keyOf(display.display_id);
    const tile = el(
      doc,
      'div',
      { class: 'tile', tabindex: '0', 'data-key': key },
      el(doc, 'strong', {}, display.computer_name),
      el(doc, 'span', {}, display.name),
    );
    if (display.needs_review) {
      tile.classList.add('review');
      tile.append(el(doc, 'em', {}, 'Review placement'));
    }
    if (key === state.selected) tile.classList.add('selected');
    tile.style.left = `${(rect.x - minX + pad) * scale}px`;
    tile.style.top = `${(rect.y - minY + pad) * scale}px`;
    tile.style.width = `${rect.w * scale}px`;
    tile.style.height = `${rect.h * scale}px`;
    const hover = () => showDetails(display);
    tile.addEventListener('mouseenter', hover);
    tile.addEventListener('focus', hover);
    tile.addEventListener('mouseleave', () => showDetails(shown()));
    tile.addEventListener('pointerdown', (event) => {
      state.drag = {
        display,
        tile,
        rect,
        originX: event.clientX,
        originY: event.clientY,
        left: Number.parseFloat(tile.style.left),
        top: Number.parseFloat(tile.style.top),
      };
    });
    return tile;
  }

  function renderReview(flagged) {
    const list = doc.getElementById('review');
    show(list, flagged.length > 0);
    list.replaceChildren(
      ...flagged.map((display) =>
        el(
          doc,
          'li',
          {},
          el(doc, 'span', { class: 'what' }, `${display.computer_name}: ${display.name}`),
          button(doc, 'Review placement', () => select(display)),
        ),
      ),
    );
  }

  function render() {
    const displays = includedDisplays();
    show(doc.getElementById('empty'), displays.length === 0);
    show(canvas, displays.length > 0);
    show(doc.getElementById('undo-bar'), state.undo !== null);
    renderReview(displays.filter((d) => d.needs_review));
    doc.getElementById('equalize').checked = state.layout.equalize_dimensions;
    if (displays.length === 0) {
      canvas.replaceChildren();
      showDetails(null);
      return;
    }
    state.view = fitView(displays.map(rectOf));
    canvas.style.width = `${state.view.width}px`;
    canvas.style.height = `${state.view.height}px`;
    canvas.replaceChildren(...displays.map(renderTile));
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
    doc.getElementById('undo-text').textContent = `${display.name} was removed.`;
    render();
  });

  doc.getElementById('undo').addEventListener(
    'click',
    guarded(errorLine, async () => {
      const { before, display } = state.undo;
      state.undo = null;
      await save({ ...before, displays: [{ display_id: display.display_id, included: true }] });
    }),
  );

  doc.getElementById('equalize').addEventListener(
    'change',
    guarded(errorLine, (event) => save(layoutParams(state.layout, { equalize_dimensions: event.target.checked }))),
  );

  const drop = guarded(errorLine, async (display, rect) => {
    const key = keyOf(display.display_id);
    const others = includedDisplays()
      .filter((d) => keyOf(d.display_id) !== key)
      .map(rectOf);
    const snapped = snapPlacement(rect, others);
    if (snapped.reason) {
      refusal.textContent = REFUSALS[snapped.reason](display.name);
      render();
      return;
    }
    const was = rectOf(display);
    if (snapped.x === was.x && snapped.y === was.y) {
      render();
      return;
    }
    state.undo = null;
    await save(
      layoutParams(state.layout, {
        placements: state.layout.placements.map((p) =>
          keyOf(p.display_id) === key ? { ...p, x: snapped.x, y: snapped.y } : p,
        ),
      }),
    );
  });

  doc.addEventListener('pointermove', (event) => {
    const { drag } = state;
    if (!drag) return;
    drag.tile.style.left = `${drag.left + event.clientX - drag.originX}px`;
    drag.tile.style.top = `${drag.top + event.clientY - drag.originY}px`;
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
    drop(drag.display, {
      ...drag.rect,
      x: Math.round(drag.rect.x + dx / scale),
      y: Math.round(drag.rect.y + dy / scale),
    });
  });

  const tick = guarded(errorLine, () => (state.drag ? undefined : load()), { clear: false });
  await api.onNotification((note) => {
    if (note.method === 'status') tick();
  });
  every(3000, tick);
  onFocus(tick);
  await guarded(errorLine, load)();
  return { tick };
}
