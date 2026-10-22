import assert from 'node:assert/strict';
import test from 'node:test';
import { init } from './quickjump.js';
import { fakeApi, loadPage } from './test_helpers.js';

async function start(letter) {
  const win = loadPage('quickjump.html');
  win.happyDOM.setURL(`http://localhost/quickjump.html?letter=${letter}`);
  const api = fakeApi();
  await init({ doc: win.document, api });
  return { doc: win.document, api };
}
const letter = (doc) => doc.getElementById('letter');

test('the window shows the letter assigned to its display', async () => {
  const { doc } = await start('K');
  assert.ok(!letter(doc).hidden);
  assert.equal(letter(doc).textContent, 'K');
});

test('each display shows its own letter', async () => {
  const first = await start('A');
  const second = await start('7');
  assert.equal(letter(first.doc).textContent, 'A');
  assert.equal(letter(second.doc).textContent, '7');
});

test('ending QuickJump removes the letter', async () => {
  const { doc, api } = await start('B');
  api.emitQuickJump();
  assert.ok(letter(doc).hidden);
  assert.equal(letter(doc).textContent, '');
});

test('the letter is centred on the display', () => {
  const css = readCss();
  assert.match(css, /display:\s*grid/);
  assert.match(css, /place-items:\s*center/);
});

function readCss() {
  return readFileSync(new URL('./quickjump.css', import.meta.url), 'utf8');
}
import { readFileSync } from 'node:fs';
