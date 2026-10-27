import assert from 'node:assert/strict';
import test from 'node:test';
import { Window } from 'happy-dom';
import { create, renderHelp } from './help.js';
import { flush, mount } from '../test_helpers.js';
import { makeStatus } from '../status_fixture.js';

const GUIDE = '# Troubleshooting\n\nRun `mouser status` first.\n\n- one\n- two';

test('renderHelp shows this computer\'s Mouser version and the troubleshooting guide', () => {
  const doc = new Window().document;
  const root = doc.createElement('div');
  renderHelp(doc, root, GUIDE, '0.3.0');
  assert.equal(root.querySelector('.version').textContent, 'Mouser 0.3.0');
  assert.equal(root.querySelector('h1').textContent, 'Troubleshooting');
  assert.equal(root.querySelector('code').textContent, 'mouser status');
  assert.deepEqual([...root.querySelectorAll('li')].map((n) => n.textContent), ['one', 'two']);
});

test('the Help section fetches help/troubleshooting.md at run time and shows it with this computer\'s version', async () => {
  const fetched = [];
  const section = await mountHelp(async (url) => {
    fetched.push(url);
    return { ok: true, text: async () => GUIDE };
  });
  assert.deepEqual(fetched, ['help/troubleshooting.md']);
  assert.equal(section.root.querySelector('.version').textContent, 'Mouser 0.3.0');
  assert.equal(section.root.querySelector('h1').textContent, 'Troubleshooting');
});

test('a guide that cannot be loaded says so, and the version still shows', async () => {
  const section = await mountHelp(async () => ({ ok: false, status: 404 }));
  assert.equal(section.root.querySelector('.error').textContent, 'The troubleshooting guide could not be loaded (404).');
  assert.equal(section.root.querySelector('.version').textContent, 'Mouser 0.3.0');
});

async function mountHelp(fetch) {
  const m = await mount({ create: (deps) => create({ ...deps, fetch }) }, { status: makeStatus() });
  await flush();
  return m;
}
