import assert from 'node:assert/strict';
import test from 'node:test';
import { HIGHLIGHT_MS, init } from './landing.js';
import { fakeApi, flush, loadPage } from './test_helpers.js';

async function start() {
  const win = loadPage('landing.html');
  const api = fakeApi();
  const timers = [];
  await init({ doc: win.document, api, after: (ms, action) => timers.push([ms, action]) });
  return { doc: win.document, api, timers };
}
const ring = (doc) => doc.getElementById('highlight');

test('before any landing nothing is highlighted', async () => {
  const { doc } = await start();
  assert.ok(ring(doc).hidden);
});

test('a landing shows the highlight at the given position', async () => {
  const { doc, api } = await start();
  api.emitLanding({ x: 60, y: 45 });
  assert.ok(!ring(doc).hidden);
  assert.equal(ring(doc).style.left, '60px');
  assert.equal(ring(doc).style.top, '45px');
});

test('the highlight is removed shortly afterwards and the window hides', async () => {
  const { doc, api, timers } = await start();
  api.emitLanding({ x: 60, y: 60 });
  assert.equal(timers.length, 1);
  assert.equal(timers[0][0], HIGHLIGHT_MS);
  assert.ok(HIGHLIGHT_MS <= 1000);
  await timers[0][1]();
  await flush();
  assert.ok(ring(doc).hidden);
  assert.deepEqual(api.calls.at(-1), ['hide_window']);
});
