// Settings: the mouse buttons, Advanced (port, allowlist) and, on a Mac,
// control at the login screen. Hotkeys, names and the rest are added by Story 4.

import { BUTTON_CHOICES } from '../input_settings.js';
import { button, el, guarded, show } from '../dom.js';

const BUTTONS = 'mouse.buttons_4_5';

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
  const element = el(
    doc,
    'div',
    { class: 'settings' },
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
        show(loginSection, macos && !(await api.portable()));
      },
      { clear: false },
    ),
    note(note) {
      if (note.method === 'config') load();
    },
    open(part) {
      const node = element.querySelector(`[data-setting="${part}"]`);
      (node.querySelector('input') ?? node).focus();
    },
  };
}
