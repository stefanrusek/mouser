// Desk settings: members, leaving or moving, and Advanced.

import { button, el, guarded, show } from './dom.js';
import { trackProblems } from './notify.js';
import { waitingText } from './first_run.js';

const CONNECTION_TEXT = {
  self: 'This computer',
  connected: 'Connected',
  reconnecting: 'Reconnecting',
  offline: 'Offline',
  incompatible: 'Needs an update',
};

export function renderMembers(doc, list, status, onRemove) {
  list.replaceChildren(
    ...status.computers.map((computer) =>
      el(
        doc,
        'li',
        {},
        el(doc, 'span', { class: 'what' }, computer.name, el(doc, 'small', {}, CONNECTION_TEXT[computer.connection])),
        computer.connection === 'self'
          ? ''
          : button(doc, 'Remove', () => onRemove(computer), { class: 'danger' }),
      ),
    ),
  );
}

export function renderOtherDesks(doc, list, desks, onMove, joining) {
  list.replaceChildren(
    ...desks.map((desk) =>
      el(
        doc,
        'li',
        {},
        el(doc, 'span', { class: 'what' }, desk.name, el(doc, 'small', {}, desk.members.join(', '))),
        button(doc, joining ? 'Join' : 'Move here', () => onMove(desk)),
      ),
    ),
  );
}

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

export function renderStatus(doc, status) {
  const desk = status.desk;
  doc.getElementById('desk-title').textContent = desk ? desk.name : 'Mouser';
  doc.getElementById('desk-status').textContent = !desk
    ? 'This computer is not on a desk yet.'
    : desk.pending_join
      ? `Waiting for approval to join ${desk.name}.`
      : 'These computers share one mouse and keyboard.';
  const member = Boolean(desk) && !desk.pending_join;
  show(doc.getElementById('member-list'), member);
  show(doc.getElementById('leave'), member);
  doc.getElementById('move-title').textContent = desk ? 'Move to another desk' : 'Join a desk';
}

export async function init({ doc, api, every, onFocus }) {
  const errorLine = doc.getElementById('error');
  const moveNote = doc.getElementById('move-note');
  const advancedNote = doc.getElementById('advanced-note');
  const problems = trackProblems(doc.getElementById('problems'), api, []);
  const current = { desk: null };

  const move = guarded(errorLine, async (desk) => {
    await api.call(current.desk ? 'desk.move' : 'desk.join', { desk_id: desk.desk_id });
    moveNote.textContent = waitingText(desk);
  });
  const remove = guarded(errorLine, async (computer) => {
    await api.call('desk.remove_member', { computer_id: computer.id });
    await refresh();
  });
  async function refresh() {
    const status = await api.call('status');
    current.desk = status.desk;
    problems.set(status.problems, status.computer.id);
    renderStatus(doc, status);
    renderMembers(doc, doc.getElementById('member-list'), status, remove);
    const desks = (await api.call('desks.discovered')).filter((d) => d.desk_id !== status.desk?.id);
    renderOtherDesks(doc, doc.getElementById('other-desks'), desks, move, !status.desk);
    show(doc.getElementById('no-other-desks'), desks.length === 0);
  }
  const tick = guarded(errorLine, refresh, { clear: false });

  doc.getElementById('leave').addEventListener(
    'click',
    guarded(errorLine, async () => {
      await api.call('desk.leave');
      await refresh();
    }),
  );

  const loadAdvanced = async () => {
    doc.getElementById('allowlist').value = (await api.call('config.get', { key: 'advanced.allowlist' })).join('\n');
    doc.getElementById('port').value = String(await api.call('config.get', { key: 'advanced.port' }));
  };
  doc.getElementById('advanced-form').addEventListener(
    'submit',
    guarded(advancedNote, async (event) => {
      event.preventDefault();
      const port = parsePort(doc.getElementById('port').value);
      if (port === null) {
        advancedNote.textContent = 'Enter the port as a whole number from 1 to 65535.';
        return;
      }
      await api.call('config.set', {
        key: 'advanced.allowlist',
        value: parseAllowlist(doc.getElementById('allowlist').value),
      });
      await api.call('config.set', { key: 'advanced.port', value: port });
      advancedNote.textContent = 'Saved.';
    }),
  );

  await api.onNotification((note) => {
    problems.update(note);
    if (note.method === 'status') tick();
  });
  every(3000, tick);
  onFocus(tick);
  await guarded(errorLine, async () => {
    await refresh();
    await loadAdvanced();
  })();
  return { tick };
}
