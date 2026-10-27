// The placement-review message: a computer's displays were placed
// automatically and should be checked against the real desk (Story 3 sc. 11).

import { button, el, guarded } from '../dom.js';

// `onChange()` reloads the layout once the owner kept a placement.
export function create({ doc, api, onChange }) {
  const element = el(doc, 'div', { class: 'review' });
  element.hidden = true;
  const errorLine = el(doc, 'p', { class: 'error', role: 'alert' });

  const keep = guarded(errorLine, async (computer) => {
    await api.call('layout.keep_placement', { computer });
    await onChange();
  });

  return {
    element,
    // Draw a message for each computer with a display that `layout` flags.
    show(layout) {
      const flagged = new Map();
      for (const display of layout.displays) {
        if (display.needs_review) flagged.set(display.display_id.computer, display.computer_name);
      }
      element.hidden = flagged.size === 0;
      element.replaceChildren(
        ...[...flagged].map(([computer, name]) =>
          el(
            doc,
            'div',
            { class: 'review-message' },
            el(
              doc,
              'p',
              {},
              el(doc, 'strong', {}, name),
              "'s displays were placed automatically. Check that they match your real desk, then keep the placement or move them.",
            ),
            button(doc, 'Keep this placement', () => keep(computer)),
          ),
        ),
        errorLine,
      );
    },
  };
}
