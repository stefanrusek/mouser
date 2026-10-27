// Friendly names: every desk computer's name, editable from any computer's
// window (Story 4 sc. 3). The engine checks the name (1-64 characters) and
// shares the change with the desk.

import { button, el, guarded } from '../dom.js';

export function create({ doc, api }) {
  const list = el(doc, 'div', { class: 'names' });
  const element = el(
    doc,
    'section',
    { 'data-setting': 'general.computer_name' },
    el(doc, 'h2', {}, 'Computer names'),
    el(doc, 'p', { class: 'hint' }, 'The name every computer on your desk shows for each computer.'),
    list,
  );
  let shown = '';

  function row(computer) {
    const field = el(doc, 'input', { type: 'text', value: computer.name, 'aria-label': `Name of ${computer.name}` });
    const note = el(doc, 'span', { class: 'name-note hint', role: 'alert' });
    const save = guarded(note, async () => {
      await api.call('desk.rename', { computer: computer.id, name: field.value.trim() });
      note.textContent = 'Saved.';
    });
    return el(doc, 'div', { 'data-computer': computer.id }, field, button(doc, 'Save', save), note);
  }

  return {
    element,
    // Rebuilt only when a name or the list of computers changed, so typing is not lost.
    update(status) {
      const signature = JSON.stringify(status.computers.map((c) => [c.id, c.name]));
      if (signature === shown) return;
      shown = signature;
      list.replaceChildren(...status.computers.map(row));
    },
  };
}
