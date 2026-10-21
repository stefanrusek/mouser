import assert from 'node:assert/strict';
import test from 'node:test';
import { init, promptText } from './join_prompt.js';
import { click, fakeApi, flush, loadPage } from './test_helpers.js';

const request = (id, computer = 'Laptop', desk = 'Home office') => ({
  request_id: id,
  computer: { id: `c${id}`, name: computer },
  desk: { id: 'd1', name: desk },
});

async function start(pending = [request(1)], answers = {}) {
  const win = loadPage('join_prompt.html');
  const api = fakeApi({ 'join.pending': pending, 'join.decide': {}, ...answers });
  const page = await init({ doc: win.document, api });
  return { win, doc: win.document, api, page };
}
const label = (win, node, text) => [...node.querySelectorAll('button')].find((b) => b.textContent === text);

test('a pending request asks Allow <computer> to join <desk>?', async () => {
  const { doc } = await start();
  assert.equal(doc.querySelector('h2').textContent, 'Allow Laptop to join Home office?');
  assert.equal(promptText(request(2, 'Studio', 'Lab')), 'Allow Studio to join Lab?');
  assert.deepEqual([...doc.querySelectorAll('button')].map((b) => b.textContent), ['Allow', 'Decline']);
});

test('Allow approves the request and closes the prompt when none are left', async () => {
  const { win, doc, api } = await start();
  click(win, label(win, doc, 'Allow'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'join.decide'), ['join.decide', { request_id: 1, approve: true }]);
  assert.equal(doc.querySelectorAll('.card').length, 0);
  assert.deepEqual(api.calls.at(-1), ['hide_window']);
});

test('Decline refuses the request', async () => {
  const { win, doc, api } = await start();
  click(win, label(win, doc, 'Decline'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'join.decide'), ['join.decide', { request_id: 1, approve: false }]);
});

test('a new request arriving while the prompt is open is added once', async () => {
  const { win, doc, api, page } = await start();
  api.emit('join_request', request(2, 'Studio'));
  api.emit('join_request', request(2, 'Studio'));
  api.emit('status', {});
  assert.equal(doc.querySelectorAll('.card').length, 2);
  assert.equal(page.requests().length, 2);
  click(win, label(win, doc.querySelectorAll('.card')[0], 'Allow'));
  await flush();
  assert.equal(doc.querySelectorAll('.card').length, 1);
  assert.ok(!api.calls.some(([m]) => m === 'hide_window'));
});

test('a failed decision is explained and the request stays', async () => {
  const { win, doc } = await start([request(1)], {
    'join.decide': new Error('That computer is no longer asking to join.'),
  });
  click(win, label(win, doc, 'Allow'));
  await flush();
  assert.equal(doc.getElementById('error').textContent, 'That computer is no longer asking to join.');
  assert.equal(doc.querySelectorAll('.card').length, 1);
});
