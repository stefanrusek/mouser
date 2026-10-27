// Desk: the home overview. The desk's name and every computer on it, who has
// the pointer, how many problems need the owner, and joining, leaving and moving.

import { button, el, guarded, show } from '../dom.js';
import { CONNECTION_TEXT, OS_NAMES } from '../labels.js';
import { create as createJoin } from './desk_join.js';
import { create as createTransfer } from './desk_transfer.js';

export function waitingText(desk) {
  const where = desk.members.length ? desk.members.join(' or ') : `a computer in ${desk.name}`;
  return `Waiting for approval on ${where}`;
}

const plural = (n, noun) => (n === 1 ? `1 ${noun}` : `${n} ${noun}s`);

function renderComputer(doc, computer, status, onRemove) {
  const me = status.computers.find((c) => c.connection === 'self');
  const mine = computer.connection === 'self';
  const open = status.problems.filter((p) => p.computer === computer.id).length;
  const system = computer.os_version ? `${OS_NAMES[computer.os]} ${computer.os_version}` : OS_NAMES[computer.os];
  const version =
    computer.version === me.version
      ? `Mouser ${computer.version}`
      : `Mouser ${computer.version} (this computer: ${me.version})`;
  const what = el(
    doc,
    'span',
    { class: 'what' },
    el(doc, 'strong', { class: 'name' }, mine ? `${computer.name} (this computer)` : computer.name),
    el(doc, 'small', { class: 'os' }, system),
    el(doc, 'small', { class: 'version' }, version),
    el(doc, 'small', { class: 'connection' }, CONNECTION_TEXT[computer.connection]),
  );
  if (status.active_computer === computer.id) what.append(el(doc, 'small', { class: 'pointer' }, 'Has the pointer'));
  if (open > 0) {
    what.append(el(doc, 'a', { class: 'problems', href: `#health/${computer.id}` }, plural(open, 'problem')));
  }
  return el(
    doc,
    'li',
    { class: 'computer', 'data-computer': computer.id },
    what,
    mine ? '' : button(doc, 'Remove', () => onRemove(computer), { class: 'danger' }),
  );
}

export function create({ doc, api }) {
  const join = createJoin({ doc, api });
  const transfer = createTransfer({ doc, api });
  const name = el(doc, 'h2', { class: 'desk-name' });
  const state = el(doc, 'p', { class: 'desk-state hint' });
  const engineState = el(doc, 'p', { class: 'engine-state' });
  const list = el(doc, 'ul', { class: 'computers' });
  const errorLine = el(doc, 'p', { class: 'error desk-error', role: 'alert' });
  const leave = button(doc, 'Leave this desk', () => leaveDesk(), { class: 'danger' });
  const otherTitle = el(doc, 'h2', { class: 'other-desks-title' });
  const otherDesks = el(doc, 'ul', { class: 'other-desks' });
  const none = el(doc, 'p', { class: 'hint' }, 'No other desks found on your network.');
  const moveNote = el(doc, 'p', { class: 'move-note hint' });
  const element = el(
    doc,
    'div',
    { class: 'desk' },
    join.element,
    transfer.element,
    el(doc, 'section', {}, name, state, engineState, list, leave),
    el(doc, 'section', {}, otherTitle, otherDesks, none, moveNote),
    errorLine,
  );
  const current = { status: null };

  const remove = guarded(errorLine, async (computer) => {
    await api.call('desk.remove_member', { computer_id: computer.id });
  });
  const leaveDesk = guarded(errorLine, async () => {
    await api.call('desk.leave');
  });
  const move = guarded(errorLine, async (desk) => {
    await api.call(current.status.desk ? 'desk.move' : 'desk.join', { desk_id: desk.desk_id });
    moveNote.textContent = waitingText(desk);
  });

  const refresh = guarded(
    errorLine,
    async (status) => {
      current.status = status;
      const { desk } = status;
      name.textContent = desk ? desk.name : 'Mouser';
      state.textContent = !desk
        ? 'This computer is not on a desk yet.'
        : desk.pending_join
          ? `Waiting for approval to join ${desk.name}.`
          : 'These computers share one mouse and keyboard.';
      engineState.textContent = `Mouser is ${status.state}`;
      show(leave, Boolean(desk) && !desk.pending_join);
      list.replaceChildren(...status.computers.map((c) => renderComputer(doc, c, status, remove)));
      otherTitle.textContent = desk ? 'Move to another desk' : 'Join a desk';
      const desks = (await api.call('desks.discovered')).filter((d) => d.desk_id !== desk?.id);
      otherDesks.replaceChildren(
        ...desks.map((d) =>
          el(
            doc,
            'li',
            {},
            el(doc, 'span', { class: 'what' }, el(doc, 'strong', {}, d.name), el(doc, 'small', {}, d.members.join(', '))),
            button(doc, desk ? 'Move here' : 'Join', () => move(d)),
          ),
        ),
      );
      show(none, desks.length === 0);
    },
    { clear: false },
  );

  return {
    element,
    update: (status) => refresh(status),
    note(note) {
      join.note(note);
      transfer.note(note);
    },
  };
}
