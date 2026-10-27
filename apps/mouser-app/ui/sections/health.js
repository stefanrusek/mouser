// Health: one entry per computer with its connection, responsiveness,
// permissions (this computer), problems with their fixes, and what does not
// work there (Story 3 sc. 5-7, 9; FR-010).

import { button, el } from '../dom.js';
import { formatMs } from '../format.js';
import { CONNECTION_TEXT, permissionLabel } from '../labels.js';
import { renderBanner, uniqueProblems } from '../notify.js';

export function responsiveness(rttMs) {
  const time = formatMs(rttMs);
  if (rttMs <= 30) return `Responds quickly (${time})`;
  return rttMs <= 100 ? `Responds a little slowly (${time})` : `Responds slowly (${time})`;
}

const missingKind = (problem, computer) => {
  const [fix, kind] = (problem.fix ?? '').split(':');
  return fix === 'open_system_settings' && computer.permissions[kind] === 'missing';
};

export function create({ doc, api, navigate }) {
  const element = el(doc, 'div', { class: 'health' });
  const state = { opened: null };

  function renderPermissions(computer) {
    return Object.entries(computer.permissions)
      .filter(([, permission]) => permission !== 'not_applicable')
      .map(([kind, permission]) => {
        const line = el(doc, 'p', { class: 'permission' }, `${permissionLabel(kind)}: ${permission === 'granted' ? 'Granted' : 'Missing'}`);
        if (permission === 'missing') line.append(' ', button(doc, 'Open settings', () => api.openPermissionSettings(kind)));
        return line;
      });
  }

  function renderEntry(computer, status) {
    const mine = computer.connection === 'self';
    const node = el(
      doc,
      'section',
      { class: 'computer-entry', 'data-computer': computer.id },
      el(doc, 'h2', { class: 'name' }, mine ? `${computer.name} (this computer)` : computer.name),
      el(doc, 'p', { class: 'connection' }, CONNECTION_TEXT[computer.connection]),
    );
    if (computer.rtt_ms != null) node.append(el(doc, 'p', { class: 'rtt' }, responsiveness(computer.rtt_ms)));
    if (mine) node.append(...renderPermissions(computer));
    const own = status.problems.filter((p) => p.computer === computer.id && !missingKind(p, computer));
    for (const problem of uniqueProblems(own)) node.append(renderBanner(doc, api, problem, computer.name, navigate));
    if (computer.limitations.length > 0) {
      node.append(
        el(
          doc,
          'div',
          { class: 'limitations' },
          el(doc, 'h3', {}, "What doesn't work here"),
          ...computer.limitations.map((text) => el(doc, 'p', { class: 'limitation' }, text)),
        ),
      );
    }
    return node;
  }

  function highlight() {
    for (const node of element.querySelectorAll('[data-computer]')) {
      node.classList.toggle('highlight', node.getAttribute('data-computer') === state.opened);
    }
  }

  return {
    element,
    update(status) {
      element.replaceChildren(...status.computers.map((c) => renderEntry(c, status)));
      highlight();
    },
    // `#health/<computer id>`: scroll to that computer's entry and highlight it.
    open(id) {
      state.opened = id;
      highlight();
      element.querySelector(`[data-computer="${id}"]`).scrollIntoView({ block: 'center' });
    },
  };
}
