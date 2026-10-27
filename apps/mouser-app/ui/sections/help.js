// Help: this computer's Mouser version and the troubleshooting guide shipped with it (Story 3 sc. 17).

import { el } from '../dom.js';
import { renderMarkdown } from '../markdown.js';

// Fill `root` with the version line and the guide's Markdown.
export function renderHelp(doc, root, markdown, version) {
  root.replaceChildren(el(doc, 'p', { class: 'version' }, `Mouser ${version}`), renderMarkdown(doc, markdown));
}

export function create({ doc, fetch = (url) => doc.defaultView.fetch(url) }) {
  const element = el(doc, 'div', { class: 'help' });
  const state = { version: null, guide: null, failure: null };

  function draw() {
    if (state.version === null || (state.guide === null && state.failure === null)) return;
    renderHelp(doc, element, state.guide ?? '', state.version);
    if (state.failure) element.append(el(doc, 'p', { class: 'error' }, state.failure));
  }
  fetch('help/troubleshooting.md').then(async (response) => {
    if (response.ok) state.guide = await response.text();
    else state.failure = `The troubleshooting guide could not be loaded (${response.status}).`;
    draw();
  });

  return {
    element,
    update(status) {
      state.version = status.version;
      draw();
    },
  };
}
