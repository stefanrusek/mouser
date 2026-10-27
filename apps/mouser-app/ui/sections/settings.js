// Settings: computer names, hotkeys, the switches (landing highlight, Command
// as Control, start at login, notifications), the mouse buttons, Advanced
// (port, allowlist) and, on a Mac, control at the login screen.

import { button, el, guarded, show } from '../dom.js';
import * as hotkeys from './settings_hotkeys.js';
import * as names from './settings_names.js';

const BUTTONS = 'mouse.buttons_4_5';

// What buttons 4 and 5 can do on the other computers.
export const BUTTON_CHOICES = [
  ['passthrough', 'Pass through unchanged'],
  ['back_forward', 'Back and Forward'],
];

const SWITCHES = [
  ['crossing.landing_highlight', 'Highlight where the pointer lands on another computer'],
  ['keyboard.command_as_control', 'Use Command as Control on Windows and Linux'],
  ['general.start_at_login', 'Start Mouser when I log in'],
  ['general.notifications', 'Show notifications'],
];

// The allowlist textarea's lines as a list of network ranges.
export function parseAllowlist(text) {
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '');
}

// A port number from the field, or null when it is not a whole number.
export function parsePort(text) {
  const port = Number(text);
  return text.trim() !== '' && Number.isInteger(port) ? port : null;
}

export function create({ doc, api }) {
  const choices = el(doc, 'div', { 'data-setting': BUTTONS });
  const buttonsNote = el(doc, 'p', { class: 'buttons-note hint', role: 'alert' });
  const allowlist = el(doc, 'textarea', { rows: '3', placeholder: '192.168.1.0/24', 'data-setting': 'advanced.allowlist' });
  const port = el(doc, 'input', { type: 'number', min: '1', max: '65535', 'data-setting': 'advanced.port' });
  const advancedNote = el(doc, 'p', { class: 'advanced-note hint' });
  const form = el(
    doc,
    'form',
    { class: 'advanced-form', novalidate: '' },
    el(doc, 'label', {}, 'Computers allowed to join', allowlist),
    el(doc, 'p', { class: 'hint' }, 'One network range per line. Leave it empty to allow any computer on your network.'),
    el(doc, 'label', {}, 'Network port', port),
    el(doc, 'p', { class: 'hint' }, 'The UDP port Mouser uses to talk to your other computers.'),
    button(doc, 'Save', () => {}, { type: 'submit', class: 'primary' }),
    advancedNote,
  );
  const loginNote = el(doc, 'p', { class: 'login-note hint' });
  const loginButton = button(doc, 'Allow control at the login screen', () => allowLogin(), { 'data-setting': 'login-screen' });
  const loginSection = el(
    doc,
    'section',
    { class: 'login-section' },
    el(doc, 'h2', {}, 'Login screen'),
    el(doc, 'p', { class: 'hint' }, "Let your other computers' mouse and keyboard control this Mac at the login screen. You will be asked for an administrator password once."),
    loginButton,
    loginNote,
  );
  loginSection.hidden = true;
  const errorLine = el(doc, 'p', { class: 'error', role: 'alert' });
  const switchesNote = el(doc, 'p', { class: 'switches-note hint', role: 'alert' });
  const boxes = SWITCHES.map(([key, label]) => {
    const box = el(doc, 'input', { type: 'checkbox', 'data-setting': key });
    box.addEventListener('change', () => saveSwitch(key, box.checked));
    return [key, el(doc, 'label', { class: 'choice' }, box, ` ${label}`), box];
  });
  const namesSection = names.create({ doc, api });
  const hotkeysSection = hotkeys.create({ doc, api });
  const element = el(
    doc,
    'div',
    { class: 'settings' },
    namesSection.element,
    hotkeysSection.element,
    el(doc, 'section', {}, el(doc, 'h2', {}, 'Behavior'), ...boxes.map(([, label]) => label), switchesNote),
    el(
      doc,
      'section',
      {},
      el(doc, 'h2', {}, 'Mouse'),
      el(doc, 'p', { class: 'hint' }, 'What buttons 4 and 5 (usually the thumb buttons) do when you press them while using another computer.'),
      choices,
      buttonsNote,
    ),
    loginSection,
    el(doc, 'details', { class: 'advanced' }, el(doc, 'summary', {}, 'Advanced'), form),
    errorLine,
  );

  const saveButtons = guarded(buttonsNote, async (value) => {
    await api.call('config.set', { key: BUTTONS, value });
    buttonsNote.textContent = 'Saved.';
  });
  const saveSwitch = guarded(switchesNote, async (key, value) => {
    await api.call('config.set', { key, value });
    switchesNote.textContent = 'Saved.';
  });
  const allowLogin = guarded(loginNote, async () => {
    await api.allowLoginScreen();
    loginNote.textContent = 'Done. This Mac can now be controlled at the login screen.';
  });
  const load = guarded(errorLine, async () => {
    const saved = await api.call('config.get', { key: BUTTONS });
    choices.replaceChildren(
      ...BUTTON_CHOICES.map(([value, label]) => {
        const input = el(doc, 'input', { type: 'radio', name: 'buttons', value });
        input.checked = value === saved;
        input.addEventListener('change', () => saveButtons(value));
        return el(doc, 'label', { class: 'choice' }, input, ` ${label}`);
      }),
    );
    for (const [key, , box] of boxes) box.checked = await api.call('config.get', { key });
    allowlist.value = (await api.call('config.get', { key: 'advanced.allowlist' })).join('\n');
    port.value = String(await api.call('config.get', { key: 'advanced.port' }));
  });
  form.addEventListener(
    'submit',
    guarded(advancedNote, async (event) => {
      event.preventDefault();
      const number = parsePort(port.value);
      if (number === null) {
        advancedNote.textContent = 'Enter the port as a whole number from 1 to 65535.';
        return;
      }
      await api.call('config.set', { key: 'advanced.allowlist', value: parseAllowlist(allowlist.value) });
      await api.call('config.set', { key: 'advanced.port', value: number });
      advancedNote.textContent = 'Saved.';
    }),
  );

  load();
  return {
    element,
    update: guarded(
      errorLine,
      async (status) => {
        const macos = status.computers.find((c) => c.connection === 'self').os === 'macos';
        namesSection.update(status);
        show(loginSection, macos && !(await api.portable()));
      },
      { clear: false },
    ),
    note(note) {
      if (note.method === 'config') load();
      hotkeysSection.note(note);
    },
    open(part) {
      const node = element.querySelector(`[data-setting="${part}"]`);
      (node.querySelector('input, button') ?? node).focus();
    },
  };
}
