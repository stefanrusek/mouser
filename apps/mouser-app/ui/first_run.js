// The first-run window: permissions, then choosing or creating a desk.

import { button, el, guarded, show } from './dom.js';
import { trackProblems } from './notify.js';

export const PERMISSIONS = {
  accessibility: {
    title: 'Accessibility',
    why: 'Lets Mouser move the pointer and type on this computer.',
  },
  input_monitoring: {
    title: 'Input Monitoring',
    why: 'Lets Mouser see your mouse and keyboard on this computer.',
  },
  screen_recording: {
    title: 'Screen Recording',
    why: 'Lets Mouser see where full-screen windows are.',
  },
  full_disk_access: {
    title: 'Full Disk Access',
    why: 'Lets Mouser copy files to and from this computer.',
  },
  input_capture_portal: {
    title: 'Input capture',
    why: "Lets Mouser capture your mouse and keyboard through the desktop's input portal.",
  },
  remote_desktop_portal: {
    title: 'Remote control',
    why: "Lets Mouser control this computer through the desktop's remote-control portal.",
  },
};

export const NAME_MIN = 1;
export const NAME_MAX = 64;

// One row per permission this computer needs, in a fixed order.
export function permissionRows(status) {
  const me = status.computers.find((computer) => computer.connection === 'self');
  return Object.entries(PERMISSIONS)
    .filter(([kind]) => me.permissions[kind] && me.permissions[kind] !== 'not_applicable')
    .map(([kind, text]) => ({ kind, ...text, granted: me.permissions[kind] === 'granted' }));
}

// The desk name to send, or null when it is not 1 to 64 characters.
export function validName(raw) {
  const name = raw.trim();
  const length = [...name].length;
  return length >= NAME_MIN && length <= NAME_MAX ? name : null;
}

export function waitingText(desk) {
  const where = desk.members.length ? desk.members.join(' or ') : `a computer in ${desk.name}`;
  return `Waiting for approval on ${where}`;
}

export function renderPermissions(doc, list, rows, onOpen) {
  list.replaceChildren(
    ...rows.map((row) =>
      el(
        doc,
        'li',
        {},
        el(doc, 'span', { class: 'what' }, row.title, el(doc, 'small', {}, row.why)),
        row.granted
          ? el(doc, 'span', { class: 'granted' }, 'Granted')
          : el(
              doc,
              'span',
              { class: 'row' },
              el(doc, 'span', { class: 'missing' }, 'Missing'),
              button(doc, 'Open settings', () => onOpen(row.kind)),
            ),
      ),
    ),
  );
}

export function renderDesks(doc, list, desks, onJoin) {
  list.replaceChildren(
    ...desks.map((desk) =>
      el(
        doc,
        'li',
        {},
        el(doc, 'span', { class: 'what' }, desk.name, el(doc, 'small', {}, desk.members.join(', '))),
        button(doc, 'Join', () => onJoin(desk)),
      ),
    ),
  );
}

const STEPS = ['permissions', 'desk', 'waiting', 'result'];

export function showStep(doc, step) {
  for (const name of STEPS) show(doc.getElementById(`step-${name}`), name === step);
}

export function showResult(doc, result, onAction) {
  const member = result.kind === 'member';
  doc.getElementById('result-text').textContent = member
    ? `Member of ${result.desk}`
    : 'Not admitted';
  doc.getElementById('result-help').textContent = member
    ? 'This computer is on the desk. You can change things later from the Mouser icon.'
    : `${result.desk} did not let this computer join.`;
  const action = doc.getElementById('result-action');
  action.textContent = member ? 'Done' : 'Choose another desk';
  action.onclick = onAction;
  showStep(doc, 'result');
}

export async function init({ doc, api, every, onFocus }) {
  const errorLine = doc.getElementById('error');
  const formError = doc.getElementById('form-error');
  const addressError = doc.getElementById('address-error');
  const nameInput = doc.getElementById('desk-name');
  const problems = trackProblems(doc.getElementById('problems'), api, []);
  const state = { step: 'permissions', joining: null };

  const chooseDesk = () => {
    state.step = 'desk';
    showStep(doc, 'desk');
  };
  const finish = (result) => {
    state.step = 'result';
    showResult(doc, result, result.kind === 'member' ? () => api.hideWindow() : chooseDesk);
  };

  const wait = (joining, text) => {
    state.joining = joining;
    state.step = 'waiting';
    doc.getElementById('waiting-text').textContent = text;
    showStep(doc, 'waiting');
  };
  const join = guarded(errorLine, async (desk) => {
    await api.call('desk.join', { desk_id: desk.desk_id });
    wait(desk, waitingText(desk));
  });

  const refreshDesks = async () => {
    const desks = await api.call('desks.discovered');
    renderDesks(doc, doc.getElementById('desk-list'), desks, join);
    show(doc.getElementById('no-desks'), desks.length === 0);
  };

  const refresh = async () => {
    const status = await api.call('status');
    problems.set(status.problems);
    renderPermissions(doc, doc.getElementById('permission-list'), permissionRows(status), (kind) =>
      api.openPermissionSettings(kind),
    );
    if (state.step === 'waiting' && !status.desk?.pending_join) {
      finish(
        status.desk
          ? { kind: 'member', desk: status.desk.name }
          : { kind: 'refused', desk: state.joining.name },
      );
    }
    if (state.step === 'desk') await refreshDesks();
    return status;
  };
  const tick = guarded(errorLine, refresh, { clear: false });

  doc.getElementById('continue').addEventListener(
    'click',
    guarded(errorLine, async () => {
      const status = await refresh();
      if (status.desk && !status.desk.pending_join) {
        finish({ kind: 'member', desk: status.desk.name });
        return;
      }
      chooseDesk();
      await refreshDesks();
    }),
  );

  doc.getElementById('create-form').addEventListener(
    'submit',
    guarded(errorLine, async (event) => {
      event.preventDefault();
      const name = validName(nameInput.value);
      formError.textContent = name
        ? ''
        : `Give your desk a name of ${NAME_MIN} to ${NAME_MAX} characters.`;
      if (!name) return;
      const desk = await api.call('desk.create', { name });
      finish({ kind: 'member', desk: desk.name });
    }),
  );

  doc.getElementById('address-form').addEventListener(
    'submit',
    guarded(errorLine, async (event) => {
      event.preventDefault();
      const address = doc.getElementById('address').value.trim();
      addressError.textContent = address === '' ? "Type the computer's network address." : '';
      if (address === '') return;
      await api.call('desk.join', { address });
      wait({ name: `The computer at ${address}` }, `Waiting for approval from ${address}`);
    }),
  );

  await api.onNotification((note) => {
    problems.update(note);
    if (note.method === 'status') tick();
  });
  every(2000, tick);
  onFocus(tick);
  await tick();
  return { state, tick };
}
