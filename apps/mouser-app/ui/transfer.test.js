import assert from 'node:assert/strict';
import test from 'node:test';
import { formatBytes, formatEta, formatRate, init, renderTransfer } from './transfer.js';
import { click, fakeApi, flush, loadPage } from './test_helpers.js';

const note = (extra = {}) => ({
  transfer_id: 'ab12',
  state: 'copying',
  current_item: 'photo.png',
  bytes_done: 1048576,
  bytes_total: 4194304,
  rate: 524288,
  eta_seconds: 6,
  ...extra,
});

async function start(transfers = []) {
  const win = loadPage('transfer.html');
  const api = fakeApi({ transfer_resolve: {}, transfers });
  const page = await init({ doc: win.document, api });
  return { win, doc: win.document, api, page };
}
const text = (doc, selector) => doc.querySelector(selector).textContent;
const buttons = (doc) => [...doc.querySelectorAll('button')];

test('sizes, speeds and times are written for people', () => {
  assert.equal(formatBytes(0), '0 B');
  assert.equal(formatBytes(1023), '1023 B');
  assert.equal(formatBytes(1536), '1.5 KB');
  assert.equal(formatBytes(1048576), '1.0 MB');
  assert.equal(formatBytes(5 * 1024 ** 3), '5.0 GB');
  assert.equal(formatRate(524288), '512.0 KB/s');
  assert.equal(formatEta(6), '6 s left');
  assert.equal(formatEta(125), '2 min 5 s left');
  assert.equal(formatEta(3600), '60 min 0 s left');
});

test('a copy shows the file name, done of total, speed and time remaining', async () => {
  const { api, doc } = await start();
  api.emit('transfer', note());
  assert.equal(text(doc, '.item'), 'photo.png');
  assert.equal(text(doc, '.amount'), '1.0 MB of 4.0 MB');
  assert.equal(text(doc, '.speed'), '512.0 KB/s');
  assert.equal(text(doc, '.eta'), '6 s left');
  assert.equal(doc.querySelector('progress').value, 1048576);
  assert.equal(doc.querySelector('progress').max, 4194304);
});

test('speed and time are left out until they are known, and an empty total shows no bar', async () => {
  const { api, doc } = await start();
  api.emit('transfer', note({ state: 'checking', rate: 0, eta_seconds: null, bytes_total: 0, bytes_done: 0, current_item: '' }));
  assert.equal(doc.querySelector('.speed'), null);
  assert.equal(doc.querySelector('.eta'), null);
  assert.equal(doc.querySelector('progress'), null);
  assert.equal(text(doc, '.item'), 'Getting ready');
});

test('a finished transfer says so', async () => {
  const { api, doc } = await start();
  api.emit('transfer', note({ state: 'completed', bytes_done: 4194304 }));
  assert.equal(text(doc, '.state'), 'Done');
  api.emit('transfer', note({ state: 'pending' }));
  assert.equal(text(doc, '.state'), 'Waiting');
  api.emit('transfer', note({ state: 'copying' }));
  assert.equal(text(doc, '.state'), 'Copying');
});

test('a conflict asks Overwrite <name>? and Overwrite answers yes', async () => {
  const { win, api, doc } = await start();
  api.emit('transfer', note({ state: 'awaiting_overwrite_decision', item: 'report.pdf' }));
  assert.equal(text(doc, '.question'), 'Overwrite report.pdf?');
  assert.deepEqual(buttons(doc).map((b) => b.textContent), ['Overwrite', 'Skip']);
  click(win, buttons(doc)[0]);
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'transfer_resolve'), ['transfer_resolve', 'ab12', true]);
});

test('Skip answers no and an answered question goes away with the next update', async () => {
  const { win, api, doc } = await start();
  api.emit('transfer', note({ state: 'awaiting_overwrite_decision', item: 'report.pdf' }));
  click(win, buttons(doc)[1]);
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'transfer_resolve'), ['transfer_resolve', 'ab12', false]);
  api.emit('transfer', note());
  assert.equal(doc.querySelector('.question'), null);
});

test('a failed answer is shown', async () => {
  const win = loadPage('transfer.html');
  const api = fakeApi({ transfer_resolve: new Error('That transfer is over.') });
  await init({ doc: win.document, api });
  api.emit('transfer', note({ state: 'awaiting_overwrite_decision', item: 'a.txt' }));
  click(win, win.document.querySelector('button'));
  await flush();
  assert.equal(win.document.getElementById('error').textContent, 'That transfer is over.');
});

test('not enough space is explained and says nothing was copied', async () => {
  const { api, doc } = await start();
  api.emit('transfer', note({ state: 'rejected', reason: 'no_space', message: 'x' }));
  assert.equal(text(doc, '.rejected'), 'There is not enough space on the other computer. Nothing was copied.');
  assert.equal(doc.querySelector('progress'), null);
});

test('any other rejection shows the engine message', async () => {
  const { api, doc } = await start();
  api.emit('transfer', note({ state: 'rejected', reason: 'other', message: 'Declined.' }));
  assert.equal(text(doc, '.rejected'), 'Declined.');
});

test('several transfers are listed once each and other notifications are ignored', async () => {
  const { api, doc, page } = await start();
  api.emit('transfer', note());
  api.emit('transfer', note({ transfer_id: 'cd34', current_item: 'b.txt' }));
  api.emit('transfer', note({ bytes_done: 2 * 1048576 }));
  api.emit('status', {});
  assert.equal(doc.querySelectorAll('.card').length, 2);
  assert.equal(page.transfers().length, 2);
  assert.equal(text(doc, '.amount'), '2.0 MB of 4.0 MB');
});

test('renderTransfer builds a card for one notification', () => {
  const win = loadPage('transfer.html');
  const card = renderTransfer(win.document, note(), () => {});
  assert.equal(card.className, 'card');
});

test('a transfer that failed shows the engine message and no progress', async () => {
  const { api, doc } = await start();
  const message = 'The copy stopped because Beta went offline. 2 of 5 files were copied.';
  api.emit('transfer', note({ state: 'failed', reason: 'peer_offline', message }));
  assert.equal(text(doc, '.failed'), message);
  assert.equal(doc.querySelector('progress'), null);
  assert.equal(doc.querySelector('.item'), null);
});

test('transfers the app saw before the window opened are shown on load', async () => {
  const rejected = note({ transfer_id: 'ee55', state: 'rejected', reason: 'no_space', message: 'x' });
  const { doc, page } = await start([rejected, note()]);
  assert.equal(doc.querySelectorAll('.card').length, 2);
  assert.equal(text(doc, '.rejected'), 'There is not enough space on the other computer. Nothing was copied.');
  assert.equal(page.transfers().length, 2);
});

test('a newer notification wins over the saved one for the same transfer', async () => {
  const win = loadPage('transfer.html');
  let release;
  const api = fakeApi({ transfers: [] });
  const saved = [note({ bytes_done: 1 }), note({ transfer_id: 'cd34', current_item: 'b.txt' })];
  api.transfers = () => new Promise((resolve) => { release = () => resolve(saved); });
  const starting = init({ doc: win.document, api });
  await flush();
  api.emit('transfer', note({ bytes_done: 3 * 1048576 }));
  release();
  const page = await starting;
  assert.equal(page.transfers().length, 2);
  assert.equal(page.transfers().find((t) => t.transfer_id === 'ab12').bytes_done, 3 * 1048576);
});
