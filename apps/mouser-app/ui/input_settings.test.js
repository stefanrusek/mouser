import assert from 'node:assert/strict';
import test from 'node:test';
import { BUTTON_CHOICES, init } from './input_settings.js';
import { fakeApi, flush, loadPage } from './test_helpers.js';

async function start(answers = {}) {
  const win = loadPage('input_settings.html');
  const api = fakeApi({ 'config.get': 'passthrough', 'config.set': {}, ...answers });
  await init({ doc: win.document, api });
  return { win, doc: win.document, api };
}
const choice = (doc, value) => doc.querySelector(`input[name="buttons"][value="${value}"]`);
const change = (win, node) => {
  node.checked = true;
  node.dispatchEvent(new win.Event('change', { bubbles: true }));
};

test('buttons 4 and 5 offer pass through or Back and Forward', async () => {
  const { doc } = await start();
  assert.deepEqual(
    BUTTON_CHOICES.map(([value, label]) => [value, label]),
    [['passthrough', 'Pass through unchanged'], ['back_forward', 'Back and Forward']],
  );
  const labels = [...doc.querySelectorAll('#buttons-choices label')].map((l) => l.textContent.trim());
  assert.deepEqual(labels, ['Pass through unchanged', 'Back and Forward']);
});

test('the saved choice is selected', async () => {
  const { doc, api } = await start({ 'config.get': 'back_forward' });
  assert.deepEqual(api.calls[0], ['config.get', { key: 'mouse.buttons_4_5' }]);
  assert.ok(choice(doc, 'back_forward').checked);
  assert.ok(!choice(doc, 'passthrough').checked);
});

test('choosing one writes mouse.buttons_4_5', async () => {
  const { win, doc, api } = await start();
  change(win, choice(doc, 'back_forward'));
  await flush();
  assert.deepEqual(api.calls.at(-1), ['config.set', { key: 'mouse.buttons_4_5', value: 'back_forward' }]);
  assert.equal(doc.getElementById('buttons-note').textContent, 'Saved.');
});

test('a rejected choice is explained', async () => {
  const { win, doc } = await start({ 'config.set': new Error('mouse.buttons_4_5: not a choice') });
  change(win, choice(doc, 'back_forward'));
  await flush();
  assert.equal(doc.getElementById('buttons-note').textContent, 'mouse.buttons_4_5: not a choice');
});
