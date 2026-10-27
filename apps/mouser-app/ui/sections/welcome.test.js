import assert from 'node:assert/strict';
import test from 'node:test';
import { create, permissionRows, validName } from './welcome.js';
import { click, flush, labelled, mount } from '../test_helpers.js';
import { makeStatus } from '../status_fixture.js';

const HOME = { desk_id: 'd1', name: 'Home office', members: ['Studio', 'Laptop'] };
const MEMBER = { id: 'd1', name: 'Home office' };
const FIRST = makeStatus({ permissions: { accessibility: 'missing', input_monitoring: 'granted', screen_recording: 'not_applicable' } });

async function start(answers = {}, status = FIRST) {
  const m = await mount({ create }, { answers: { 'desks.discovered': [HOME, { desk_id: 'd2', name: 'Lab bench', members: ['Bench PC'] }], ...answers }, status });
  return { ...m, q: (selector) => m.root.querySelector(selector), shown: (name) => !m.root.querySelector(`.step-${name}`).hidden };
}
const submit = (win, form) => form.dispatchEvent(new win.Event('submit', { cancelable: true }));

test('only granted and missing permissions are listed, in a fixed order', () => {
  const rows = permissionRows(
    makeStatus({ permissions: { input_monitoring: 'granted', accessibility: 'missing', screen_recording: 'not_applicable' } }),
  );
  assert.deepEqual(rows.map((r) => [r.kind, r.granted]), [['accessibility', false], ['input_monitoring', true]]);
});

test('first run starts with the permission step inside the window, marking each permission granted or missing', async () => {
  const { root, shown } = await start();
  const rows = root.querySelectorAll('.permission-list li');
  assert.equal(rows.length, 2);
  assert.match(rows[0].textContent, /Accessibility/);
  assert.match(rows[0].textContent, /Missing/);
  assert.match(rows[1].textContent, /Input Monitoring/);
  assert.match(rows[1].textContent, /Granted/);
  assert.equal(rows[1].querySelectorAll('button').length, 0);
  assert.ok(shown('permissions'));
  assert.ok(!shown('desk'));
});

test('Open settings asks the app to open that permission screen', async () => {
  const { win, api, root } = await start();
  click(win, labelled(root, 'Open settings'));
  await flush();
  assert.deepEqual(api.calls.at(-1), ['open_permission_settings', 'accessibility']);
});

test('Continue shows the discovered desks with their member names', async () => {
  const { win, q, root, shown } = await start();
  click(win, q('.continue'));
  await flush();
  assert.ok(shown('desk'));
  const rows = root.querySelectorAll('.desk-list li');
  assert.equal(rows.length, 2);
  assert.match(rows[0].textContent, /Home office/);
  assert.match(rows[0].textContent, /Studio, Laptop/);
  assert.equal(q('.no-desks').hidden, true);
});

test('with no desks around, the step says so and offers to create one', async () => {
  const { win, q } = await start({ 'desks.discovered': [] });
  click(win, q('.continue'));
  await flush();
  assert.equal(q('.no-desks').hidden, false);
  assert.ok(q('.create-form'));
});

test('choosing a desk waits for approval, then shows membership, and Done goes to the desk', async () => {
  const { win, api, q, root, section, shown, navigated } = await start();
  click(win, q('.continue'));
  await flush();
  click(win, labelled(root, 'Join'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'desk.join'), ['desk.join', { desk_id: 'd1' }]);
  assert.equal(q('.waiting-text').textContent, 'Waiting for approval on Studio or Laptop');
  assert.ok(shown('waiting'));

  section.update(makeStatus({ desk: { ...MEMBER, pending_join: true } }));
  assert.ok(shown('waiting'));
  section.update(makeStatus({ desk: MEMBER }));
  assert.equal(q('.result-text').textContent, 'Member of Home office');
  assert.ok(shown('result'));
  assert.equal(q('.result-action').textContent, 'Done');
  click(win, q('.result-action'));
  assert.deepEqual(navigated, ['#desk']);
});

test('a computer that is already on a desk goes straight to membership', async () => {
  const { win, q, navigated } = await start({}, makeStatus({ desk: MEMBER }));
  click(win, q('.continue'));
  await flush();
  assert.equal(q('.result-text').textContent, 'Member of Home office');
  click(win, q('.result-action'));
  assert.deepEqual(navigated, ['#desk']);
});

test('a declined request says Not admitted and offers another desk', async () => {
  const { win, q, root, section, shown } = await start();
  click(win, q('.continue'));
  await flush();
  click(win, labelled(root, 'Join'));
  await flush();
  section.update(makeStatus());
  assert.equal(q('.result-text').textContent, 'Not admitted');
  assert.equal(q('.result-help').textContent, 'Home office did not let this computer join.');
  assert.equal(q('.result-action').textContent, 'Choose another desk');
  click(win, q('.result-action'));
  await flush();
  assert.ok(shown('desk'));
  assert.ok(!shown('result'));
});

test('a desk that is not listed is joined by the address of a computer on it', async () => {
  const { win, api, q, section } = await start();
  click(win, q('.continue'));
  await flush();
  assert.match(q('.address-form').textContent, /Desk not listed\? Enter the address of a computer on it/);
  submit(win, q('.address-form'));
  await flush();
  assert.equal(q('.address-error').textContent, "Type the computer's network address.");
  assert.ok(!api.calls.some(([m]) => m === 'desk.join'));
  q('.address').value = ' 192.168.1.20 ';
  submit(win, q('.address-form'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'desk.join'), ['desk.join', { address: '192.168.1.20' }]);
  assert.equal(q('.address-error').textContent, '');
  assert.equal(q('.waiting-text').textContent, 'Waiting for approval from 192.168.1.20');
  section.update(makeStatus());
  assert.equal(q('.result-help').textContent, 'The computer at 192.168.1.20 did not let this computer join.');
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
  const { win, api, q } = await start({ 'desk.create': { desk_id: 'd9', name: 'Home office', state: {} } });
  click(win, q('.continue'));
  await flush();
  submit(win, q('.create-form'));
  await flush();
  assert.equal(q('.form-error').textContent, 'Give your desk a name of 1 to 64 characters.');
  assert.ok(!api.calls.some(([m]) => m === 'desk.create'));
  q('.desk-name').value = '  Home office ';
  submit(win, q('.create-form'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'desk.create'), ['desk.create', { name: 'Home office' }]);
  assert.equal(q('.form-error').textContent, '');
  assert.equal(q('.result-text').textContent, 'Member of Home office');
});

test('failures are shown in plain words instead of being lost', async () => {
  const { win, q, root } = await start({ 'desk.join': Object.assign(new Error('Mouser could not reach Studio.'), { code: -32000 }) });
  click(win, q('.continue'));
  await flush();
  click(win, labelled(root, 'Join'));
  await flush();
  assert.equal(q('.welcome-error').textContent, 'Mouser could not reach Studio.');
});

test('a status update refreshes the permission list', async () => {
  const { section, root } = await start();
  section.update(makeStatus({ permissions: { accessibility: 'granted' } }));
  assert.match(root.querySelector('.permission-list').textContent, /Granted/);
  assert.doesNotMatch(root.querySelector('.permission-list').textContent, /Missing/);
});
