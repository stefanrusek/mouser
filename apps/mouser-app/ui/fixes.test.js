import assert from 'node:assert/strict';
import test from 'node:test';
import { fixLabel, runFix } from './fixes.js';
import { Window } from 'happy-dom';
import { click, fakeApi, flush } from './test_helpers.js';
import { renderFix } from './fixes.js';
import { renderBanner } from './notify.js';

const ID = 'ab'.repeat(16);
const problem = (fix, extra = {}) => ({ problem_id: `p:${fix}`, computer: ID, message: 'm', fix, ...extra });

test('a fix has one label', () => {
  assert.equal(fixLabel('open_system_settings:accessibility'), 'Open settings');
  assert.equal(fixLabel('approve_join'), 'Review the request');
  assert.equal(fixLabel('open_update'), 'Update now');
  assert.equal(fixLabel('cross_anyway_hint'), 'Show me how');
  assert.equal(fixLabel('open_arranger_edge'), 'Change this edge');
  assert.equal(fixLabel(`restart_elevated:${ID}`), 'Restart Mouser as administrator');
  assert.equal(fixLabel(`keep_placement:${ID}`), 'Keep this placement');
  assert.equal(fixLabel(`update_computer:${ID}`), 'Update this computer');
});

test('open_system_settings opens that permission screen', async () => {
  const api = fakeApi();
  await runFix(api, problem('open_system_settings:input_monitoring'), () => {});
  assert.deepEqual(api.calls, [['open_permission_settings', 'input_monitoring']]);
});

test('keep_placement:<id> calls layout.keep_placement for that computer', async () => {
  const api = fakeApi();
  await runFix(api, problem(`keep_placement:${ID}`), () => {});
  assert.deepEqual(api.calls, [['layout.keep_placement', { computer: ID }]]);
});

test('update_computer:<id> calls update.apply for that computer', async () => {
  const api = fakeApi();
  await runFix(api, problem(`update_computer:${ID}`), () => {});
  assert.deepEqual(api.calls, [['update.apply', { computer: ID }]]);
});

test('restart_elevated:<id> calls elevation.restart for that computer', async () => {
  const api = fakeApi();
  await runFix(api, problem(`restart_elevated:${ID}`), () => {});
  assert.deepEqual(api.calls, [['elevation.restart', { computer: ID }]]);
});

test('approve_join goes to the desk and the others are run by the app', async () => {
  const api = fakeApi();
  const went = [];
  await runFix(api, problem('approve_join'), (route) => went.push(route));
  assert.deepEqual(went, ['#desk']);
  await runFix(api, problem('open_arranger_edge'), (route) => went.push(route));
  await runFix(api, problem('open_update'), () => {});
  assert.deepEqual(api.calls, [
    ['fix', 'p:open_arranger_edge', 'open_arranger_edge'],
    ['fix', 'p:open_update', 'open_update'],
  ]);
});

test('a problem with a fix renders as one action and a failure is shown beside it', async () => {
  const win = new Window();
  const doc = win.document;
  const api = fakeApi({ 'layout.keep_placement': new Error('That did not work.') });
  const node = renderFix(doc, api, problem(`keep_placement:${ID}`), () => {});
  const buttons = node.querySelectorAll('button');
  assert.equal(buttons.length, 1);
  assert.equal(buttons[0].textContent, 'Keep this placement');
  click(win, buttons[0]);
  await flush();
  assert.equal(node.querySelector('.error').textContent, 'That did not work.');
  assert.equal(renderFix(doc, api, problem(null), () => {}), null);
});

test('restart_elevated for this computer or a peer asks for the Windows prompt on that computer', async () => {
  for (const name of ['Win PC', 'Desk PC']) {
    const api = fakeApi();
    const message = await runFix(api, problem(`restart_elevated:${ID}`), () => {}, name);
    assert.equal(message, `Approve the Windows prompt on ${name}`);
    assert.deepEqual(api.calls, [['elevation.restart', { computer: ID }]]);
  }
});

test('the restart message is shown beside the action and cleared by the next press', async () => {
  const win = new Window();
  const api = fakeApi({ 'elevation.restart': new Error('That did not work.') });
  const node = renderFix(win.document, api, problem(`restart_elevated:${ID}`), () => {}, 'Win PC');
  click(win, node.querySelector('button'));
  await flush();
  assert.equal(node.querySelector('.error').textContent, 'That did not work.');
  assert.equal(node.querySelector('.note').textContent, '');
  const ok = renderFix(win.document, fakeApi(), problem(`restart_elevated:${ID}`), () => {}, 'Win PC');
  click(win, ok.querySelector('button'));
  await flush();
  assert.equal(ok.querySelector('.note').textContent, 'Approve the Windows prompt on Win PC');
});

test('a banner passes its computer name to the fix', async () => {
  const win = new Window();
  const banner = renderBanner(win.document, fakeApi(), problem(`restart_elevated:${ID}`), 'Win PC', () => {});
  click(win, banner.querySelector('button'));
  await flush();
  assert.equal(banner.querySelector('.note').textContent, 'Approve the Windows prompt on Win PC');
});
