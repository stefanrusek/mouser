import assert from 'node:assert/strict';
import test from 'node:test';
import { create } from './desk.js';
import { click, flush, labelled, mount } from '../test_helpers.js';
import {
  STATUS_FIXTURES, STUDIO, STUDIO_MAC, WINDOWS, WINDOWS_PC, makeStatus, problem,
} from '../status_fixture.js';

const DESK = { id: '1'.repeat(32), name: 'Home office' };
const answers = (extra = {}) => ({ 'desks.discovered': [], 'join.pending': [], transfers: [], ...extra });
const start = (status, extra) => mount({ create }, { answers: answers(extra), status });
const rows = (root) => [...root.querySelectorAll('li.computer')];
const row = (root, name) => rows(root).find((r) => r.querySelector('.name').textContent.startsWith(name));

test('Desk shows the desk name and each computer with its name, system, version and connection', async () => {
  const { root } = await start(STATUS_FIXTURES.twoMacsWindowsLinux);
  assert.equal(root.querySelector('h2.desk-name').textContent, 'Home office');
  assert.deepEqual(rows(root).map((r) => r.querySelector('.name').textContent), [
    'Desk Mac (this computer)',
    'Studio Mac',
    'Win PC',
    'Linux Box',
  ]);
  const studio = row(root, 'Studio Mac');
  assert.equal(studio.querySelector('.os').textContent, 'macOS 15.4');
  assert.equal(studio.querySelector('.version').textContent, 'Mouser 0.3.0');
  assert.equal(studio.querySelector('.connection').textContent, 'Connected');
  assert.equal(row(root, 'Linux Box').querySelector('.os').textContent, 'Linux');
  assert.equal(row(root, 'Desk Mac').querySelector('.connection').textContent, 'This computer');
});

test('it says which computer has the pointer, from active_computer', async () => {
  const { root } = await start(STATUS_FIXTURES.twoMacsWindowsLinux);
  const pointers = rows(root).filter((r) => r.querySelector('.pointer'));
  assert.deepEqual(pointers.map((r) => r.querySelector('.name').textContent), ['Studio Mac']);
  assert.equal(pointers[0].querySelector('.pointer').textContent, 'Has the pointer');
});

test('a computer whose version differs shows both versions next to it', async () => {
  const { root } = await start(STATUS_FIXTURES.oldPeer);
  assert.equal(row(root, 'Old Mac').querySelector('.version').textContent, 'Mouser 0.2.8 (this computer: 0.3.0)');
  assert.equal(row(root, 'Studio Mac').querySelector('.version').textContent, 'Mouser 0.3.0');
});

test('an incompatible computer says it needs an update', async () => {
  const { root } = await start(STATUS_FIXTURES.incompatiblePeer);
  assert.equal(row(root, 'Ancient PC').querySelector('.connection').textContent, 'Needs an update');
});

test('each computer shows how many problems need the owner, linking to its entry in Health', async () => {
  const status = makeStatus({
    desk: DESK,
    computers: [STUDIO_MAC, WINDOWS_PC],
    problems: [
      problem('a', 'One.', null, WINDOWS),
      problem('b', 'Two.', null, WINDOWS),
      problem('c', 'Three.', null, STUDIO),
    ],
  });
  const { root } = await start(status);
  const link = (name) => row(root, name).querySelector('a.problems');
  assert.equal(link('Win PC').textContent, '2 problems');
  assert.equal(link('Win PC').getAttribute('href'), `#health/${WINDOWS}`);
  assert.equal(link('Studio Mac').textContent, '1 problem');
  assert.equal(link('Studio Mac').getAttribute('href'), `#health/${STUDIO}`);
  assert.equal(link('Desk Mac'), null);
});

test('members can be removed, and this computer can leave the desk', async () => {
  const { win, api, root } = await start(STATUS_FIXTURES.twoMacsWindowsLinux, { 'desk.remove_member': {}, 'desk.leave': {} });
  assert.equal(row(root, 'Desk Mac').querySelector('button'), null);
  click(win, labelled(row(root, 'Studio Mac'), 'Remove'));
  await flush();
  click(win, labelled(root, 'Leave this desk'));
  await flush();
  assert.deepEqual(api.calls.filter(([m]) => m.startsWith('desk.')), [
    ['desk.remove_member', { computer_id: STUDIO }],
    ['desk.leave', undefined],
  ]);
});

test('other desks on the network can be moved to, with the wait for approval named', async () => {
  const desks = [
    { desk_id: '2'.repeat(32), name: 'Studio', members: ['Kitchen', 'Den'] },
    { desk_id: DESK.id, name: 'Home office', members: [] },
    { desk_id: '3'.repeat(32), name: 'Empty', members: [] },
  ];
  const { win, api, root } = await start(STATUS_FIXTURES.alone, { 'desks.discovered': desks, 'desk.move': {} });
  assert.equal(root.querySelector('.other-desks-title').textContent, 'Move to another desk');
  const names = [...root.querySelectorAll('.other-desks li strong')].map((n) => n.textContent);
  assert.deepEqual(names, ['Studio', 'Empty'], 'the desk it is on is not offered');
  click(win, labelled(root, 'Move here'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'desk.move'), ['desk.move', { desk_id: '2'.repeat(32) }]);
  assert.equal(root.querySelector('.move-note').textContent, 'Waiting for approval on Kitchen or Den');
});

test('off a desk it offers to join one and waits for approval of a desk with no known member', async () => {
  const { win, api, root } = await start(makeStatus(), {
    'desks.discovered': [{ desk_id: '4'.repeat(32), name: 'Lab', members: [] }],
    'desk.join': {},
  });
  assert.equal(root.querySelector('.desk-name').textContent, 'Mouser');
  assert.equal(root.querySelector('.desk-state').textContent, 'This computer is not on a desk yet.');
  assert.equal(root.querySelector('.other-desks-title').textContent, 'Join a desk');
  assert.equal(labelled(root, 'Leave this desk').hidden, true);
  click(win, labelled(root, 'Join'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'desk.join'), ['desk.join', { desk_id: '4'.repeat(32) }]);
  assert.equal(root.querySelector('.move-note').textContent, 'Waiting for approval on a computer in Lab');
});

test('while a join is waiting it says so', async () => {
  const { root } = await start(makeStatus({ desk: { ...DESK, pending_join: true } }));
  assert.equal(root.querySelector('.desk-state').textContent, 'Waiting for approval to join Home office.');
  assert.equal(labelled(root, 'Leave this desk').hidden, true);
  const member = await start(STATUS_FIXTURES.alone);
  assert.equal(member.root.querySelector('.desk-state').textContent, 'These computers share one mouse and keyboard.');
  assert.equal(labelled(member.root, 'Leave this desk').hidden, false);
});

test('a failed action is shown, and join requests and transfers appear in Desk', async () => {
  const { win, emit, root } = await start(STATUS_FIXTURES.twoMacsWindowsLinux, { 'desk.remove_member': new Error('Not now.') });
  click(win, labelled(row(root, 'Studio Mac'), 'Remove'));
  await flush();
  assert.equal(root.querySelector('.desk-error').textContent, 'Not now.');
  emit('join_request', { request_id: 7, computer: { id: 'c', name: 'Laptop' }, desk: { id: 'd', name: 'Home office' } });
  assert.equal(root.querySelector('.join-requests h2').textContent, 'Allow Laptop to join Home office?');
  emit('transfer', { transfer_id: 't', state: 'copying', current_item: 'a.png', bytes_done: 1, bytes_total: 2, rate: 0, eta_seconds: null });
  assert.equal(root.querySelector('.transfers .item').textContent, 'a.png');
});

test('it says whether Mouser is active or paused', async () => {
  const { root, section } = await start(STATUS_FIXTURES.alone);
  assert.equal(root.querySelector('.engine-state').textContent, 'Mouser is active');
  section.update(makeStatus({ desk: DESK, state: 'paused' }));
  await flush();
  assert.equal(root.querySelector('.engine-state').textContent, 'Mouser is paused');
});
