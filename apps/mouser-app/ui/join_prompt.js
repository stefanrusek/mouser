// The join approval prompt: "Allow <computer> to join <desk>?" (FR-013).

import { button, el, guarded } from './dom.js';

export function promptText(request) {
  return `Allow ${request.computer.name} to join ${request.desk.name}?`;
}

export function renderRequests(doc, container, requests, onDecide) {
  container.replaceChildren(
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
          button(doc, 'Allow', () => onDecide(request, true), { class: 'primary' }),
          button(doc, 'Decline', () => onDecide(request, false)),
        ),
      ),
    ),
  );
}

export async function init({ doc, api }) {
  const container = doc.getElementById('requests');
  const errorLine = doc.getElementById('error');
  let requests = [];

  const draw = () => renderRequests(doc, container, requests, decide);
  const add = (request) => {
    if (!requests.some((r) => r.request_id === request.request_id)) requests = [...requests, request];
    draw();
  };
  const decide = guarded(errorLine, async (request, approve) => {
    await api.call('join.decide', { request_id: request.request_id, approve });
    requests = requests.filter((r) => r.request_id !== request.request_id);
    draw();
    if (requests.length === 0) await api.hideWindow();
  });

  // The window opens after the notification was sent, so ask for what is pending.
  await api.onNotification((note) => {
    if (note.method === 'join_request') add(note.params);
  });
  for (const request of await api.call('join.pending')) add(request);
  return { requests: () => requests };
}
