import assert from 'node:assert/strict';
import test from 'node:test';
import { init, parseAllowlist, parsePort } from './desk_settings.js';
import { click, fakeApi, flush, loadPage } from './test_helpers.js';
import { makeStatus, problem } from './status_fixture.js';

const HOME = { id: 'd1', name: 'Home office' };
const peer = (id, name, connection) => ({
  id,
  name,
  os: 'windows',
  version: '0.1.0',
  connection,
  permissions: {},
  limitations: [],
  displays: 1,
});
const MEMBER_STATUS = makeStatus({
  desk: HOME,
  computers: [peer('b1', 'Studio', 'connected'), peer('b2', 'Laptop', 'offline')],
});
const LAB = { desk_id: 'd2', name: 'Lab bench', members: ['Bench PC'] };

async function start(answers = {}) {
  const win = loadPage('desk_settings.html');
  const api = fakeApi({
    status: MEMBER_STATUS,
    'desks.discovered': [{ desk_id: 'd1', name: 'Home office', members: ['Studio'] }, LAB],
    'config.get': (params) => (params.key === 'advanced.port' ? 47801 : ['10.0.0.0/8', '192.168.1.0/24']),
    'config.set': {},
    'desk.leave': {},
    'desk.remove_member': {},
    'desk.move': { state: 'pending' },
    'desk.join': { state: 'pending' },
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
const named = (root, label) => [...root.querySelectorAll('button')].find((b) => b.textContent === label);
const text = (doc, id) => doc.getElementById(id).textContent;
const submit = (win, doc, id) => doc.getElementById(id).dispatchEvent(new win.Event('submit', { cancelable: true }));

test('the desk section lists every member with Remove on the others', async () => {
  const { doc } = await start();
  assert.equal(text(doc, 'desk-title'), 'Home office');
  const rows = [...doc.getElementById('member-list').querySelectorAll('li')];
  assert.deepEqual(rows.map((r) => r.querySelector('.what').textContent), [
    'Desk MacThis computer',
    'StudioConnected',
    'LaptopOffline',
  ]);
  assert.equal(rows[0].querySelectorAll('button').length, 0);
  assert.equal(rows[1].querySelector('button').textContent, 'Remove');
  assert.ok(!doc.getElementById('leave').hidden);
});

test('Remove takes a member off the desk', async () => {
  const { win, doc, api } = await start();
  click(win, named(doc.getElementById('member-list').querySelectorAll('li')[1], 'Remove'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'desk.remove_member'), ['desk.remove_member', { computer_id: 'b1' }]);
});

test('Leave this desk leaves it and shows the result', async () => {
  const { win, doc, api } = await start();
  api.call = async (method) => (method === 'status' ? makeStatus() : []);
  click(win, doc.getElementById('leave'));
  await flush();
  assert.equal(text(doc, 'desk-status'), 'This computer is not on a desk yet.');
  assert.ok(doc.getElementById('leave').hidden);
  assert.ok(doc.getElementById('member-list').hidden);
  assert.equal(text(doc, 'move-title'), 'Join a desk');
  assert.ok(!doc.getElementById('no-other-desks').hidden);
});

test('other desks are offered for a move, not the current one', async () => {
  const { win, doc, api } = await start();
  const rows = doc.getElementById('other-desks').querySelectorAll('li');
  assert.equal(rows.length, 1);
  assert.match(rows[0].textContent, /Lab bench/);
  assert.ok(doc.getElementById('no-other-desks').hidden);
  click(win, named(rows[0], 'Move here'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'desk.move'), ['desk.move', { desk_id: 'd2' }]);
  assert.equal(text(doc, 'move-note'), 'Waiting for approval on Bench PC');
});

test('off a desk, choosing a desk joins it', async () => {
  const { win, doc, api } = await start({ status: makeStatus() });
  const join = named(doc.getElementById('other-desks'), 'Join');
  click(win, join);
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'desk.join'), ['desk.join', { desk_id: 'd1' }]);
});

test('a join that waits for approval says so', async () => {
  const pending = makeStatus({ desk: { ...HOME, pending_join: true } });
  const { doc } = await start({ status: pending });
  assert.equal(text(doc, 'desk-status'), 'Waiting for approval to join Home office.');
  assert.ok(doc.getElementById('leave').hidden);
});

test('desk settings no longer offer adding a computer by address', async () => {
  const { doc } = await start();
  assert.equal(doc.getElementById('address-form'), null);
  assert.ok(!/Add computer by address/.test(doc.body.textContent));
});

test('the mouse settings are one click away', async () => {
  const { doc } = await start();
  assert.equal(doc.getElementById('input-link').getAttribute('href'), 'input_settings.html');
});

test('Advanced shows the allowlist and the port', async () => {
  const { doc } = await start();
  assert.equal(doc.getElementById('allowlist').value, '10.0.0.0/8\n192.168.1.0/24');
  assert.equal(doc.getElementById('port').value, '47801');
  assert.ok(doc.getElementById('advanced').querySelector('summary').textContent === 'Advanced');
});

test('Save writes the allowlist and the port', async () => {
  const { win, doc, api } = await start();
  doc.getElementById('allowlist').value = ' 10.1.0.0/16 \n\n';
  doc.getElementById('port').value = '50000';
  submit(win, doc, 'advanced-form');
  await flush();
  const sets = api.calls.filter(([m]) => m === 'config.set');
  assert.deepEqual(sets, [
    ['config.set', { key: 'advanced.allowlist', value: ['10.1.0.0/16'] }],
    ['config.set', { key: 'advanced.port', value: 50000 }],
  ]);
  assert.equal(text(doc, 'advanced-note'), 'Saved.');
});

test('a port that is not a number is refused before anything is saved', async () => {
  const { win, doc, api } = await start();
  doc.getElementById('port').value = 'abc';
  submit(win, doc, 'advanced-form');
  await flush();
  assert.equal(text(doc, 'advanced-note'), 'Enter the port as a whole number from 1 to 65535.');
  assert.ok(!api.calls.some(([m]) => m === 'config.set'));
});

test('a rejected setting shows the reason next to the fields', async () => {
  const { win, doc } = await start({ 'config.set': new Error('advanced.port: must be 1-65535') });
  doc.getElementById('port').value = '70000';
  submit(win, doc, 'advanced-form');
  await flush();
  assert.equal(text(doc, 'advanced-note'), 'advanced.port: must be 1-65535');
});

test('parsing helpers', () => {
  assert.deepEqual(parseAllowlist(' a \n\n b\n'), ['a', 'b']);
  assert.deepEqual(parseAllowlist(''), []);
  assert.equal(parsePort('47801'), 47801);
  assert.equal(parsePort(''), null);
  assert.equal(parsePort('4.5'), null);
  assert.equal(parsePort('x'), null);
});

test('the window keeps itself current and shows problems', async () => {
  const { doc, api, timers, focus } = await start();
  assert.equal(timers[0][0], 3000);
  const before = api.calls.length;
  await timers[0][1]();
  await focus[0]();
  api.emit('status', {});
  await flush();
  assert.equal(api.calls.length, before + 6);
  api.emit('problem', problem('p', 'Studio is offline.'));
  assert.match(doc.getElementById('problems').textContent, /Studio is offline\./);
});

test('a failed load is shown rather than hidden', async () => {
  const { doc } = await start({ 'config.get': new Error('config.get is not available yet') });
  assert.equal(text(doc, 'error'), 'config.get is not available yet');
});

test('on a Mac, Allow control at the login screen is offered and runs the install', async () => {
  const { win, doc, api } = await start();
  assert.ok(!doc.getElementById('login-section').hidden);
  click(win, doc.getElementById('login-screen'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'allow_login_screen'), ['allow_login_screen']);
  assert.equal(text(doc, 'login-note'), 'Done. This Mac can now be controlled at the login screen.');
});

test('a refused administrator prompt is explained and nothing is claimed done', async () => {
  const message = 'Mouser was not given permission, so nothing was changed.';
  const { win, doc } = await start({ allow_login_screen: new Error(message) });
  click(win, doc.getElementById('login-screen'));
  await flush();
  assert.equal(text(doc, 'login-note'), message);
});

test('the login screen action is not offered on other systems', async () => {
  const status = makeStatus();
  status.computers[0].os = 'windows';
  const { doc } = await start({ status });
  assert.ok(doc.getElementById('login-section').hidden);
});

test('a portable run does not offer to install anything at the login screen', async () => {
  const { doc } = await start({ portable: true });
  assert.ok(doc.getElementById('login-section').hidden);
});
