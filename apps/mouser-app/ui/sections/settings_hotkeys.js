// Hotkeys: the panic key, QuickJump, enable/disable and targeted paste, each
// set by pressing the new keys (Story 4 sc. 1-2).

import { startRecording } from '../key_recorder.js';
import { button, el, guarded } from '../dom.js';

const HOTKEYS = [
  ['panic', 'Panic key', 'Press it several times to bring the pointer and keyboard back to the computer you are at.', 'panic'],
  ['quickjump', 'QuickJump', 'Opens the list of computers to jump to.', 'chord'],
  ['enable_disable', 'Pause or resume Mouser', 'Stops the pointer crossing to other computers, or lets it again.', 'chord'],
  ['targeted_paste', 'Targeted paste', 'Pastes on the computer you choose next.', 'chord'],
];

export function create({ doc, api }) {
  const win = doc.defaultView;
  const note = el(doc, 'p', { class: 'hotkeys-note error', role: 'alert' });
  // Ends the recording in progress, anywhere, and puts that hotkey's saved key back on show.
  let cancelCurrent = () => {};
  const rows = {};

  function makeRow([name, title, hint, mode]) {
    const key = `hotkeys.${name}`;
    const current = el(doc, 'span', { class: 'current' });
    const rowNote = el(doc, 'span', { class: 'note hint', role: 'alert' });
    const refusal = el(doc, 'span', { class: 'refusal error', role: 'alert' });
    let saved = '';
    let pending = null;
    const show = (text) => {
      current.textContent = text;
    };
    const save = guarded(rowNote, async (value) => {
      try {
        await api.call('config.set', { key, value });
        saved = value;
      } finally {
        cancelCurrent();
      }
    });
    const onKey = (value) => {
      if (mode === 'panic') {
        pending = value;
        show(value);
      } else {
        save(value);
      }
    };
    const done = button(doc, 'Done', () => {
      if (pending !== null) save(pending);
      else cancelCurrent();
    });
    done.hidden = true;
    const record = button(doc, 'Record', () => {
      cancelCurrent();
      pending = null;
      rowNote.textContent = '';
      show(mode === 'panic' ? 'Press the key, as many times as it should take.' : 'Press the new key combination.');
      done.hidden = mode !== 'panic';
      const stop = startRecording(win, mode, { onKey, onCancel: () => cancelCurrent() });
      cancelCurrent = () => {
        stop();
        done.hidden = true;
        show(saved);
        cancelCurrent = () => {};
      };
    });
    const node = el(
      doc,
      'div',
      { class: 'hotkey', 'data-setting': key },
      el(doc, 'h3', {}, title),
      el(doc, 'p', { class: 'hint' }, hint),
      current,
      record,
      done,
      rowNote,
      refusal,
    );
    rows[name] = {
      node,
      set(value, refused) {
        saved = value;
        show(value);
        refusal.textContent = refused ?? '';
      },
    };
    return node;
  }

  const element = el(doc, 'section', {}, el(doc, 'h2', {}, 'Hotkeys'), ...HOTKEYS.map(makeRow), note);

  const load = guarded(note, async () => {
    const refusals = await api.hotkeyRefusals();
    for (const [name] of HOTKEYS) {
      rows[name].set(await api.call('config.get', { key: `hotkeys.${name}` }), refusals[name]);
    }
  });
  load();

  return {
    element,
    note(notification) {
      if (notification.method === 'config' && notification.params.key.startsWith('hotkeys.')) load();
    },
  };
}
