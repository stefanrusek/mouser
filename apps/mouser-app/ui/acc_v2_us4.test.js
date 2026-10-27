// Acceptance tests for 002 Story 4, "tailor hotkeys, names and pointer behavior from the
// Mouser window": the real window (index.html with every section) over fixtures. Each test
// is named with its scenario's Given/When/Then text from spec.md. What the engine does with
// a setting (the panic key returning control, a combination acting) is core's acceptance.
import assert from 'node:assert/strict';
import test from 'node:test';
import { init } from './nav.js';
import { REGISTRY } from './sections/registry.js';
import { click, fakeApi, flush, labelled, loadPage } from './test_helpers.js';
import { STATUS_FIXTURES, STUDIO } from './status_fixture.js';

const DEFAULTS = {
  'advanced.allowlist': [],
  'advanced.port': 47801,
  'mouse.buttons_4_5': 'passthrough',
  'crossing.landing_highlight': true,
  'keyboard.command_as_control': true,
  'general.start_at_login': false,
  'general.notifications': true,
  'hotkeys.panic': 'Escape*2',
  'hotkeys.quickjump': 'Ctrl+Alt+Shift+J',
  'hotkeys.enable_disable': 'Ctrl+Alt+Shift+M',
  'hotkeys.targeted_paste': 'Ctrl+Alt+Shift+V',
};

async function openSettings(config = {}, answers = {}) {
  const win = loadPage('index.html');
  win.fetch = async () => ({ ok: true, text: async () => '' });
  win.location.hash = '#settings';
  const status = structuredClone(STATUS_FIXTURES.twoMacsWindowsLinux);
  const values = { ...DEFAULTS, ...config };
  const api = fakeApi({
    status: () => status,
    'config.get': (p) => values[p.key],
    'config.set': (p) => {
      values[p.key] = p.value;
      return {};
    },
    'desk.rename': (p) => {
      status.computers.find((c) => c.id === p.computer).name = p.name;
      return {};
    },
    'layout.get': { placements: [], edge_options: [], equalize_dimensions: false, block_fullscreen: true, displays: [] },
    'desks.discovered': [],
    'join.pending': [],
    transfers: [],
    portable: false,
    ...answers,
  });
  await init({ doc: win.document, api, onFocus: () => {}, sections: REGISTRY });
  await flush();
  await flush();
  const settings = win.document.getElementById('section-settings');
  const press = (init) => win.dispatchEvent(new win.KeyboardEvent('keydown', { cancelable: true, ...init }));
  const sets = () => api.calls.filter(([m]) => m === 'config.set');
  return { win, doc: win.document, api, settings, press, sets };
}
const hotkey = (settings, name) => settings.querySelector(`[data-setting="hotkeys.${name}"]`);

test('Given Settings is shown, When the desk owner records a new key and number of presses for the panic key, Then pressing that key that many times returns the pointer and keyboard to the computer the desk user is at, and the previous panic key no longer does.', async () => {
  const { win, settings, press, sets } = await openSettings();
  const panic = hotkey(settings, 'panic');
  assert.equal(panic.querySelector('.current').textContent, 'Escape*2');
  click(win, labelled(panic, 'Record'));
  press({ code: 'F8' });
  press({ code: 'F8' });
  press({ code: 'F8' });
  click(win, labelled(panic, 'Done'));
  await flush();
  assert.deepEqual(sets(), [['config.set', { key: 'hotkeys.panic', value: 'F8*3', origin: 'app' }]]);
  assert.equal(panic.querySelector('.current').textContent, 'F8*3');
});

test('Given Settings is shown, When the desk owner records a new key combination for QuickJump, enable/disable, or targeted paste by pressing it, Then that combination performs the action on this computer and the previous one no longer does.', async () => {
  const { win, settings, press, sets } = await openSettings();
  const wanted = { quickjump: ['KeyK', 'Ctrl+Alt+K'], enable_disable: ['KeyP', 'Ctrl+Alt+P'], targeted_paste: ['KeyB', 'Ctrl+Alt+B'] };
  for (const [name, [code]] of Object.entries(wanted)) {
    click(win, labelled(hotkey(settings, name), 'Record'));
    press({ code, ctrlKey: true, altKey: true });
    await flush();
  }
  assert.deepEqual(
    sets(),
    Object.entries(wanted).map(([name, [, value]]) => ['config.set', { key: `hotkeys.${name}`, value, origin: 'app' }]),
  );
  for (const [name, [, value]] of Object.entries(wanted)) assert.equal(hotkey(settings, name).querySelector('.current').textContent, value);
});

test('Given a desk computer, When the desk owner gives it a friendly name from any desk computer\'s window, Then every desk computer shows that name wherever it names that computer, including the arranger, Health, notifications and the command-line tool\'s status.', async () => {
  const { win, doc, api, settings } = await openSettings();
  const row = settings.querySelector(`[data-computer="${STUDIO}"]`);
  row.querySelector('input').value = 'Studio upstairs';
  click(win, labelled(row, 'Save'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'desk.rename'), ['desk.rename', { computer: STUDIO, name: 'Studio upstairs' }]);
  // The engine shares the name; the window shows it wherever it names the computer.
  api.emit('status', {});
  await flush();
  click(win, [...doc.querySelectorAll('#nav a')].find((a) => a.textContent === 'Health'));
  assert.ok([...doc.querySelectorAll('#section-health .name')].some((n) => n.textContent === 'Studio upstairs'));
  assert.equal(settings.querySelector(`[data-computer="${STUDIO}"] input`).value, 'Studio upstairs');
});

test('Given the landing highlight is turned off, When the pointer crosses to another computer, Then no highlight is shown where it lands (amends 001 Story 2 scenario 9, which stays the default).', async () => {
  const { win, settings, sets } = await openSettings();
  const box = settings.querySelector('[data-setting="crossing.landing_highlight"]');
  assert.equal(box.checked, true, 'on by default');
  box.checked = false;
  box.dispatchEvent(new win.Event('change', { bubbles: true }));
  await flush();
  assert.deepEqual(sets(), [['config.set', { key: 'crossing.landing_highlight', value: false, origin: 'app' }]]);
});

test('Given "Use Command as Control on Windows and Linux" is on (the default, 001 Story 2 scenario 4), When the desk user at a Mac presses Command with another key on a Windows or Linux computer, Then it acts as Control with that key.', async () => {
  const { settings } = await openSettings();
  const box = settings.querySelector('[data-setting="keyboard.command_as_control"]');
  assert.equal(box.checked, true);
  assert.ok([...settings.querySelectorAll('label')].some((l) => l.textContent.trim() === 'Use Command as Control on Windows and Linux' && l.contains(box)));
});

test('Given "Use Command as Control on Windows and Linux" is off, When the desk user at a Mac presses Command or Control with another key on a Windows or Linux computer, Then Command acts as the Windows key and Control acts as Control.', async () => {
  const { win, settings, sets } = await openSettings({ 'keyboard.command_as_control': false });
  const box = settings.querySelector('[data-setting="keyboard.command_as_control"]');
  assert.equal(box.checked, false);
  box.checked = true;
  box.dispatchEvent(new win.Event('change', { bubbles: true }));
  await flush();
  box.checked = false;
  box.dispatchEvent(new win.Event('change', { bubbles: true }));
  await flush();
  assert.deepEqual(sets().map(([, p]) => p.value), [true, false]);
  assert.ok(sets().every(([, p]) => p.key === 'keyboard.command_as_control' && p.origin === 'app'));
});
