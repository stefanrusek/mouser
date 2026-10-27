import assert from 'node:assert/strict';
import test from 'node:test';
import { create } from './desk_transfer.js';
import { click, flush, labelled, mount } from '../test_helpers.js';

const note = (extra = {}) => ({
  transfer_id: 'ab12',
  state: 'copying',
  current_item: 'photo.png',
  bytes_done: 1048576,
  bytes_total: 4194304,
  rate: 524288,
  eta_seconds: 6.4,
  ...extra,
});
const start = (transfers = [], answers = {}) => mount({ create }, { answers: { transfers, ...answers } });
const text = (root, selector) => root.querySelector(selector).textContent;

test('a copy shows the item, bytes, rate and ETA rounded as format.js does, in a panel', async () => {
  const { emit, api, root } = await start();
  assert.equal(root.hidden, true, 'no panel without a transfer');
  emit('transfer', note());
  assert.equal(root.hidden, false);
  assert.equal(text(root, '.item'), 'photo.png');
  assert.equal(text(root, '.amount'), '1.0 MB of 4.0 MB');
  assert.equal(text(root, '.speed'), '512.0 KB/s');
  assert.equal(text(root, '.eta'), '6 s left');
  assert.equal(root.querySelector('progress').value, 1048576);
});

test('speed and time are left out until known, and an empty total shows no bar', async () => {
  const { emit, api, root } = await start();
  emit('transfer', note({ state: 'checking', rate: 0, eta_seconds: null, bytes_total: 0, bytes_done: 0, current_item: '' }));
  assert.equal(root.querySelector('.speed'), null);
  assert.equal(root.querySelector('.eta'), null);
  assert.equal(root.querySelector('progress'), null);
  assert.equal(text(root, '.item'), 'Getting ready');
  assert.equal(text(root, '.state'), 'Waiting');
});

test('the overwrite question has Overwrite and Skip, which answer it', async () => {
  const { emit, win, api, root } = await start();
  emit('transfer', note({ state: 'awaiting_overwrite_decision', item: 'photo.png' }));
  assert.equal(text(root, '.question'), 'Overwrite photo.png?');
  click(win, labelled(root, 'Overwrite'));
  click(win, labelled(root, 'Skip'));
  await flush();
  assert.deepEqual(api.calls.filter(([m]) => m === 'transfer_resolve'), [
    ['transfer_resolve', 'ab12', true],
    ['transfer_resolve', 'ab12', false],
  ]);
});

test('a rejection for space and a failure say what the engine says', async () => {
  const { emit, api, root } = await start();
  emit('transfer', note({ state: 'rejected', reason: 'no_space', message: 'x' }));
  assert.equal(text(root, '.rejected'), 'There is not enough space on the other computer. Nothing was copied.');
  emit('transfer', note({ state: 'rejected', reason: 'other', message: 'Refused.' }));
  assert.equal(text(root, '.rejected'), 'Refused.');
  emit('transfer', note({ state: 'failed', message: 'The link dropped.' }));
  assert.equal(text(root, '.failed'), 'The link dropped.');
});

test('a finished transfer says Done', async () => {
  const { emit, api, root } = await start();
  emit('transfer', note({ state: 'completed' }));
  assert.equal(text(root, '.state'), 'Done');
});

test('transfers the app saw before the window opened are shown, and a newer notification wins', async () => {
  const { emit, api, root } = await start([note({ transfer_id: 'old', current_item: 'old.png' }), note({ transfer_id: 'ab12', current_item: 'saved.png' })]);
  assert.deepEqual([...root.querySelectorAll('.item')].map((n) => n.textContent), ['old.png', 'saved.png']);
  emit('transfer', note({ current_item: 'newer.png' }));
  assert.deepEqual([...root.querySelectorAll('.item')].map((n) => n.textContent), ['old.png', 'newer.png']);
});

test('a failed answer is shown in the panel', async () => {
  const { emit, win, api, root } = await start([], { transfer_resolve: new Error('Gone.') });
  emit('transfer', note({ state: 'awaiting_overwrite_decision', item: 'a' }));
  click(win, labelled(root, 'Skip'));
  await flush();
  assert.equal(text(root, '.error'), 'Gone.');
});

test('other notifications are ignored', async () => {
  const { emit, api, root } = await start();
  emit('status', {});
  assert.equal(root.hidden, true);
});
