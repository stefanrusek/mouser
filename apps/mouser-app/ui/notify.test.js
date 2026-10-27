import assert from 'node:assert/strict';
import test from 'node:test';
import { Window } from 'happy-dom';
import { problemKey, renderBanner, uniqueProblems } from './notify.js';
import { click, fakeApi, flush } from './test_helpers.js';
import { ME, STUDIO, WINDOWS, problem } from './status_fixture.js';

const doc = () => new Window().document;

test('a banner is keyed by the problem id and the computer', () => {
  assert.notEqual(problemKey(problem('x', 'm', null, STUDIO)), problemKey(problem('x', 'm', null, WINDOWS)));
  assert.equal(problemKey(problem('x', 'a', null, STUDIO)), problemKey(problem('x', 'b', null, STUDIO)));
});

test('each (problem_id, computer) appears once, the first one, in order', () => {
  const list = [
    problem('x', 'first', null, STUDIO),
    problem('x', 'on another computer', null, WINDOWS),
    problem('x', 'again', null, STUDIO),
    problem('y', 'other', null, STUDIO),
  ];
  assert.deepEqual(uniqueProblems(list).map((p) => p.message), ['first', 'on another computer', 'other']);
});

test('a banner carries the computer\'s name, the message and at most one fix', async () => {
  const d = doc();
  const api = fakeApi({ 'layout.keep_placement': {} });
  const banner = renderBanner(d, api, problem('x', 'Check the placement.', `keep_placement:${STUDIO}`, STUDIO), 'Studio Mac', () => {});
  assert.equal(banner.getAttribute('data-computer'), STUDIO);
  assert.equal(banner.querySelector('.where').textContent, 'Studio Mac');
  assert.equal(banner.querySelector('.message').textContent, 'Check the placement.');
  assert.equal(banner.querySelectorAll('button').length, 1);
  click(d.defaultView, banner.querySelector('button'));
  await flush();
  assert.deepEqual(api.calls, [['layout.keep_placement', { computer: STUDIO }]]);
  const plain = renderBanner(d, api, problem('y', 'Nothing to do.', null, ME), 'Desk Mac', () => {});
  assert.equal(plain.querySelectorAll('button').length, 0);
});
