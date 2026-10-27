// Displays: the arranger with full-name labels, the placement-review message
// and the desk-wide display options (Story 3 sc. 10-11).

import { init as startArranger } from '../arranger.js';
import { el } from '../dom.js';
import { create as createReview } from './displays_review.js';

// The arranger's markup; its ids are what `arranger.js` reads.
export function arrangerMarkup(doc) {
  const root = el(doc, 'div', { class: 'arranger' });
  root.innerHTML = `
    <p class="lead">Drag each display to match where it sits on your desk. Displays snap edge to edge.</p>
    <section>
      <div id="canvas" class="canvas" hidden></div>
      <p id="empty" class="hint" hidden>No displays to arrange yet. Join a desk first.</p>
      <p id="refusal" class="error" role="alert"></p>
      <div id="undo-bar" class="undo-bar" hidden>
        <span id="undo-text"></span>
        <button id="undo" type="button">Undo</button>
      </div>
    </section>
    <section id="details" class="details" aria-live="polite"></section>
    <section id="edge-options" class="details" hidden>
      <h3 id="edge-title"></h3>
      <label class="check"><input id="block-corners" data-setting="block_corners" type="checkbox"> Block crossing in corners</label>
      <p class="hint">The pointer stays put when it reaches the edge in a screen corner. Push the same edge twice in a row to cross anyway.</p>
    </section>
    <section>
      <label class="check"><input id="block-fullscreen" data-setting="block_fullscreen" type="checkbox"> Don't cross while an app is full screen</label>
      <p class="hint">Applies to the whole desk. Handy for games and video.</p>
    </section>
    <section>
      <label class="check"><input id="equalize" data-setting="equalize_dimensions" type="checkbox"> Equalize display dimensions</label>
      <p class="hint">When displays are different sizes, the pointer lines up with the same spot on the display it enters instead of the same distance from the corner.</p>
    </section>
    <p id="error" class="error" role="alert"></p>`;
  return root;
}

export function create({ doc, api }) {
  const markup = arrangerMarkup(doc);
  const state = { arranger: null };
  const review = createReview({ doc, api, onChange: () => state.arranger.tick() });
  const element = el(doc, 'div', { class: 'displays' }, review.element, markup);
  // `tick` runs once the arranger has its first layout, which may be after the section is built.
  const started = startArranger({ doc, api, root: markup, onLayout: (layout) => review.show(layout) }).then((arranger) => {
    state.arranger = arranger;
  });

  return {
    element,
    update: () => started.then(() => state.arranger.tick()),
    open(part) {
      markup.querySelector(`[data-setting="${part}"]`).focus();
    },
  };
}
