import assert from 'node:assert/strict';
import test from 'node:test';
import { create } from './displays.js';
import { click, flush, labelled, mount } from '../test_helpers.js';
import { STATUS_FIXTURES, STUDIO, makeLayout } from '../status_fixture.js';

const start = (layout = makeLayout(), extra = {}) =>
  mount({ create }, {
    answers: { 'layout.get': layout, 'layout.set': (p) => ({ ...layout, ...p }), 'layout.keep_placement': {}, ...extra },
    status: STATUS_FIXTURES.twoMacsWindowsLinux,
  });
const tiles = (root) => [...root.querySelectorAll('.tile')];

test('every display tile shows its computer name and display name in full, and carries both as its accessible name and tooltip', async () => {
  const { root } = await start();
  const labels = tiles(root).map((t) => [t.querySelector('strong').textContent, t.querySelector('span').textContent]);
  assert.deepEqual(labels, [
    ['Desk Mac', 'Built-in'],
    ['Studio Mac', 'Studio Display'],
    ['Win PC', 'DELL U2720Q'],
    ['Linux Box', 'DP-1'],
  ]);
  assert.equal(tiles(root)[1].getAttribute('aria-label'), 'Studio Mac: Studio Display');
  assert.equal(tiles(root)[1].getAttribute('title'), 'Studio Mac: Studio Display');
  assert.equal(root.querySelectorAll('.external-label').length, 0);
});

test('a tile too small for its name gets an external label with a leader line', async () => {
  const layout = makeLayout();
  layout.displays[1].physical_size_mm = [60, 34];
  const { root } = await start(layout);
  const labels = [...root.querySelectorAll('.external-label')];
  assert.equal(labels.length, 1);
  assert.equal(labels[0].textContent, 'Studio Mac: Studio Display');
  assert.equal(labels[0].getAttribute('title'), 'Studio Mac: Studio Display');
  assert.ok(labels[0].querySelector('.leader'));
  const small = tiles(root).find((t) => t.getAttribute('aria-label') === 'Studio Mac: Studio Display');
  assert.equal(small.classList.contains('small'), true);
  assert.equal(small.querySelector('strong').textContent, 'Studio Mac', 'the names stay on the tile for assistive tools');
});

test('desk-wide "Don\'t cross while an app is full screen" and "Equalize display dimensions" and the edge options are here', async () => {
  const { win, api, root } = await start();
  const control = (key) => root.querySelector(`[data-setting="${key}"]`);
  assert.equal(control('block_fullscreen').checked, true);
  assert.equal(control('equalize_dimensions').checked, false);
  assert.ok(control('block_corners'));
  control('equalize_dimensions').checked = true;
  control('equalize_dimensions').dispatchEvent(new win.Event('change', { bubbles: true }));
  await flush();
  assert.equal(api.calls.find(([m]) => m === 'layout.set')[1].equalize_dimensions, true);
});

test('a placement that needs review shows the message with Keep this placement', async () => {
  const { win, api, root } = await start(makeLayout({ review: [STUDIO] }));
  assert.match(root.querySelector('.review-message').textContent, /Studio Mac's displays were placed automatically/);
  click(win, labelled(root, 'Keep this placement'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'layout.keep_placement'), ['layout.keep_placement', { computer: STUDIO }]);
});

test('each status update reloads the arrangement, and a part of the route focuses that control', async () => {
  const { doc, api, section, root } = await start();
  section.update(STATUS_FIXTURES.alone);
  await flush();
  assert.equal(api.calls.filter(([m]) => m === 'layout.get').length, 3, 'once on creation and once per status');
  section.open('block_fullscreen');
  assert.equal(doc.activeElement, root.querySelector('[data-setting="block_fullscreen"]'));
});
