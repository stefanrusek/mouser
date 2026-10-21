// Small DOM helpers shared by every window.

export function el(doc, tag, attrs = {}, ...children) {
  const node = doc.createElement(tag);
  for (const [name, value] of Object.entries(attrs)) {
    node.setAttribute(name, value);
  }
  node.append(...children);
  return node;
}

export function button(doc, label, onClick, attrs = {}) {
  const node = el(doc, 'button', { type: 'button', ...attrs }, label);
  node.addEventListener('click', onClick);
  return node;
}

// The text to show for a failed call: control-API errors carry a plain-language message.
export function errorText(error) {
  return typeof error?.message === 'string' ? error.message : String(error);
}

// Wrap an async action so a failure is shown in `target` instead of being lost.
// `clear: false` leaves an earlier message alone (for background refreshes).
export function guarded(target, action, { clear = true } = {}) {
  return async (...args) => {
    try {
      if (clear) target.textContent = '';
      await action(...args);
    } catch (error) {
      target.textContent = errorText(error);
    }
  };
}

export function show(node, visible) {
  node.hidden = !visible;
}
