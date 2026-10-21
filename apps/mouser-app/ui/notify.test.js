import assert from 'node:assert/strict';
import test from 'node:test';
import { Window } from 'happy-dom';
import { applyProblem, fixLabel, renderProblem, renderProblems, trackProblems } from './notify.js';
import { click, fakeApi, flush } from './test_helpers.js';
import { problem } from './status_fixture.js';

const page = () => {
  const win = new Window();
  win.document.body.innerHTML = '<div id="box"></div>';
  return { win, box: win.document.getElementById('box') };
};

test('every fix action has a plain-language label', () => {
  assert.equal(fixLabel('open_system_settings:accessibility'), 'Open settings');
  assert.equal(fixLabel('approve_join'), 'Review the request');
  assert.equal(fixLabel('open_update'), 'Update now');
  assert.equal(fixLabel('cross_anyway_hint'), 'Show me how');
  assert.equal(fixLabel('open_arranger_edge'), 'Change this edge');
});

test('a problem shows its message and no button when there is no fix', () => {
  const { win } = page();
  const node = renderProblem(win.document, problem('a', 'Studio is offline.'), async () => {});
  assert.equal(node.querySelector('p').textContent, 'Studio is offline.');
  assert.equal(node.querySelectorAll('button').length, 0);
});

test('a problem with a fix shows exactly one fix button that runs the fix', async () => {
  const { win, box } = page();
  const fixes = [];
  renderProblems(box, [problem('a', 'Mouser needs Accessibility access.', 'open_system_settings:accessibility')], async (fix) => {
    fixes.push(fix);
  });
  const buttons = box.querySelectorAll('button');
  assert.equal(buttons.length, 1);
  assert.equal(buttons[0].textContent, 'Open settings');
  click(win, buttons[0]);
  await flush();
  assert.deepEqual(fixes, ['open_system_settings:accessibility']);
});

test('a fix that fails says why next to its button', async () => {
  const { win, box } = page();
  renderProblems(box, [problem('a', 'Update needed.', 'open_update')], async () => {
    throw new Error('The fix "open_update" is not available yet.');
  });
  click(win, box.querySelector('button'));
  await flush();
  assert.equal(box.querySelector('.error').textContent, 'The fix "open_update" is not available yet.');
});

test('raising replaces a problem and clearing removes it', () => {
  const first = problem('a', 'one');
  const second = problem('b', 'two');
  const raised = applyProblem([first], second);
  assert.deepEqual(raised.map((p) => p.problem_id), ['a', 'b']);
  assert.deepEqual(applyProblem(raised, { ...first, message: 'changed' }).map((p) => p.message), ['two', 'changed']);
  assert.deepEqual(applyProblem(raised, problem('a', 'one', null, '2026-10-21T00:01:00Z')), [second]);
});

test('the tracker draws the initial problems and follows problem notifications', () => {
  const { box } = page();
  const tracker = trackProblems(box, fakeApi(), [problem('a', 'first')]);
  assert.equal(box.children.length, 1);
  tracker.update({ method: 'problem', params: problem('b', 'second') });
  assert.equal(box.children.length, 2);
  tracker.update({ method: 'status', params: {} });
  assert.equal(box.children.length, 2);
  tracker.update({ method: 'problem', params: problem('a', 'first', null, 'x') });
  assert.equal(box.children.length, 1);
  tracker.set([]);
  assert.equal(box.children.length, 0);
});

test('the tracker sends a fix through the api', async () => {
  const { win, box } = page();
  const api = fakeApi();
  trackProblems(box, api, [problem('a', 'Needs access', 'approve_join')]);
  click(win, box.querySelector('button'));
  await flush();
  assert.deepEqual(api.calls, [['fix', 'approve_join']]);
});
