// The join approval card: "Allow <computer> to join <desk>?" (001 FR-013), now in Desk.

import { button, el, guarded } from '../dom.js';

export const promptText = (request) => `Allow ${request.computer.name} to join ${request.desk.name}?`;

export function create({ doc, api }) {
  const element = el(doc, 'div', { class: 'join-requests' });
  const errorLine = el(doc, 'p', { class: 'error', role: 'alert' });
  let requests = [];

  const draw = () =>
    element.replaceChildren(
      ...requests.map((request) =>
        el(
          doc,
          'div',
          { class: 'card' },
          el(doc, 'h2', {}, promptText(request)),
          el(doc, 'p', { class: 'hint' }, 'It will share your mouse and keyboard with this desk.'),
          el(
            doc,
            'div',
            { class: 'row' },
            button(doc, 'Allow', () => decide(request, true), { class: 'primary' }),
            button(doc, 'Decline', () => decide(request, false)),
          ),
        ),
      ),
      errorLine,
    );
  const add = (request) => {
    if (!requests.some((r) => r.request_id === request.request_id)) requests = [...requests, request];
    draw();
  };
  const decide = guarded(errorLine, async (request, approve) => {
    await api.call('join.decide', { request_id: request.request_id, approve });
    requests = requests.filter((r) => r.request_id !== request.request_id);
    draw();
  });

  draw();
  // The window may open after the notification was sent, so ask for what is pending.
  guarded(errorLine, async () => {
    for (const request of await api.call('join.pending')) add(request);
  })();
  return {
    element,
    update() {},
    note(note) {
      if (note.method === 'join_request') add(note.params);
    },
  };
}
