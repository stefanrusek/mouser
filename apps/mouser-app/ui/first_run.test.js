import assert from 'node:assert/strict';
import test from 'node:test';
import { init, permissionRows, validName, waitingText } from './first_run.js';
import { click, fakeApi, flush, loadPage } from './test_helpers.js';
import { makeStatus, problem } from './status_fixture.js';

const HOME = { desk_id: 'd1', name: 'Home office', members: ['Studio', 'Laptop'] };
const LAB = { desk_id: 'd2', name: 'Lab bench', members: ['Bench PC'] };
const MEMBER = { id: 'd1', name: 'Home office' };

async function start(answers = {}) {
  const win = loadPage('first_run.html');
  const doc = win.document;
  const api = fakeApi({
    status: makeStatus({ permissions: { accessibility: 'missing', input_monitoring: 'granted', screen_recording: 'not_applicable' } }),
    'desks.discovered': [HOME, LAB],
    ...answers,
  });
  const timers = [];
  const focus = [];
  const page = await init({
    doc,
    api,
    every: (ms, tick) => timers.push([ms, tick]),
    onFocus: (handler) => focus.push(handler),
  });
  return { win, doc, api, page, timers, focus };
}
const text = (doc, id) => doc.getElementById(id).textContent;
const shown = (doc, id) => !doc.getElementById(id).hidden;
const buttonNamed = (win, root, label) => [...root.querySelectorAll('button')].find((b) => b.textContent === label);

test('only granted and missing permissions are listed, in a fixed order', () => {
  const rows = permissionRows(
    makeStatus({ permissions: { input_monitoring: 'granted', accessibility: 'missing', screen_recording: 'not_applicable' } }),
  );
  assert.deepEqual(rows.map((r) => [r.kind, r.granted]), [['accessibility', false], ['input_monitoring', true]]);
});

test('the permission step marks each permission granted or missing', async () => {
  const { doc } = await start();
  const rows = doc.getElementById('permission-list').querySelectorAll('li');
  assert.equal(rows.length, 2);
  assert.match(rows[0].textContent, /Accessibility/);
  assert.match(rows[0].textContent, /Missing/);
  assert.match(rows[1].textContent, /Input Monitoring/);
  assert.match(rows[1].textContent, /Granted/);
  assert.equal(rows[1].querySelectorAll('button').length, 0);
  assert.ok(shown(doc, 'step-permissions'));
  assert.ok(!shown(doc, 'step-desk'));
});

test('Open settings asks the app to open that permission screen', async () => {
  const { win, doc, api } = await start();
  click(win, buttonNamed(win, doc, 'Open settings'));
  await flush();
  assert.deepEqual(api.calls.at(-1), ['open_permission_settings', 'accessibility']);
});

test('Continue shows the discovered desks with their member names', async () => {
  const { win, doc } = await start();
  click(win, doc.getElementById('continue'));
  await flush();
  assert.ok(shown(doc, 'step-desk'));
  const rows = doc.getElementById('desk-list').querySelectorAll('li');
  assert.equal(rows.length, 2);
  assert.match(rows[0].textContent, /Home office/);
  assert.match(rows[0].textContent, /Studio, Laptop/);
  assert.ok(!shown(doc, 'no-desks'));
});

test('with no desks around, the step says so and offers to create one', async () => {
  const { win, doc } = await start({ 'desks.discovered': [] });
  click(win, doc.getElementById('continue'));
  await flush();
  assert.ok(shown(doc, 'no-desks'));
  assert.ok(doc.getElementById('create-form'));
});

test('choosing a desk waits for approval, then shows membership', async () => {
  const { win, doc, api, page } = await start();
  click(win, doc.getElementById('continue'));
  await flush();
  click(win, buttonNamed(win, doc, 'Join'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'desk.join'), ['desk.join', { desk_id: 'd1' }]);
  assert.equal(text(doc, 'waiting-text'), 'Waiting for approval on Studio or Laptop');
  assert.ok(shown(doc, 'step-waiting'));

  api.call = async (method) => (method === 'status' ? makeStatus({ desk: { ...MEMBER, pending_join: true } }) : []);
  await page.tick();
  assert.ok(shown(doc, 'step-waiting'));

  api.call = async () => makeStatus({ desk: MEMBER });
  await page.tick();
  assert.equal(text(doc, 'result-text'), 'Member of Home office');
  assert.ok(shown(doc, 'step-result'));
  assert.equal(text(doc, 'result-action'), 'Done');
});

test('Done closes the window', async () => {
  const { win, doc, api } = await start({ status: makeStatus({ desk: MEMBER }) });
  click(win, doc.getElementById('continue'));
  await flush();
  assert.equal(text(doc, 'result-text'), 'Member of Home office');
  click(win, doc.getElementById('result-action'));
  await flush();
  assert.deepEqual(api.calls.at(-1), ['hide_window']);
});

test('a declined request says Not admitted and offers another desk', async () => {
  const { win, doc, api, page } = await start();
  click(win, doc.getElementById('continue'));
  await flush();
  click(win, buttonNamed(win, doc, 'Join'));
  await flush();
  api.call = async (method) => (method === 'status' ? makeStatus() : [HOME]);
  await page.tick();
  assert.equal(text(doc, 'result-text'), 'Not admitted');
  assert.equal(text(doc, 'result-help'), 'Home office did not let this computer join.');
  assert.equal(text(doc, 'result-action'), 'Choose another desk');
  click(win, doc.getElementById('result-action'));
  await flush();
  assert.ok(shown(doc, 'step-desk'));
  assert.ok(!shown(doc, 'step-result'));
});

test('a desk name must be 1 to 64 characters', () => {
  assert.equal(validName(''), null);
  assert.equal(validName('   '), null);
  assert.equal(validName(' Home office '), 'Home office');
  assert.equal(validName('x'.repeat(64)), 'x'.repeat(64));
  assert.equal(validName('x'.repeat(65)), null);
  assert.equal(validName('é'.repeat(64)), 'é'.repeat(64));
});

test('creating a desk needs a valid name, then makes this computer a member', async () => {
  const { win, doc, api } = await start({ 'desk.create': { desk_id: 'd9', name: 'Home office', state: {} } });
  click(win, doc.getElementById('continue'));
  await flush();
  const form = doc.getElementById('create-form');
  const submit = () => form.dispatchEvent(new win.Event('submit', { cancelable: true }));
  submit();
  await flush();
  assert.equal(text(doc, 'form-error'), 'Give your desk a name of 1 to 64 characters.');
  assert.ok(!api.calls.some(([m]) => m === 'desk.create'));
  doc.getElementById('desk-name').value = '  Home office ';
  submit();
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'desk.create'), ['desk.create', { name: 'Home office' }]);
  assert.equal(text(doc, 'form-error'), '');
  assert.equal(text(doc, 'result-text'), 'Member of Home office');
});

test('waiting text names the desk when no member names are known', () => {
  assert.equal(waitingText({ name: 'Home office', members: [] }), 'Waiting for approval on a computer in Home office');
});

test('failures are shown in plain words instead of being lost', async () => {
  const { win, doc, api } = await start();
  click(win, doc.getElementById('continue'));
  await flush();
  api.call = async () => {
    throw { code: -32000, message: 'Mouser could not reach Studio.' };
  };
  click(win, buttonNamed(win, doc, 'Join'));
  await flush();
  assert.equal(text(doc, 'error'), 'Mouser could not reach Studio.');
  assert.ok(shown(doc, 'step-desk'));
});

test('the window keeps itself current: timer, focus and status notifications', async () => {
  const { doc, api, page, timers, focus } = await start();
  assert.equal(timers[0][0], 2000);
  const before = api.calls.length;
  await timers[0][1]();
  await focus[0]();
  assert.equal(api.calls.length, before + 2);
  api.emit('status', {});
  await flush();
  assert.equal(api.calls.length, before + 3);
  api.emit('problem', problem('p', 'Mouser needs Accessibility access.', 'open_system_settings:accessibility'));
  assert.match(doc.getElementById('problems').textContent, /Mouser needs Accessibility access/);
  assert.equal(page.state.step, 'permissions');
});

test('problems from status are shown above the steps', async () => {
  const { doc } = await start({ status: makeStatus({ problems: [problem('p', 'Look here.')], permissions: {} }) });
  assert.match(doc.getElementById('problems').textContent, /Look here\./);
});
