import assert from 'node:assert/strict';
import test from 'node:test';
import { Window } from 'happy-dom';
import { create } from './displays_review.js';
import { click, fakeApi, flush, labelled } from '../test_helpers.js';
import { LINUX, STUDIO, makeLayout } from '../status_fixture.js';

function start(answers = {}) {
  const win = new Window({ url: 'http://localhost/index.html' });
  const api = fakeApi({ 'layout.keep_placement': {}, ...answers });
  const reloaded = [];
  const review = create({ doc: win.document, api, onChange: async () => reloaded.push(true) });
  win.document.body.append(review.element);
  return { win, api, review, reloaded, root: review.element };
}

test('a computer whose displays need review gets a message naming it, saying they were placed automatically', () => {
  const { review, root } = start();
  review.show(makeLayout({ review: [STUDIO] }));
  assert.equal(root.hidden, false);
  const messages = [...root.querySelectorAll('.review-message')];
  assert.equal(messages.length, 1);
  assert.equal(
    messages[0].querySelector('p').textContent,
    "Studio Mac's displays were placed automatically. Check that they match your real desk, then keep the placement or move them.",
  );
  assert.deepEqual([...messages[0].querySelectorAll('button')].map((b) => b.textContent), ['Keep this placement']);
});

test('a computer with several flagged displays gets one message, and each flagged computer its own', () => {
  const { review, root } = start();
  const layout = makeLayout({ review: [STUDIO, LINUX] });
  layout.displays.push({ ...layout.displays[1], display_id: { computer: STUDIO, key: '2' }, name: 'Second' });
  review.show(layout);
  assert.deepEqual(
    [...root.querySelectorAll('.review-message strong')].map((n) => n.textContent),
    ['Studio Mac', 'Linux Box'],
  );
});

test('"Keep this placement" calls layout.keep_placement for that computer and the message goes away', async () => {
  const { win, api, review, reloaded, root } = start();
  const layout = makeLayout({ review: [STUDIO] });
  review.show(layout);
  click(win, labelled(root, 'Keep this placement'));
  await flush();
  assert.deepEqual(api.calls.find(([m]) => m === 'layout.keep_placement'), ['layout.keep_placement', { computer: STUDIO }]);
  assert.equal(reloaded.length, 1, 'the arranger reloads, and the engine no longer flags the displays');
  review.show(makeLayout());
  assert.equal(root.hidden, true);
  assert.equal(root.querySelectorAll('.review-message').length, 0);
});

test('a placement that was moved is no longer flagged, so its message goes away', () => {
  const { review, root } = start();
  review.show(makeLayout({ review: [STUDIO] }));
  review.show(makeLayout());
  assert.equal(root.hidden, true);
});

test('a refused keep is shown', async () => {
  const { win, review, root } = start({ 'layout.keep_placement': new Error('Not on a desk.') });
  review.show(makeLayout({ review: [STUDIO] }));
  click(win, labelled(root, 'Keep this placement'));
  await flush();
  assert.equal(root.querySelector('.error').textContent, 'Not on a desk.');
});
