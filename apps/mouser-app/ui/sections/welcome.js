// Welcome: first run inside the window. The permission step and the desk step,
// then on to Desk (001 Story 1 scenario 1; Story 3 sc. 9).

import { button, el, guarded, show } from '../dom.js';
import { PERMISSIONS } from '../labels.js';
import { waitingText } from './desk.js';

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

const STEPS = ['permissions', 'desk', 'waiting', 'result'];

export function create({ doc, api, navigate }) {
  const element = el(doc, 'div', { class: 'welcome' });
  element.innerHTML = `
    <p class="lead">Use one mouse and keyboard across all your computers.</p>
    <section class="step-permissions">
      <h2>Let Mouser work on this computer</h2>
      <p class="hint">Your system asks for these once. Mouser opens the right screen for each one.</p>
      <ul class="permission-list"></ul>
      <button class="continue primary" type="button">Continue</button>
    </section>
    <section class="step-desk" hidden>
      <h2>Choose a desk</h2>
      <p class="hint">A desk is the group of computers that share your mouse and keyboard.</p>
      <ul class="desk-list"></ul>
      <p class="no-desks hint">No desks found on your network yet. Create one, or wait a moment while Mouser looks.</p>
      <form class="create-form" novalidate>
        <label>Create a new desk
          <input class="desk-name" type="text" placeholder="For example: Home office" autocomplete="off"></label>
        <button class="primary" type="submit">Create desk</button>
        <p class="form-error error" role="alert"></p>
      </form>
      <form class="address-form" novalidate>
        <label>Desk not listed? Enter the address of a computer on it
          <input class="address" type="text" placeholder="For example: 192.168.1.20" autocomplete="off"></label>
        <button type="submit">Join by address</button>
        <p class="address-error error" role="alert"></p>
      </form>
    </section>
    <section class="step-waiting" hidden>
      <h2 class="waiting-text"></h2>
      <p class="hint">Click Allow on that computer. No code or password is needed.</p>
    </section>
    <section class="step-result" hidden>
      <h2 class="result-text"></h2>
      <p class="result-help hint"></p>
      <button class="result-action primary" type="button"></button>
    </section>
    <p class="welcome-error error" role="alert"></p>`;
  const q = (selector) => element.querySelector(selector);
  const errorLine = q('.welcome-error');
  const state = { step: 'permissions', joining: null, status: null };

  const showStep = (step) => {
    state.step = step;
    for (const name of STEPS) show(q(`.step-${name}`), name === step);
  };
  const finish = (result) => {
    const member = result.kind === 'member';
    q('.result-text').textContent = member ? `Member of ${result.desk}` : 'Not admitted';
    q('.result-help').textContent = member
      ? 'This computer is on the desk. You can change things later from the Mouser icon.'
      : `${result.desk} did not let this computer join.`;
    const action = q('.result-action');
    action.textContent = member ? 'Done' : 'Choose another desk';
    action.onclick = member ? () => navigate('#desk') : chooseDesk;
    showStep('result');
  };
  const wait = (joining, text) => {
    state.joining = joining;
    q('.waiting-text').textContent = text;
    showStep('waiting');
  };

  const refreshDesks = async () => {
    const desks = await api.call('desks.discovered');
    q('.desk-list').replaceChildren(
      ...desks.map((desk) =>
        el(
          doc,
          'li',
          {},
          el(doc, 'span', { class: 'what' }, desk.name, el(doc, 'small', {}, desk.members.join(', '))),
          button(doc, 'Join', () => join(desk)),
        ),
      ),
    );
    show(q('.no-desks'), desks.length === 0);
  };
  async function chooseDesk() {
    showStep('desk');
    await refreshDesks();
  }
  const join = guarded(errorLine, async (desk) => {
    await api.call('desk.join', { desk_id: desk.desk_id });
    wait(desk, waitingText(desk));
  });

  q('.continue').addEventListener(
    'click',
    guarded(errorLine, async () => {
      const { desk } = state.status;
      if (desk && !desk.pending_join) {
        finish({ kind: 'member', desk: desk.name });
        return;
      }
      await chooseDesk();
    }),
  );
  q('.create-form').addEventListener(
    'submit',
    guarded(errorLine, async (event) => {
      event.preventDefault();
      const name = validName(q('.desk-name').value);
      q('.form-error').textContent = name ? '' : `Give your desk a name of ${NAME_MIN} to ${NAME_MAX} characters.`;
      if (!name) return;
      const desk = await api.call('desk.create', { name });
      finish({ kind: 'member', desk: desk.name });
    }),
  );
  q('.address-form').addEventListener(
    'submit',
    guarded(errorLine, async (event) => {
      event.preventDefault();
      const address = q('.address').value.trim();
      q('.address-error').textContent = address === '' ? "Type the computer's network address." : '';
      if (address === '') return;
      await api.call('desk.join', { address });
      wait({ name: `The computer at ${address}` }, `Waiting for approval from ${address}`);
    }),
  );

  return {
    element,
    update(status) {
      state.status = status;
      q('.permission-list').replaceChildren(
        ...permissionRows(status).map((row) =>
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
                  button(doc, 'Open settings', () => api.openPermissionSettings(row.kind)),
                ),
          ),
        ),
      );
      if (state.step === 'waiting' && !status.desk?.pending_join) {
        finish(
          status.desk
            ? { kind: 'member', desk: status.desk.name }
            : { kind: 'refused', desk: state.joining.name },
        );
      }
    },
  };
}
