import assert from 'node:assert/strict';
import test from 'node:test';
import { BUTTON_CHOICES, create, parseAllowlist, parsePort } from './settings.js';
import { click, flush, labelled, mount } from '../test_helpers.js';
import { STATUS_FIXTURES, makeStatus } from '../status_fixture.js';

const CONFIG = {
  'advanced.allowlist': ['192.168.1.0/24'],
  'advanced.port': 47801,
  'mouse.buttons_4_5': 'back_forward',
  'crossing.landing_highlight': true,
  'keyboard.command_as_control': false,
  'general.start_at_login': false,
  'general.notifications': true,
  'hotkeys.panic': 'Escape*2',
  'hotkeys.quickjump': 'Ctrl+Alt+Shift+J',
  'hotkeys.enable_disable': 'Ctrl+Alt+Shift+M',
  'hotkeys.targeted_paste': 'Ctrl+Alt+Shift+V',
};
const start = (status = STATUS_FIXTURES.alone, answers = {}) =>
  mount({ create }, { answers: { 'config.get': (p) => CONFIG[p.key], 'config.set': {}, ...answers }, status });
const control = (root, key) => root.querySelector(`[data-setting="${key}"]`);

test('the allowlist is lines of network ranges and the port a whole number', () => {
  assert.deepEqual(parseAllowlist(' 10.0.0.0/8 \n\n192.168.1.0/24\n'), ['10.0.0.0/8', '192.168.1.0/24']);
  assert.equal(parsePort('47801'), 47801);
  assert.equal(parsePort(' '), null);
  assert.equal(parsePort('4.5'), null);
  assert.equal(parsePort('abc'), null);
});

test('Settings shows the current mouse buttons choice, port and allowlist', async () => {
  const { root } = await start();
  assert.equal(control(root, 'mouse.buttons_4_5').querySelector('input:checked').value, 'back_forward');
  assert.equal(control(root, 'advanced.port').value, '47801');
  assert.equal(control(root, 'advanced.allowlist').value, '192.168.1.0/24');
});

test('choosing what buttons 4 and 5 do writes mouse.buttons_4_5', async () => {
  const { win, api, root } = await start();
  const radio = control(root, 'mouse.buttons_4_5').querySelector('input[value="passthrough"]');
  radio.checked = true;
  radio.dispatchEvent(new win.Event('change', { bubbles: true }));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'config.set'), ['config.set', { key: 'mouse.buttons_4_5', value: 'passthrough', origin: 'app' }]);
  assert.equal(root.querySelector('.buttons-note').textContent, 'Saved.');
});

test('Advanced saves the allowlist and the port, and refuses a port that is not a whole number', async () => {
  const { win, api, root } = await start();
  const form = root.querySelector('.advanced-form');
  const submit = () => form.dispatchEvent(new win.Event('submit', { cancelable: true }));
  control(root, 'advanced.port').value = 'x';
  submit();
  await flush();
  assert.equal(root.querySelector('.advanced-note').textContent, 'Enter the port as a whole number from 1 to 65535.');
  assert.ok(!api.calls.some(([m]) => m === 'config.set'));
  control(root, 'advanced.port').value = '47802';
  control(root, 'advanced.allowlist').value = '10.0.0.0/8\n';
  submit();
  await flush();
  assert.deepEqual(api.calls.filter(([m]) => m === 'config.set'), [
    ['config.set', { key: 'advanced.allowlist', value: ['10.0.0.0/8'], origin: 'app' }],
    ['config.set', { key: 'advanced.port', value: 47802, origin: 'app' }],
  ]);
  assert.equal(root.querySelector('.advanced-note').textContent, 'Saved.');
});

test('a Mac that is not portable can allow control at the login screen', async () => {
  const { win, api, root } = await start(STATUS_FIXTURES.alone, { portable: false, allow_login_screen: undefined });
  const section = root.querySelector('.login-section');
  assert.equal(section.hidden, false);
  click(win, labelled(root, 'Allow control at the login screen'));
  await flush();
  assert.deepEqual(api.calls.at(-1), ['allow_login_screen']);
  assert.equal(root.querySelector('.login-note').textContent, 'Done. This Mac can now be controlled at the login screen.');
});

test('the login screen choice is hidden on portable runs and on other systems', async () => {
  const portable = await start(STATUS_FIXTURES.alone, { portable: true });
  assert.equal(portable.root.querySelector('.login-section').hidden, true);
  const windows = makeStatus({ desk: STATUS_FIXTURES.alone.desk });
  windows.computers[0].os = 'windows';
  const other = await start(windows);
  assert.equal(other.root.querySelector('.login-section').hidden, true);
});

test('a failed save is shown', async () => {
  const { win, root } = await start(STATUS_FIXTURES.alone, { 'config.set': new Error('Refused.') });
  const radio = control(root, 'mouse.buttons_4_5').querySelector('input[value="passthrough"]');
  radio.checked = true;
  radio.dispatchEvent(new win.Event('change', { bubbles: true }));
  await flush();
  assert.equal(root.querySelector('.buttons-note').textContent, 'Refused.');
});

test('#settings/<key> focuses that control', async () => {
  const { doc, section, root } = await start();
  section.open('advanced.port');
  assert.equal(doc.activeElement, control(root, 'advanced.port'));
  section.open('mouse.buttons_4_5');
  assert.equal(doc.activeElement, control(root, 'mouse.buttons_4_5').querySelector('input'));
});

test('a setting changed elsewhere is shown when its config notification arrives', async () => {
  const { emit, root } = await start();
  CONFIG['advanced.port'] = 50000;
  emit('config', { key: 'advanced.port', old: 47801, new: 50000, origin: 'cli' });
  await flush();
  CONFIG['advanced.port'] = 47801;
  assert.equal(control(root, 'advanced.port').value, '50000');
  emit('status', {});
});

test('a hotkey changed elsewhere is shown when its config notification arrives', async () => {
  const { emit, root } = await start();
  CONFIG['hotkeys.panic'] = 'F9*3';
  emit('config', { key: 'hotkeys.panic', old: 'Escape*2', new: 'F9*3', origin: 'cli' });
  await flush();
  CONFIG['hotkeys.panic'] = 'Escape*2';
  assert.equal(control(root, 'hotkeys.panic').querySelector('.current').textContent, 'F9*3');
});

test('buttons 4 and 5 offer pass through or Back and Forward', () => {
  assert.deepEqual(BUTTON_CHOICES, [['passthrough', 'Pass through unchanged'], ['back_forward', 'Back and Forward']]);
});

test('Settings shows the friendly names and the hotkeys', async () => {
  const { root } = await start(STATUS_FIXTURES.twoMacsWindowsLinux, { 'desk.rename': {} });
  assert.equal(root.querySelectorAll('[data-computer]').length, 4);
  assert.ok(control(root, 'hotkeys.panic'));
  assert.equal(control(root, 'hotkeys.quickjump').querySelector('.current').textContent, 'Ctrl+Alt+Shift+J');
  assert.ok(control(root, 'general.computer_name'));
});

const TOGGLES = [
  ['crossing.landing_highlight', 'Highlight where the pointer lands on another computer', true],
  ['keyboard.command_as_control', 'Use Command as Control on Windows and Linux', false],
  ['general.start_at_login', 'Start Mouser when I log in', false],
  ['general.notifications', 'Show notifications', true],
];

test('Settings shows each switch as saved', async () => {
  const { root } = await start();
  for (const [key, label, value] of TOGGLES) {
    const box = control(root, key);
    assert.equal(box.checked, value, key);
    assert.ok([...root.querySelectorAll('label')].some((l) => l.textContent.trim() === label && l.contains(box)), label);
  }
});

for (const [key, , value] of TOGGLES) {
  test(`switching ${key} writes it with origin app`, async () => {
    const { win, api, root } = await start();
    const box = control(root, key);
    box.checked = !value;
    box.dispatchEvent(new win.Event('change', { bubbles: true }));
    await flush();
    assert.deepEqual(api.calls.find(([m]) => m === 'config.set'), ['config.set', { key, value: !value, origin: 'app' }]);
    assert.equal(root.querySelector('.switches-note').textContent, 'Saved.');
  });
}

test('a switch the engine refuses is shown', async () => {
  const { win, root } = await start(STATUS_FIXTURES.alone, { 'config.set': new Error('Refused.') });
  const box = control(root, 'general.notifications');
  box.dispatchEvent(new win.Event('change', { bubbles: true }));
  await flush();
  assert.equal(root.querySelector('.switches-note').textContent, 'Refused.');
});

test('#settings/<key> focuses a switch, a hotkey and the names', async () => {
  const { doc, section, root } = await start();
  section.open('keyboard.command_as_control');
  assert.equal(doc.activeElement, control(root, 'keyboard.command_as_control'));
  section.open('hotkeys.panic');
  assert.equal(doc.activeElement, control(root, 'hotkeys.panic').querySelector('button'));
  section.open('general.computer_name');
  assert.equal(doc.activeElement, control(root, 'general.computer_name').querySelector('input'));
});
