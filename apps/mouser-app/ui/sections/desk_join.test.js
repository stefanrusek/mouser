import assert from 'node:assert/strict';
import test from 'node:test';
import { create } from './desk_join.js';
import { click, flush, labelled, mount } from '../test_helpers.js';

const request = (id, computer = 'Laptop', desk = 'Home office') => ({
  request_id: id,
  computer: { id: `c${id}`, name: computer },
  desk: { id: 'd1', name: desk },
});
const start = (pending = [request(1)], answers = {}) =>
  mount({ create }, { answers: { 'join.pending': pending, 'join.decide': {}, ...answers } });

test('a join_request shows an approval card "Allow <computer> to join <desk>?" with Allow and Decline', async () => {
  const { emit, api, root } = await start([]);
  assert.equal(root.querySelectorAll('.card').length, 0);
  emit('join_request', request(2, 'Studio', 'Lab'));
  assert.equal(root.querySelector('h2').textContent, 'Allow Studio to join Lab?');
  assert.deepEqual([...root.querySelectorAll('button')].map((b) => b.textContent), ['Allow', 'Decline']);
});

test('a request that was pending before the window opened is shown, and the same request twice once', async () => {
  const { emit, api, root } = await start([request(1)]);
  assert.equal(root.querySelectorAll('.card').length, 1);
  emit('join_request', request(1));
  emit('status', {});
  assert.equal(root.querySelectorAll('.card').length, 1);
});

test('Allow and Decline call join.decide and take the card away', async () => {
  const { win, api, root } = await start([request(1), request(2, 'Studio')]);
  click(win, labelled(root, 'Allow'));
  await flush();
  click(win, labelled(root, 'Decline'));
  await flush();
  assert.deepEqual(api.calls.filter(([m]) => m === 'join.decide'), [
    ['join.decide', { request_id: 1, approve: true }],
    ['join.decide', { request_id: 2, approve: false }],
  ]);
  assert.equal(root.querySelectorAll('.card').length, 0);
});

test('a failed decision is shown and the card stays', async () => {
  const { win, root } = await start([request(1)], { 'join.decide': new Error('Too late.') });
  click(win, labelled(root, 'Allow'));
  await flush();
  assert.equal(root.querySelector('.error').textContent, 'Too late.');
  assert.equal(root.querySelectorAll('.card').length, 1);
});
