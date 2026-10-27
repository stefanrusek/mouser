import assert from 'node:assert/strict';
import test from 'node:test';
import { create } from './settings_hotkeys.js';
import { click, flush, labelled, mount } from '../test_helpers.js';

const CONFIG = {
  'hotkeys.panic': 'Escape*2',
  'hotkeys.quickjump': 'Ctrl+Alt+Shift+J',
  'hotkeys.enable_disable': 'Ctrl+Alt+Shift+M',
  'hotkeys.targeted_paste': 'Ctrl+Alt+Shift+V',
};
const start = (answers = {}) =>
  mount({ create }, { answers: { 'config.get': (p) => CONFIG[p.key], 'config.set': {}, ...answers } });
const row = (root, key) => root.querySelector(`[data-setting="hotkeys.${key}"]`);
const press = (win, init) => win.dispatchEvent(new win.KeyboardEvent('keydown', { cancelable: true, ...init }));
const sets = (api) => api.calls.filter(([m]) => m === 'config.set');

test('every hotkey shows its current key', async () => {
  const { root } = await start();
  assert.equal(row(root, 'panic').querySelector('.current').textContent, 'Escape*2');
  assert.equal(row(root, 'quickjump').querySelector('.current').textContent, 'Ctrl+Alt+Shift+J');
  assert.equal(row(root, 'enable_disable').querySelector('.current').textContent, 'Ctrl+Alt+Shift+M');
  assert.equal(row(root, 'targeted_paste').querySelector('.current').textContent, 'Ctrl+Alt+Shift+V');
});

for (const key of ['quickjump', 'enable_disable', 'targeted_paste']) {
  test(`recording the ${key} hotkey saves the pressed chord with origin app`, async () => {
    const { win, api, root } = await start();
    click(win, labelled(row(root, key), 'Record'));
    assert.match(row(root, key).querySelector('.current').textContent, /Press the new key combination/);
    press(win, { code: 'KeyK', ctrlKey: true, altKey: true });
    await flush();
    assert.deepEqual(sets(api), [['config.set', { key: `hotkeys.${key}`, value: 'Ctrl+Alt+K', origin: 'app' }]]);
    assert.equal(row(root, key).querySelector('.current').textContent, 'Ctrl+Alt+K');
  });
}

test('recording the panic key counts presses and saves when done', async () => {
  const { win, api, root } = await start();
  const panic = row(root, 'panic');
  click(win, labelled(panic, 'Record'));
  press(win, { code: 'F9' });
  press(win, { code: 'F9' });
  press(win, { code: 'F9' });
  assert.match(panic.querySelector('.current').textContent, /F9\*3/);
  assert.deepEqual(sets(api), []);
  click(win, labelled(panic, 'Done'));
  await flush();
  assert.deepEqual(sets(api), [['config.set', { key: 'hotkeys.panic', value: 'F9*3', origin: 'app' }]]);
  assert.equal(panic.querySelector('.current').textContent, 'F9*3');
});

test('finishing the panic recording with no key pressed saves nothing', async () => {
  const { win, api, root } = await start();
  const panic = row(root, 'panic');
  click(win, labelled(panic, 'Record'));
  click(win, labelled(panic, 'Done'));
  await flush();
  assert.deepEqual(sets(api), []);
  assert.equal(panic.querySelector('.current').textContent, 'Escape*2');
});

test('Esc cancels recording and keeps the old key', async () => {
  const { win, api, root } = await start();
  click(win, labelled(row(root, 'quickjump'), 'Record'));
  press(win, { code: 'Escape' });
  await flush();
  assert.deepEqual(sets(api), []);
  assert.equal(row(root, 'quickjump').querySelector('.current').textContent, 'Ctrl+Alt+Shift+J');
  const panic = row(root, 'panic');
  click(win, labelled(panic, 'Record'));
  press(win, { code: 'F9' });
  press(win, { code: 'Escape' });
  assert.equal(panic.querySelector('.current').textContent, 'Escape*2');
  assert.equal(labelled(panic, 'Done').hidden, true);
});

test('starting a second recording ends the first', async () => {
  const { win, api, root } = await start();
  click(win, labelled(row(root, 'quickjump'), 'Record'));
  click(win, labelled(row(root, 'targeted_paste'), 'Record'));
  press(win, { code: 'KeyB', ctrlKey: true });
  await flush();
  assert.deepEqual(sets(api), [['config.set', { key: 'hotkeys.targeted_paste', value: 'Ctrl+B', origin: 'app' }]]);
  assert.equal(row(root, 'quickjump').querySelector('.current').textContent, 'Ctrl+Alt+Shift+J');
});

test('a key the engine refuses is shown next to that hotkey and the old key stays', async () => {
  const { win, root } = await start({ 'config.set': new Error('hotkeys.quickjump: "Q" is not a known key') });
  click(win, labelled(row(root, 'quickjump'), 'Record'));
  press(win, { code: 'KeyQ' });
  await flush();
  assert.equal(row(root, 'quickjump').querySelector('.note').textContent, 'hotkeys.quickjump: "Q" is not a known key');
  assert.equal(row(root, 'quickjump').querySelector('.current').textContent, 'Ctrl+Alt+Shift+J');
});

test('a hotkey the operating system refused shows the refusal next to that hotkey', async () => {
  const { root } = await start({ hotkey_refusals: { enable_disable: 'Ctrl+Alt+Shift+M is already used by another application' } });
  assert.equal(row(root, 'enable_disable').querySelector('.refusal').textContent, 'Ctrl+Alt+Shift+M is already used by another application');
  assert.equal(row(root, 'quickjump').querySelector('.refusal').textContent, '');
});

test('a hotkeys config notification reloads the keys and the refusals; other keys are ignored', async () => {
  const { emit, api, root } = await start();
  const before = api.calls.length;
  emit('config', { key: 'advanced.port', old: 1, new: 2, origin: 'cli' });
  await flush();
  assert.equal(api.calls.length, before);
  emit('status', {});
  await flush();
  assert.equal(api.calls.length, before);
  CONFIG['hotkeys.quickjump'] = 'Ctrl+J';
  emit('config', { key: 'hotkeys.quickjump', old: 'x', new: 'Ctrl+J', origin: 'cli' });
  await flush();
  CONFIG['hotkeys.quickjump'] = 'Ctrl+Alt+Shift+J';
  assert.equal(row(root, 'quickjump').querySelector('.current').textContent, 'Ctrl+J');
});

test('a failed load is shown', async () => {
  const { root } = await start({ 'config.get': new Error('The engine is not ready.') });
  assert.equal(root.querySelector('.hotkeys-note').textContent, 'The engine is not ready.');
});
