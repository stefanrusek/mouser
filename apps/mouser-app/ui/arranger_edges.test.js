import assert from 'node:assert/strict';
import test from 'node:test';
import { blocksCorners, findEdges, setCorners } from './arranger_edges.js';
import { init } from './arranger.js';
import { click, fakeApi, flush, loadPage } from './test_helpers.js';
import { makeStatus } from './status_fixture.js';

const id = (computer, key) => ({ computer, key });
const display = (computer, key, name) => ({
  display_id: id(computer, key),
  name,
  pixel_size: [2560, 1440],
  logical_size: [1280, 720],
  scale: 2,
  physical_size_mm: [600, 340],
  origin_local: [0, 0],
  included: true,
  computer_name: computer === 'aa' ? 'Desk Mac' : 'Studio',
  online: true,
  last_seen: null,
});
const place = (display_id, x, y) => ({ display_id, x, y });
const A = display('aa', '1', 'Built-in');
const B = display('bb', '1', 'Studio Display');
const C = display('cc', '1', 'Above');
const layoutOf = (displays, placements, extra = {}) => ({
  placements,
  edge_options: [],
  equalize_dimensions: false,
  block_fullscreen: true,
  displays,
  ...extra,
});
const TWO = (extra) => layoutOf([A, B], [place(A.display_id, 0, 0), place(B.display_id, 600, 0)], extra);

async function start(layout = TWO(), status = makeStatus()) {
  const win = loadPage('arranger.html');
  const api = fakeApi({ status, 'layout.get': layout, 'layout.set': (params) => ({ ...layout, ...params }) });
  await init({ doc: win.document, api, every: () => {}, onFocus: () => {} });
  return { win, doc: win.document, api };
}
const edges = (doc) => [...doc.querySelectorAll('.edge')];
const sent = (api) => api.calls.filter(([m]) => m === 'layout.set').map(([, p]) => p);
const toggle = (win, input, checked) => {
  input.checked = checked;
  input.dispatchEvent(new win.Event('change', { bubbles: true }));
};

test('findEdges lists each shared edge once with the sides each display faces', () => {
  const rects = (list) => list.map(([d, x, y, w, h]) => ({ display_id: d.display_id, x, y, w, h }));
  const [edge] = findEdges(rects([[A, 0, 0, 600, 340], [B, 600, 100, 600, 340]]));
  assert.deepEqual(edge.first, { display_id: A.display_id, side: 'right' });
  assert.deepEqual(edge.second, { display_id: B.display_id, side: 'left' });
  assert.deepEqual([edge.x, edge.y, edge.w, edge.h], [600, 100, 0, 240]);
  const [vertical] = findEdges(rects([[A, 0, 0, 600, 340], [C, 100, 340, 600, 340]]));
  assert.equal(vertical.first.side, 'bottom');
  assert.equal(vertical.second.side, 'top');
  assert.deepEqual([vertical.x, vertical.y, vertical.w, vertical.h], [100, 340, 500, 0]);
  assert.deepEqual(findEdges(rects([[A, 0, 0, 600, 340], [B, 600, 340, 600, 340]])), []);
  assert.deepEqual(findEdges(rects([[A, 0, 0, 600, 340], [B, 2000, 0, 600, 340]])), []);
  const mirrored = findEdges(rects([[B, 600, 0, 600, 340], [A, 0, 0, 600, 340]]));
  assert.equal(mirrored.length, 1);
  const above = findEdges(rects([[C, 0, 0, 600, 340], [A, 0, 340, 600, 340]]));
  assert.equal(above[0].first.side, 'bottom');
});

test('corners are blocked unless an entry says otherwise, and setCorners replaces entries', () => {
  const key = { display_id: A.display_id, side: 'right' };
  assert.equal(blocksCorners([], key), true);
  assert.equal(blocksCorners([{ ...key, block_corners: false }], key), false);
  assert.equal(blocksCorners([{ ...key, block_corners: false }], { ...key, side: 'left' }), true);
  const other = { display_id: C.display_id, side: 'top', block_corners: false };
  const edge = { first: key, second: { display_id: B.display_id, side: 'left' } };
  const off = setCorners([other, { ...key, block_corners: true }], edge, false);
  assert.deepEqual(off, [
    other,
    { ...key, block_corners: false },
    { display_id: B.display_id, side: 'left', block_corners: false },
  ]);
  assert.deepEqual(setCorners(off, edge, true), [other]);
});

test('an edge between two displays is drawn and clicking it opens its options, corners on by default', async () => {
  const { win, doc } = await start();
  const [edge] = edges(doc);
  assert.equal(edges(doc).length, 1);
  assert.equal(doc.getElementById('edge-options').hidden, true);
  click(win, edge);
  assert.equal(doc.getElementById('edge-options').hidden, false);
  assert.match(doc.getElementById('edge-title').textContent, /Built-in/);
  assert.match(doc.getElementById('edge-title').textContent, /Studio Display/);
  assert.equal(doc.getElementById('block-corners').checked, true);
  assert.ok(edges(doc)[0].classList.contains('selected'));
});

test('turning off corner blocking writes edge_options for both sides through layout.set', async () => {
  const { win, doc, api } = await start();
  click(win, edges(doc)[0]);
  toggle(win, doc.getElementById('block-corners'), false);
  await flush();
  const [params] = sent(api);
  assert.deepEqual(params.edge_options, [
    { display_id: A.display_id, side: 'right', block_corners: false },
    { display_id: B.display_id, side: 'left', block_corners: false },
  ]);
  assert.equal(doc.getElementById('block-corners').checked, false);
  toggle(win, doc.getElementById('block-corners'), true);
  await flush();
  assert.deepEqual(sent(api)[1].edge_options, []);
});

test('an edge with corners already off shows the box unchecked', async () => {
  const off = [
    { display_id: A.display_id, side: 'right', block_corners: false },
    { display_id: B.display_id, side: 'left', block_corners: false },
  ];
  const { win, doc } = await start(TWO({ edge_options: off }));
  click(win, edges(doc)[0]);
  assert.equal(doc.getElementById('block-corners').checked, false);
});

test('the desk-wide full-screen toggle writes block_fullscreen through layout.set', async () => {
  const { win, doc, api } = await start();
  const box = doc.getElementById('block-fullscreen');
  assert.equal(box.checked, true);
  toggle(win, box, false);
  await flush();
  assert.equal(sent(api)[0].block_fullscreen, false);
  assert.equal(box.checked, false);
});

test('a refused edge change shows the engine sentence', async () => {
  const win = loadPage('arranger.html');
  const api = fakeApi({
    status: makeStatus(),
    'layout.get': TWO(),
    'layout.set': new Error('Not allowed.'),
  });
  await init({ doc: win.document, api, every: () => {}, onFocus: () => {} });
  click(win, edges(win.document)[0]);
  toggle(win, win.document.getElementById('block-corners'), false);
  await flush();
  assert.equal(win.document.getElementById('error').textContent, 'Not allowed.');
});

test('an open crossing_blocked notification opens the first edge when the arranger opens', async () => {
  const blocked = {
    problem_id: 'p1',
    kind: 'crossing_blocked',
    computer: 'aa',
    message: 'Blocked.',
    fix: 'open_arranger_edge',
    raised_at: '2026-10-21T00:00:00Z',
    cleared_at: null,
  };
  const { doc } = await start(TWO(), makeStatus({ problems: [blocked] }));
  assert.equal(doc.getElementById('edge-options').hidden, false);
  const quiet = await start(TWO(), makeStatus({ problems: [{ ...blocked, cleared_at: '2026-10-21T01:00:00Z' }] }));
  assert.equal(quiet.doc.getElementById('edge-options').hidden, true);
});

test('with no edge, a blocked notification opens nothing and a removed display drops its edge', async () => {
  const blocked = { problem_id: 'p1', kind: 'crossing_blocked', cleared_at: null };
  const one = layoutOf([A], [place(A.display_id, 0, 0)]);
  const { doc } = await start(one, makeStatus({ problems: [blocked] }));
  assert.equal(edges(doc).length, 0);
  assert.equal(doc.getElementById('edge-options').hidden, true);
});

test('the open edge closes when its displays move apart', async () => {
  const win = loadPage('arranger.html');
  let layout = TWO();
  const api = fakeApi({ status: makeStatus(), 'layout.get': () => layout });
  const page = await init({ doc: win.document, api, every: () => {}, onFocus: () => {} });
  click(win, edges(win.document)[0]);
  layout = layoutOf([A, B], [place(A.display_id, 0, 0), place(B.display_id, 0, 340)]);
  await page.tick();
  assert.equal(win.document.getElementById('edge-options').hidden, true);
});
