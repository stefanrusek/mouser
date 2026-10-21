// Input settings: what mouse buttons 4 and 5 do on the other computers.

import { el, guarded } from './dom.js';

const KEY = 'mouse.buttons_4_5';

export const BUTTON_CHOICES = [
  ['passthrough', 'Pass through unchanged'],
  ['back_forward', 'Back and Forward'],
];

export async function init({ doc, api }) {
  const note = doc.getElementById('buttons-note');
  const save = guarded(note, async (value) => {
    await api.call('config.set', { key: KEY, value });
    note.textContent = 'Saved.';
  });
  const saved = await api.call('config.get', { key: KEY });
  doc.getElementById('buttons-choices').replaceChildren(
    ...BUTTON_CHOICES.map(([value, label]) => {
      const input = el(doc, 'input', { type: 'radio', name: 'buttons', value });
      input.checked = value === saved;
      input.addEventListener('change', () => save(value));
      return el(doc, 'label', { class: 'choice' }, input, ` ${label}`);
    }),
  );
}
