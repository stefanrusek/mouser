import assert from 'node:assert/strict';
import test from 'node:test';
import { EVENT, makeApi } from './api.js';
import { boot } from './boot.js';
import { button, errorText, guarded } from './dom.js';
import { loadPage } from './test_helpers.js';

function fakeTauri() {
  const invoked = [];
  const listeners = {};
  return {
    invoked,
    listeners,
    core: { invoke: async (cmd, args) => { invoked.push([cmd, args]); return 'ok'; } },
    event: { listen: async (name, handler) => { listeners[name] = handler; return () => {}; } },
  };
}

test('calls go through the control command, other commands by name', async () => {
  const tauri = fakeTauri();
  const api = makeApi(tauri);
  assert.equal(await api.call('status'), 'ok');
  await api.call('desk.create', { name: 'Home' });
  await api.openPermissionSettings('accessibility');
  await api.fix('approve_join');
  await api.hideWindow();
  assert.deepEqual(tauri.invoked, [
    ['control', { request: { method: 'status', params: null } }],
    ['control', { request: { method: 'desk.create', params: { name: 'Home' } } }],
    ['open_permission_settings', { kind: 'accessibility' }],
    ['fix', { fix: 'approve_join' }],
    ['hide_window', undefined],
  ]);
});

test('notifications arrive as the bare notification', async () => {
  const tauri = fakeTauri();
  const seen = [];
  await makeApi(tauri).onNotification((note) => seen.push(note));
  tauri.listeners[EVENT]({ payload: { method: 'status', params: {} } });
  assert.deepEqual(seen, [{ method: 'status', params: {} }]);
});

test('boot hands a window module the document, the api, a timer and focus', () => {
  const tauri = fakeTauri();
  const calls = [];
  const win = {
    document: 'doc',
    __TAURI__: tauri,
    setInterval: (tick, ms) => calls.push(['interval', tick, ms]),
    addEventListener: (name, handler) => calls.push(['listener', name, handler]),
  };
  const result = boot((deps) => deps, win);
  assert.equal(result.doc, 'doc');
  result.every(500, 'tick');
  result.onFocus('handler');
  assert.deepEqual(calls, [['interval', 'tick', 500], ['listener', 'focus', 'handler']]);
  assert.equal(typeof result.api.call, 'function');
});

test('error text prefers the engine message', () => {
  assert.equal(errorText({ code: 1, message: 'Plain words.' }), 'Plain words.');
  assert.equal(errorText('boom'), 'boom');
});

test('buttons and guarded actions', async () => {
  const win = loadPage('join_prompt.html');
  const target = win.document.getElementById('error');
  let clicked = 0;
  const node = button(win.document, 'Go', () => clicked++);
  node.click();
  assert.equal(clicked, 1);
  target.textContent = 'old';
  await guarded(target, async () => {})();
  assert.equal(target.textContent, '');
  target.textContent = 'kept';
  await guarded(target, async () => {}, { clear: false })();
  assert.equal(target.textContent, 'kept');
});
