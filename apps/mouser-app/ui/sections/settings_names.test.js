import assert from 'node:assert/strict';
import test from 'node:test';
import { create } from './settings_names.js';
import { click, flush, labelled, mount } from '../test_helpers.js';
import { ME, STATUS_FIXTURES, STUDIO } from '../status_fixture.js';

const start = (answers = {}, status = STATUS_FIXTURES.twoMacsWindowsLinux) =>
  mount({ create }, { answers: { 'desk.rename': {}, ...answers }, status });
const row = (root, id) => root.querySelector(`[data-computer="${id}"]`);

test('every desk computer has its friendly name in an editable field, this computer first', async () => {
  const { root } = await start();
  const fields = [...root.querySelectorAll('[data-computer]')].map((r) => [r.dataset.computer, r.querySelector('input').value]);
  assert.deepEqual(fields[0], [ME, 'Desk Mac']);
  assert.deepEqual(fields[1], [STUDIO, 'Studio Mac']);
  assert.equal(fields.length, 4);
});

test('saving a name calls desk.rename with the computer and the trimmed name', async () => {
  const { api, root } = await start();
  const r = row(root, STUDIO);
  r.querySelector('input').value = '  Studio  ';
  click(root.ownerDocument.defaultView, labelled(r, 'Save'));
  await flush();
  assert.deepEqual(api.calls.at(-1), ['desk.rename', { computer: STUDIO, name: 'Studio' }]);
  assert.equal(r.querySelector('.name-note').textContent, 'Saved.');
});

test('an invalid name shows the engine error next to that field only', async () => {
  const { api, root } = await start({ 'desk.rename': new Error('A name is 1 to 64 characters.') });
  const r = row(root, ME);
  r.querySelector('input').value = '   ';
  click(root.ownerDocument.defaultView, labelled(r, 'Save'));
  await flush();
  assert.deepEqual(api.calls.at(-1), ['desk.rename', { computer: ME, name: '' }]);
  assert.equal(r.querySelector('.name-note').textContent, 'A name is 1 to 64 characters.');
  assert.equal(row(root, STUDIO).querySelector('.name-note').textContent, '');
});

test('a typed name survives a status refresh that changes no name, and a renamed computer shows its new name', async () => {
  const { section, root } = await start();
  const field = row(root, STUDIO).querySelector('input');
  field.value = 'Typing';
  section.update(STATUS_FIXTURES.twoMacsWindowsLinux);
  assert.equal(row(root, STUDIO).querySelector('input').value, 'Typing');
  const renamed = structuredClone(STATUS_FIXTURES.twoMacsWindowsLinux);
  renamed.computers[1].name = 'Studio';
  section.update(renamed);
  assert.equal(row(root, STUDIO).querySelector('input').value, 'Studio');
});

test('without a status the section has no rows', async () => {
  const { root } = await mount({ create }, {});
  assert.equal(root.querySelectorAll('[data-computer]').length, 0);
});
