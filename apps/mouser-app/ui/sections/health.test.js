import assert from 'node:assert/strict';
import test from 'node:test';
import { create } from './health.js';
import { click, flush, labelled, mount } from '../test_helpers.js';
import {
  ME, STATUS_FIXTURES, STUDIO, STUDIO_MAC, TOO_OLD, WINDOWS, WINDOWS_PC, LINUX, makeStatus, problem,
} from '../status_fixture.js';

const start = (status, answers = {}) => mount({ create }, { answers, status });
const entry = (root, id) => root.querySelector(`[data-computer="${id}"]`);
const lines = (node, selector) => [...node.querySelectorAll(selector)].map((n) => n.textContent);

test('Health has one entry per computer with its connection', async () => {
  const { root } = await start(STATUS_FIXTURES.twoMacsWindowsLinux);
  const entries = [...root.querySelectorAll('[data-computer]')];
  assert.deepEqual(entries.map((e) => e.querySelector('.name').textContent), [
    'Desk Mac (this computer)',
    'Studio Mac',
    'Win PC',
    'Linux Box',
  ]);
  assert.equal(entry(root, STUDIO).querySelector('.connection').textContent, 'Connected');
  assert.equal(entry(root, ME).querySelector('.connection').textContent, 'This computer');
});

test('responsiveness is in whole milliseconds, e.g. "Responds quickly (9 ms)"', async () => {
  const { root } = await start(makeStatus({ desk: STATUS_FIXTURES.alone.desk, computers: [STUDIO_MAC, { ...STUDIO_MAC, id: LINUX, name: 'Mid', rtt_ms: 61.4 }, { ...WINDOWS_PC, rtt_ms: 250.6 }] }));
  assert.equal(entry(root, STUDIO).querySelector('.rtt').textContent, 'Responds quickly (9 ms)');
  assert.equal(entry(root, LINUX).querySelector('.rtt').textContent, 'Responds a little slowly (61 ms)');
  assert.equal(entry(root, WINDOWS).querySelector('.rtt').textContent, 'Responds slowly (251 ms)');
  assert.equal(entry(root, ME).querySelector('.rtt'), null, 'this computer has no round trip to itself');
});

test("this computer's permissions are each granted or not, with Open settings for the missing one", async () => {
  const status = makeStatus({
    desk: STATUS_FIXTURES.alone.desk,
    permissions: { accessibility: 'granted', input_monitoring: 'missing', screen_recording: 'not_applicable' },
    problems: [problem('missing_permission:input_monitoring', 'Needs Input Monitoring.', 'open_system_settings:input_monitoring')],
  });
  const { win, api, root } = await start(status);
  const mine = entry(root, ME);
  assert.deepEqual(lines(mine, '.permission'), ['Accessibility: Granted', 'Input Monitoring: Missing Open settings']);
  assert.deepEqual([...mine.querySelectorAll('.permission button')].map((b) => b.textContent), ['Open settings']);
  click(win, mine.querySelector('.permission button'));
  await flush();
  assert.deepEqual(api.calls, [['open_permission_settings', 'input_monitoring']]);
  assert.deepEqual(lines(mine, '.problem .message'), [], 'the missing permission is not listed a second time as a problem');
});

test('a peer shows no permissions of its own', async () => {
  const { root } = await start(STATUS_FIXTURES.twoMacsWindowsLinux);
  assert.equal(entry(root, STUDIO).querySelectorAll('.permission').length, 0);
});

test('a problem appears once, in its computer\'s entry and no other, with its fix as one action', async () => {
  const { win, api, root } = await start(STATUS_FIXTURES.elevatedApp, { 'elevation.restart': {} });
  assert.deepEqual(lines(entry(root, WINDOWS), '.problem .message'), [
    'Portable Mouser cannot control VMware Workstation, which runs as administrator on Win PC.',
  ]);
  assert.equal(entry(root, STUDIO).querySelectorAll('.problem').length, 0);
  assert.equal(entry(root, ME).querySelectorAll('.problem').length, 0);
  assert.equal(root.querySelectorAll('.problem').length, 1);
  const buttons = entry(root, WINDOWS).querySelectorAll('.problem button');
  assert.deepEqual([...buttons].map((b) => b.textContent), ['Restart Mouser as administrator']);
  click(win, buttons[0]);
  await flush();
  assert.deepEqual(api.calls, [['elevation.restart', { computer: WINDOWS }]]);
});

test('the same problem id on two computers is two problems, each under its own computer', async () => {
  const status = makeStatus({
    desk: STATUS_FIXTURES.alone.desk,
    computers: [STUDIO_MAC, WINDOWS_PC],
    problems: [problem('peer_offline:x', 'It is offline.', null, STUDIO), problem('peer_offline:x', 'It is offline.', null, WINDOWS)],
  });
  const { root } = await start(status);
  assert.equal(entry(root, STUDIO).querySelectorAll('.problem').length, 1);
  assert.equal(entry(root, WINDOWS).querySelectorAll('.problem').length, 1);
});

test('a version that cannot work says which computer to update and offers the update as one action', async () => {
  const { win, api, root } = await start(STATUS_FIXTURES.incompatiblePeer, { 'update.apply': {} });
  const mine = entry(root, TOO_OLD);
  assert.match(mine.querySelector('.problem .message').textContent, /Update Ancient PC\./);
  click(win, labelled(mine, 'Update this computer'));
  await flush();
  assert.deepEqual(api.calls, [['update.apply', { computer: TOO_OLD }]]);
});

test('"What doesn\'t work here" lists the limitations of that computer only', async () => {
  const { root } = await start(STATUS_FIXTURES.twoMacsWindowsLinux);
  assert.equal(entry(root, LINUX).querySelector('h3').textContent, "What doesn't work here");
  assert.deepEqual(lines(entry(root, LINUX), '.limitation'), ["Mouser cannot control this computer's login screen."]);
  assert.deepEqual(lines(entry(root, WINDOWS), '.limitation'), [
    "Mouser cannot send Ctrl-Alt-Del to this computer's policy-locked screen.",
  ]);
  assert.equal(entry(root, STUDIO).querySelector('.limitations'), null);
  assert.equal(entry(root, ME).querySelector('h3'), null);
  assert.equal(root.querySelectorAll('.limitation').length, 2, 'never repeated under another computer');
});

test('#health/<id> scrolls to that computer and highlights it, and only it', async () => {
  const { section, root } = await start(STATUS_FIXTURES.twoMacsWindowsLinux);
  let scrolled = null;
  entry(root, WINDOWS).scrollIntoView = () => {
    scrolled = WINDOWS;
  };
  section.open(WINDOWS);
  assert.equal(scrolled, WINDOWS);
  assert.deepEqual([...root.querySelectorAll('.highlight')].map((n) => n.getAttribute('data-computer')), [WINDOWS]);
  section.open(STUDIO);
  assert.deepEqual([...root.querySelectorAll('.highlight')].map((n) => n.getAttribute('data-computer')), [STUDIO]);
  section.update(STATUS_FIXTURES.twoMacsWindowsLinux);
  assert.deepEqual([...root.querySelectorAll('.highlight')].map((n) => n.getAttribute('data-computer')), [STUDIO], 'a status update keeps the highlight');
});

test('a fix that fails says why inside its problem', async () => {
  const status = makeStatus({
    desk: STATUS_FIXTURES.alone.desk,
    computers: [STUDIO_MAC],
    problems: [problem('join_refused:b', 'It refused.', `keep_placement:${STUDIO}`, STUDIO, { kind: 'join_refused' })],
  });
  const { win, root } = await start(status, { 'layout.keep_placement': new Error('No.') });
  click(win, labelled(entry(root, STUDIO), 'Keep this placement'));
  await flush();
  assert.equal(entry(root, STUDIO).querySelector('.problem .error').textContent, 'No.');
});

test('a problem the engine lists twice for one computer appears once', async () => {
  const twice = problem('peer_offline:x', 'It is offline.', null, STUDIO);
  const { root } = await start(makeStatus({ desk: STATUS_FIXTURES.alone.desk, computers: [STUDIO_MAC], problems: [twice, { ...twice }] }));
  assert.equal(root.querySelectorAll('.problem').length, 1);
});
