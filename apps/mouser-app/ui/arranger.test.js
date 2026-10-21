import assert from 'node:assert/strict';
import test from 'node:test';
import { deskSize, detailLines, fitView, init, snapPlacement } from './arranger.js';
import { click, fakeApi, flush, loadPage } from './test_helpers.js';
import { makeStatus } from './status_fixture.js';

const id = (computer, key) => ({ computer, key });
const display = (computer, key, name, extra = {}) => ({
  display_id: id(computer, key),
  name,
  pixel_size: [2560, 1440],
  logical_size: [1280, 720],
  scale: 2,
  physical_size_mm: [600, 340],
  origin_local: [0, 0],
  included: true,
  computer_name: computer === 'aa' ? 'Desk Mac' : 'Studio',
  online: computer === 'aa',
  last_seen: computer === 'aa' ? null : '2026-10-20T10:00:00Z',
  ...extra,
});
const place = (display_id, x, y) => ({ display_id, x, y });

const A = display('aa', '1', 'Built-in');
const B = display('bb', '1', 'Studio Display');
const layoutOf = (displays, placements, extra = {}) => ({
  placements,
  edge_options: [],
  equalize_dimensions: false,
  block_fullscreen: true,
  displays,
  ...extra,
});
const TWO = () => layoutOf([A, B], [place(A.display_id, 0, 0), place(B.display_id, 600, 0)]);
const sent = (api) => api.calls.filter(([m]) => m === 'layout.set').map(([, p]) => p);

async function start(layout = TWO(), answers = {}) {
  const win = loadPage('arranger.html');
  const api = fakeApi({
    status: makeStatus(),
    'layout.get': layout,
    'layout.set': (params) => layoutOf(layout.displays.map((d) => ({ ...d, included: params.displays?.find((e) => JSON.stringify(e.display_id) === JSON.stringify(d.display_id))?.included ?? d.included })), params.placements, {
      equalize_dimensions: params.equalize_dimensions,
    }),
    ...answers,
  });
  const timers = [];
  const focus = [];
  const page = await init({
    doc: win.document,
    api,
    every: (ms, tick) => timers.push([ms, tick]),
    onFocus: (handler) => focus.push(handler),
  });
  return { win, doc: win.document, api, page, timers, focus };
}

const tiles = (doc) => [...doc.querySelectorAll('.tile')];
const tileOf = (doc, name) => tiles(doc).find((t) => t.textContent.includes(name));
const px = (node, prop) => Number.parseFloat(node.style[prop]);
const text = (doc, domId) => doc.getElementById(domId).textContent;
const named = (root, label) => [...root.querySelectorAll('button')].find((b) => b.textContent === label);
const pointer = (win, target, type, x, y) =>
  target.dispatchEvent(new win.MouseEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0 }));
// Drag `tile` by (dx, dy) desk millimetres at the window's scale.
function drag(win, doc, tile, scale, dxMm, dyMm) {
  pointer(win, tile, 'pointerdown', 100, 100);
  pointer(win, doc, 'pointermove', 100 + dxMm * scale, 100 + dyMm * scale);
  pointer(win, doc, 'pointerup', 100 + dxMm * scale, 100 + dyMm * scale);
}
const scaleOf = (layout) => fitView(layout.displays.filter((d) => d.included).map((d, i) => ({ x: layout.placements[i].x, y: layout.placements[i].y, ...sizeOf(d) }))).scale;
const sizeOf = (d) => {
  const [w, h] = deskSize(d);
  return { w, h };
};
const S = scaleOf(TWO());

test('every display is drawn to scale, labelled with its computer, and removed ones are not drawn', async () => {
  const hidden = display('aa', '2', 'Old monitor', { included: false });
  const { doc } = await start(layoutOf([A, B, hidden], [place(A.display_id, 0, 0), place(B.display_id, 600, 0)]));
  const [a, b] = tiles(doc);
  assert.equal(tiles(doc).length, 2);
  assert.match(a.textContent, /Desk Mac/);
  assert.match(a.textContent, /Built-in/);
  assert.match(b.textContent, /Studio/);
  assert.match(b.textContent, /Studio Display/);
  assert.ok(Math.abs(px(a, 'width') - 600 * S) < 0.01);
  assert.ok(Math.abs(px(a, 'height') - 340 * S) < 0.01);
  assert.ok(Math.abs(px(b, 'left') - px(a, 'left') - 600 * S) < 0.01);
  assert.equal(px(b, 'top'), px(a, 'top'));
});

test('a display without a known physical size is sized from its logical pixels at 96 dpi', () => {
  assert.deepEqual(deskSize(display('aa', '1', 'x', { physical_size_mm: null, logical_size: [960, 540] })), [254, 143]);
  assert.deepEqual(deskSize(A), [600, 340]);
});

test('fitView pads the layout so displays can be dragged outward', () => {
  const view = fitView([{ x: 0, y: 0, w: 100, h: 100 }]);
  assert.equal(view.pad, 200);
  assert.equal(view.minX, 0);
  assert.equal(view.scale, 0.5);
  assert.equal(view.width, 250);
  assert.equal(view.height, 250);
});

test('dropping next to another display snaps it edge to edge and saves the layout', async () => {
  const { win, doc, api } = await start();
  // Studio Display moved below Built-in, 12 mm and 5 mm off: it snaps flush underneath with the left edges aligned.
  drag(win, doc, tileOf(doc, 'Studio Display'), S, -600 + 12, 345);
  await flush();
  assert.deepEqual(sent(api), [
    {
      placements: [place(A.display_id, 0, 0), place(B.display_id, 0, 340)],
      edge_options: [],
      equalize_dimensions: false,
      block_fullscreen: true,
    },
  ]);
  assert.equal(text(doc, 'refusal'), '');
});

test('a drop within reach of an aligned edge snaps to the alignment', async () => {
  const { win, doc, api } = await start();
  // nothing lines up here; the free position along the shared edge is kept
  drag(win, doc, tileOf(doc, 'Studio Display'), S, -600 + 100, 340 + 20);
  await flush();
  assert.deepEqual(sent(api)[0].placements[1], place(B.display_id, 100, 340));
});

test('a display can be snapped by aligning its bottom edge', () => {
  const others = [{ x: 0, y: 0, w: 600, h: 340 }];
  const snapped = snapPlacement({ x: 590, y: -100, w: 300, h: 440 }, others);
  assert.deepEqual(snapped, { x: 600, y: -100 });
  const bottom = snapPlacement({ x: 600, y: -90, w: 300, h: 420 }, others);
  assert.deepEqual(bottom, { x: 600, y: -80 });
});

test('a drop that leaves nothing adjoining is refused visibly and the display goes back', async () => {
  const { win, doc, api } = await start();
  const b = tileOf(doc, 'Studio Display');
  const before = px(b, 'left');
  drag(win, doc, b, S, 1500, 0);
  await flush();
  assert.deepEqual(sent(api), []);
  assert.equal(text(doc, 'refusal'), 'Studio Display has to touch another display along an edge, so it went back.');
  assert.equal(px(tileOf(doc, 'Studio Display'), 'left'), before);
});

test('a drop on top of another display is refused visibly', async () => {
  const { win, doc, api } = await start();
  drag(win, doc, tileOf(doc, 'Studio Display'), S, -500, 0);
  await flush();
  assert.deepEqual(sent(api), []);
  assert.equal(text(doc, 'refusal'), 'Studio Display would overlap another display, so it went back.');
});

test('a snap that would only land on another display is an overlap too', async () => {
  const wide = display('cc', '1', 'Wide', { physical_size_mm: [1200, 340], computer_name: 'Third' });
  const layout = layoutOf(
    [A, B, wide],
    [place(A.display_id, 0, 0), place(B.display_id, 600, 0), place(wide.display_id, 0, 340)],
  );
  const { win, doc, api } = await start(layout);
  // B is dragged to just under A's right edge, where only Wide is in the way.
  drag(win, doc, tileOf(doc, 'Studio Display'), scaleOf(layout), 0, 300);
  await flush();
  assert.deepEqual(sent(api), []);
  assert.match(text(doc, 'refusal'), /would overlap another display/);
});

test('a lone display can be moved anywhere', () => {
  assert.deepEqual(snapPlacement({ x: 55, y: 66, w: 10, h: 10 }, []), { x: 55, y: 66 });
});

test('a drop where nothing changed saves nothing', async () => {
  const { win, doc, api } = await start();
  drag(win, doc, tileOf(doc, 'Studio Display'), S, 20, 0);
  await flush();
  assert.deepEqual(sent(api), []);
});

test('the engine refusing a layout shows its sentence and the arrangement is reloaded', async () => {
  const { win, doc, api } = await start(TWO(), {
    'layout.set': new Error('Studio Display cannot be reached from the other displays'),
  });
  drag(win, doc, tileOf(doc, 'Studio Display'), S, -600, 340);
  await flush();
  assert.equal(text(doc, 'error'), 'Studio Display cannot be reached from the other displays');
  assert.equal(api.calls.filter(([m]) => m === 'layout.get').length, 2);
});

test('pointer movement without a press, or after release, does nothing', async () => {
  const { win, doc, api } = await start();
  pointer(win, doc, 'pointermove', 5, 5);
  pointer(win, doc, 'pointerup', 5, 5);
  const b = tileOf(doc, 'Studio Display');
  drag(win, doc, b, S, 0, 0);
  pointer(win, doc, 'pointermove', 400, 400);
  await flush();
  assert.deepEqual(sent(api), []);
});

test('pointing at a display shows its resolution, scale, computer and online state', async () => {
  const { win, doc } = await start();
  const a = tileOf(doc, 'Built-in');
  a.dispatchEvent(new win.MouseEvent('mouseenter'));
  const details = doc.getElementById('details');
  assert.match(details.textContent, /Built-in/);
  assert.match(details.textContent, /Desk Mac/);
  assert.match(details.textContent, /2560 × 1440 pixels/);
  assert.match(details.textContent, /Scale 2×/);
  assert.match(details.textContent, /Online/);
  tileOf(doc, 'Studio Display').dispatchEvent(new win.MouseEvent('mouseenter'));
  assert.match(details.textContent, /Offline, last seen 2026-10-20T10:00:00Z/);
  tileOf(doc, 'Studio Display').dispatchEvent(new win.MouseEvent('mouseleave'));
  assert.match(details.textContent, /Point at a display/);
  tileOf(doc, 'Studio Display').dispatchEvent(new win.FocusEvent('focus'));
  assert.match(details.textContent, /Studio Display/);
});

test('detail lines cover an offline computer that has never been seen', () => {
  assert.deepEqual(detailLines(display('bb', '1', 'X', { last_seen: null })).slice(-1), ['Offline']);
});

test('clicking a display keeps its details up after the pointer leaves', async () => {
  const { win, doc } = await start();
  const a = tileOf(doc, 'Built-in');
  pointer(win, a, 'pointerdown', 10, 10);
  pointer(win, doc, 'pointerup', 10, 10);
  a.dispatchEvent(new win.MouseEvent('mouseleave'));
  assert.match(text(doc, 'details'), /Built-in/);
  assert.ok(a.classList.contains('selected'));
});

test('Remove hides a display and Undo brings it back where it was', async () => {
  const { win, doc, api } = await start();
  const b = tileOf(doc, 'Studio Display');
  b.dispatchEvent(new win.MouseEvent('mouseenter'));
  click(win, named(doc.getElementById('details'), 'Remove this display'));
  await flush();
  assert.deepEqual(sent(api)[0], {
    placements: [place(A.display_id, 0, 0)],
    edge_options: [],
    equalize_dimensions: false,
    block_fullscreen: true,
    displays: [{ display_id: B.display_id, included: false }],
  });
  assert.equal(tiles(doc).length, 1);
  assert.ok(!doc.getElementById('undo-bar').hidden);
  assert.match(text(doc, 'undo-text'), /Studio Display was removed/);
  click(win, doc.getElementById('undo'));
  await flush();
  assert.deepEqual(sent(api)[1], {
    placements: [place(A.display_id, 0, 0), place(B.display_id, 600, 0)],
    edge_options: [],
    equalize_dimensions: false,
    block_fullscreen: true,
    displays: [{ display_id: B.display_id, included: true }],
  });
  assert.equal(tiles(doc).length, 2);
  assert.ok(doc.getElementById('undo-bar').hidden);
});

test('moving a display forgets an old undo', async () => {
  const { win, doc } = await start();
  tileOf(doc, 'Studio Display').dispatchEvent(new win.MouseEvent('mouseenter'));
  click(win, named(doc.getElementById('details'), 'Remove this display'));
  await flush();
  drag(win, doc, tileOf(doc, 'Built-in'), S, 0, 30);
  await flush();
  assert.ok(doc.getElementById('undo-bar').hidden);
});

test('removing a display that belongs to another computer shows why', async () => {
  const { win, doc } = await start(TWO(), {
    'layout.set': new Error('Studio Display belongs to another computer; remove it on that computer'),
  });
  tileOf(doc, 'Studio Display').dispatchEvent(new win.MouseEvent('mouseenter'));
  click(win, named(doc.getElementById('details'), 'Remove this display'));
  await flush();
  assert.equal(text(doc, 'error'), 'Studio Display belongs to another computer; remove it on that computer');
  assert.ok(doc.getElementById('undo-bar').hidden);
});

test('Equalize display dimensions toggles the setting through layout.set', async () => {
  const { win, doc, api } = await start();
  const box = doc.getElementById('equalize');
  assert.equal(box.checked, false);
  box.checked = true;
  box.dispatchEvent(new win.Event('change'));
  await flush();
  assert.equal(sent(api)[0].equalize_dimensions, true);
  assert.equal(box.checked, true);
  box.checked = false;
  box.dispatchEvent(new win.Event('change'));
  await flush();
  assert.equal(sent(api)[1].equalize_dimensions, false);
});

test('displays that need review are highlighted and offer Review placement', async () => {
  const flagged = display('bb', '1', 'Studio Display', { needs_review: true });
  const { win, doc } = await start(layoutOf([A, flagged], [place(A.display_id, 0, 0), place(flagged.display_id, 600, 0)]));
  assert.ok(tileOf(doc, 'Studio Display').classList.contains('review'));
  assert.ok(!tileOf(doc, 'Built-in').classList.contains('review'));
  assert.match(tileOf(doc, 'Studio Display').textContent, /Review placement/);
  const review = doc.getElementById('review');
  assert.ok(!review.hidden);
  assert.match(review.textContent, /Studio Display/);
  click(win, named(review, 'Review placement'));
  assert.match(text(doc, 'details'), /Studio Display/);
  assert.ok(tileOf(doc, 'Studio Display').classList.contains('selected'));
});

test('with nothing to review the review list is hidden; with no displays a hint shows', async () => {
  const { doc } = await start();
  assert.ok(doc.getElementById('review').hidden);
  assert.ok(doc.getElementById('empty').hidden);
  const none = await start(layoutOf([], []));
  assert.ok(!none.doc.getElementById('empty').hidden);
  assert.equal(tiles(none.doc).length, 0);
});

test('the arrangement refreshes on status changes, on focus and on a timer, but not mid-drag', async () => {
  const { win, doc, api, timers, focus } = await start();
  assert.equal(timers[0][0], 3000);
  await timers[0][1]();
  await focus[0]();
  api.emit('status', {});
  api.emit('problem', { problem_id: 'p', message: 'm', fix: null, cleared_at: null });
  await flush();
  assert.equal(api.calls.filter(([m]) => m === 'layout.get').length, 4);
  const b = tileOf(doc, 'Studio Display');
  pointer(win, b, 'pointerdown', 0, 0);
  pointer(win, doc, 'pointermove', 40, 0);
  await timers[0][1]();
  assert.equal(api.calls.filter(([m]) => m === 'layout.get').length, 4);
  pointer(win, doc, 'pointerup', 40, 0);
  await flush();
});

test('a first load the engine refuses shows its sentence', async () => {
  const { doc } = await start(TWO(), { 'layout.get': new Error('This computer is not on a desk') });
  assert.equal(text(doc, 'error'), 'This computer is not on a desk');
});
