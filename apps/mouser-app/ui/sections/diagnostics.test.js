import assert from 'node:assert/strict';
import test from 'node:test';
import { create } from './diagnostics.js';
import { click, flush, labelled, mount } from '../test_helpers.js';
import { makeStatus } from '../status_fixture.js';

const BUNDLE = {
  path: '/Users/me/Downloads/mouser-logs-Home-office-20261027T100000Z.zip',
  computers: [
    { id: 'a', name: 'Desk Mac', included: true },
    { id: 'b', name: 'Studio Mac', included: false, reason: 'offline' },
    { id: 'c', name: 'Win PC', included: false, reason: 'logging_off' },
    { id: 'd', name: 'Old Mac', included: false, reason: 'older_version' },
    { id: 'e', name: 'Linux Box', included: true },
  ],
};
const start = (logging = { enabled: false, path: '' }, answers = {}) =>
  mount({ create }, { answers: { collect_logs: BUNDLE, 'logging.set': { path: '/logs/mouser.log' }, ...answers }, status: makeStatus({ logging }) });
const toggle = (win, input, checked) => {
  input.checked = checked;
  input.dispatchEvent(new win.Event('change', { bubbles: true }));
};

test('the logging switch shows whether logging is on and calls logging.set', async () => {
  const { win, api, root } = await start();
  const box = root.querySelector('[data-setting="logging"]');
  assert.equal(box.checked, false);
  assert.equal(labelled(root, 'Open log location').hidden, true);
  toggle(win, box, true);
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'logging.set'), ['logging.set', { enabled: true }]);
  assert.equal(labelled(root, 'Open log location').hidden, false);
  toggle(win, box, false);
  await flush();
  assert.equal(labelled(root, 'Open log location').hidden, true);
});

test('"Open log location" reveals the log folder', async () => {
  const { win, api, root } = await start({ enabled: true, path: '/logs/mouser.log' });
  click(win, labelled(root, 'Open log location'));
  await flush();
  assert.deepEqual(api.calls.at(-1), ['reveal_in_file_manager', '/logs/mouser.log']);
});

test('after switching on, "Open log location" opens the log that was just started', async () => {
  const { win, api, root } = await start();
  toggle(win, root.querySelector('[data-setting="logging"]'), true);
  await flush();
  click(win, labelled(root, 'Open log location'));
  await flush();
  assert.deepEqual(api.calls.at(-1), ['reveal_in_file_manager', '/logs/mouser.log']);
});

test('"Collect logs from the desk" shows one line per computer and reveals the bundle', async () => {
  const { win, api, root } = await start();
  click(win, labelled(root, 'Collect logs from the desk'));
  await flush();
  assert.deepEqual([...root.querySelectorAll('.collected li')].map((n) => n.textContent), [
    'Desk Mac: included',
    'Studio Mac: not included (offline)',
    'Win PC: not included (logging is off)',
    'Old Mac: not included (older version)',
    'Linux Box: included',
  ]);
  assert.equal(root.querySelector('.bundle-path').textContent, `Saved to ${BUNDLE.path}`);
  assert.deepEqual(api.calls.slice(-2), [['collect_logs'], ['reveal_in_file_manager', BUNDLE.path]]);
});

test('a bundle that cannot be written says why', async () => {
  const { win, root } = await start(undefined, { collect_logs: new Error('The Downloads folder is not writable.') });
  click(win, labelled(root, 'Collect logs from the desk'));
  await flush();
  assert.equal(root.querySelector('.error').textContent, 'The Downloads folder is not writable.');
  assert.equal(root.querySelectorAll('.collected li').length, 0);
});

test('#diagnostics/logging focuses the logging switch', async () => {
  const { doc, section, root } = await start();
  section.open('logging');
  assert.equal(doc.activeElement, root.querySelector('[data-setting="logging"]'));
});
