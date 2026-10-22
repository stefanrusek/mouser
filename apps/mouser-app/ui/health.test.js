import assert from 'node:assert/strict';
import test from 'node:test';
import { init, latestProblem, permissionLabel, responsiveness } from './health.js';
import { click, fakeApi, flush, loadPage } from './test_helpers.js';
import { makeStatus, problem } from './status_fixture.js';

const studio = (extra = {}) => ({
  id: 'bb',
  name: 'Studio',
  os: 'windows',
  version: '0.1.0',
  connection: 'connected',
  rtt_ms: 12,
  permissions: {},
  limitations: [],
  displays: 1,
  ...extra,
});
const logging = (enabled, path = '/home/me/logs/mouser.log') => ({ logging: { enabled, path } });

async function start(status, extra = {}) {
  const win = loadPage('health.html');
  const api = fakeApi({ status, 'logging.set': { path: '/home/me/logs/mouser.log' }, ...extra });
  const timers = [];
  const focus = [];
  const page = await init({
    doc: win.document,
    api,
    every: (ms, tick) => timers.push([ms, tick]),
    onFocus: (handler) => focus.push(handler),
  });
  return { win, doc: win.document, api, timers, focus, page };
}
const rows = (doc) => [...doc.querySelectorAll('#computers > li')];
const text = (node, selector) => node.querySelector(selector).textContent;
const full = (overrides = {}) => ({ ...makeStatus({ computers: [studio()] }), ...logging(false), ...overrides });

test('responsiveness is written from the round trip time', () => {
  assert.equal(responsiveness(12), 'Responds quickly (12 ms)');
  assert.equal(responsiveness(80), 'Responds a little slowly (80 ms)');
  assert.equal(responsiveness(300), 'Responds slowly (300 ms)');
});

test('permissions have plain names', () => {
  assert.equal(permissionLabel('accessibility'), 'Accessibility');
  assert.equal(permissionLabel('input_monitoring'), 'Input Monitoring');
  assert.equal(permissionLabel('screen_recording'), 'Screen Recording');
  assert.equal(permissionLabel('full_disk_access'), 'Full Disk Access');
  assert.equal(permissionLabel('input_capture_portal'), 'Input capture');
  assert.equal(permissionLabel('remote_desktop_portal'), 'Remote desktop');
});

test('the most recent open problem of a computer is the one with the latest raised_at', () => {
  const old = { ...problem('a', 'Old.'), raised_at: '2026-10-21T00:00:00Z' };
  const recent = { ...problem('b', 'Recent.'), raised_at: '2026-10-21T01:00:00Z' };
  const closed = { ...problem('c', 'Closed.'), raised_at: '2026-10-21T02:00:00Z', cleared_at: '2026-10-21T03:00:00Z' };
  const theirs = { ...problem('d', 'Theirs.'), computer: 'bb', raised_at: '2026-10-21T04:00:00Z' };
  assert.equal(latestProblem([old, recent, closed, theirs], 'aa').problem_id, 'b');
  assert.equal(latestProblem([closed], 'aa'), undefined);
});

test('each computer is listed with how it is connected and how fast it answers', async () => {
  const { doc } = await start(full());
  const [self, other] = rows(doc);
  assert.equal(text(self, '.name'), 'Desk Mac (this computer)');
  assert.equal(text(self, '.connection'), 'This computer');
  assert.equal(self.querySelector('.rtt'), null);
  assert.equal(text(other, '.name'), 'Studio');
  assert.equal(text(other, '.connection'), 'Connected');
  assert.equal(text(other, '.rtt'), 'Responds quickly (12 ms)');
});

test('reconnecting, offline and incompatible computers are named as such and have no speed', async () => {
  const status = full({
    computers: [
      ...makeStatus().computers,
      studio({ id: 'b1', name: 'One', connection: 'reconnecting', rtt_ms: null }),
      studio({ id: 'b2', name: 'Two', connection: 'offline', rtt_ms: null }),
      studio({ id: 'b3', name: 'Three', connection: 'incompatible', rtt_ms: null }),
    ],
  });
  const { doc } = await start(status);
  const connections = rows(doc).map((r) => text(r, '.connection'));
  assert.deepEqual(connections, ['This computer', 'Reconnecting', 'Offline', 'Needs an update']);
  assert.equal(doc.querySelectorAll('.rtt').length, 0);
});

test('a missing permission of this computer is listed with an Open settings button that runs its fix', async () => {
  const fix = { ...problem('missing_permission:accessibility', 'Mouser needs Accessibility access.', 'open_system_settings:accessibility') };
  const status = full({
    problems: [fix],
    computers: [{ ...makeStatus().computers[0], permissions: { accessibility: 'missing', input_monitoring: 'granted' } }],
  });
  const { win, doc, api } = await start(status);
  const [self] = rows(doc);
  const missing = [...self.querySelectorAll('.permission')];
  assert.equal(missing.length, 1);
  assert.match(missing[0].textContent, /Accessibility is missing/);
  click(win, missing[0].querySelector('button'));
  await flush();
  assert.equal(missing[0].querySelector('button').textContent, 'Open settings');
  assert.deepEqual(api.calls.filter(([m]) => m === 'fix'), [['fix', 'missing_permission:accessibility', 'open_system_settings:accessibility']]);
});

test('a failed Open settings shows why', async () => {
  const fix = problem('p', 'Mouser needs Accessibility access.', 'open_system_settings:accessibility');
  const status = full({
    problems: [fix],
    computers: [{ ...makeStatus().computers[0], permissions: { accessibility: 'missing' } }],
  });
  const { win, doc } = await start(status, { fixError: new Error('That screen could not be opened.') });
  click(win, doc.querySelector('.permission button'));
  await flush();
  assert.equal(doc.getElementById('error').textContent, 'That screen could not be opened.');
});

test("another computer's missing permission is listed without a button, and so is one with no fix to run", async () => {
  const status = full({
    computers: [
      { ...makeStatus().computers[0], permissions: { screen_recording: 'missing' } },
      studio({ permissions: { accessibility: 'missing' } }),
    ],
    problems: [{ ...problem('x', 'Studio needs Accessibility access.', 'open_system_settings:accessibility'), computer: 'bb' }],
  });
  const { doc } = await start(status);
  const [self, other] = rows(doc);
  assert.match(text(other, '.permission'), /Accessibility is missing/);
  assert.equal(other.querySelector('.permission button'), null);
  assert.match(text(self, '.permission'), /Screen Recording is missing/);
  assert.equal(self.querySelector('.permission button'), null);
});

test('limitations and the most recent problem are written under the computer', async () => {
  const status = full({
    computers: [
      makeStatus().computers[0],
      studio({ limitations: ['Dragging files is not supported.'] }),
    ],
    problems: [
      { ...problem('a', 'Studio went quiet.'), computer: 'bb', kind: 'peer_offline', raised_at: '2026-10-21T00:00:00Z' },
      { ...problem('b', 'Studio cannot receive clicks.'), computer: 'bb', raised_at: '2026-10-21T02:00:00Z' },
    ],
  });
  const { doc } = await start(status);
  const [self, other] = rows(doc);
  assert.equal(text(other, '.limitation'), 'Dragging files is not supported.');
  assert.equal(text(other, '.problem'), 'Studio cannot receive clicks.');
  assert.equal(self.querySelector('.limitation'), null);
  assert.equal(self.querySelector('.problem'), null);
});

test('diagnostic logging is a checkbox that starts from the status and calls logging.set', async () => {
  const off = await start(full());
  assert.equal(off.doc.getElementById('logging').checked, false);
  assert.equal(off.doc.getElementById('open-log').hidden, true);

  const box = off.doc.getElementById('logging');
  box.checked = true;
  box.dispatchEvent(new off.win.Event('change', { bubbles: true }));
  await flush();
  assert.deepEqual(off.api.calls.find(([m]) => m === 'logging.set'), ['logging.set', { enabled: true }]);
  assert.equal(off.doc.getElementById('open-log').hidden, false);
  assert.equal(off.doc.getElementById('open-log').textContent, 'Open log folder');
  click(off.win, off.doc.getElementById('open-log'));
  await flush();
  assert.deepEqual(off.api.calls.find(([m]) => m === 'open_folder'), ['open_folder', '/home/me/logs/mouser.log']);

  box.checked = false;
  box.dispatchEvent(new off.win.Event('change', { bubbles: true }));
  await flush();
  assert.equal(off.doc.getElementById('open-log').hidden, true);
});

test('logging that is already on shows Open log folder for the status path', async () => {
  const { win, doc, api } = await start(full(logging(true, '/var/mouser.log')));
  assert.equal(doc.getElementById('logging').checked, true);
  assert.equal(doc.getElementById('open-log').hidden, false);
  click(win, doc.getElementById('open-log'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'open_folder'), ['open_folder', '/var/mouser.log']);
});

test('a refused logging change or folder is explained', async () => {
  const { win, doc } = await start(full(logging(true)), { 'logging.set': new Error('The log cannot be written.') });
  const box = doc.getElementById('logging');
  box.checked = false;
  box.dispatchEvent(new win.Event('change', { bubbles: true }));
  await flush();
  assert.equal(doc.getElementById('error').textContent, 'The log cannot be written.');
});

test('the panel keeps itself current on status and problem notifications, a timer and focus', async () => {
  const { api, doc, timers, focus } = await start(full());
  assert.equal(timers[0][0], 3000);
  api.emit('status', {});
  api.emit('problem', {});
  api.emit('transfer', {});
  await flush();
  await timers[0][1]();
  await focus[0]();
  assert.equal(api.calls.filter(([m]) => m === 'status').length, 5);
  assert.equal(rows(doc).length, 2);
});

test('a failed refresh is shown', async () => {
  const { doc } = await start(new Error('Mouser is not running.'));
  assert.equal(doc.getElementById('error').textContent, 'Mouser is not running.');
});

test('Send Ctrl-Alt-Del shows only while a Windows computer is active and calls send_cad', async () => {
  const hidden = await start(full());
  assert.equal(hidden.doc.getElementById('cad-section').hidden, true);

  const home = await start({ ...full(), computers: [{ ...full().computers[0], os: 'windows' }] });
  assert.equal(home.doc.getElementById('cad-section').hidden, false);

  const remote = await start({ ...full(), active_computer: 'bb' }, { send_cad: {} });
  assert.equal(remote.doc.getElementById('cad-section').hidden, false);
  click(remote.win, remote.doc.getElementById('send-cad'));
  await flush();
  assert.deepEqual(remote.api.calls.at(-1), ['send_cad', undefined]);
});

test('a refused Send Ctrl-Alt-Del shows the engine message', async () => {
  const page = await start({ ...full(), active_computer: 'bb' }, { send_cad: new Error('Ctrl-Alt-Del cannot be sent to Studio.') });
  click(page.win, page.doc.getElementById('send-cad'));
  await flush();
  assert.equal(page.doc.getElementById('error').textContent, 'Ctrl-Alt-Del cannot be sent to Studio.');
});
