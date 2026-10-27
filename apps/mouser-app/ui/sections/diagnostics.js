// Diagnostics: the logging switch, the log's location and collecting the
// desk's logs into one bundle (Story 3 sc. 12; contracts/log-bundle.md).

import { button, el, guarded, show } from '../dom.js';

const NOT_INCLUDED = { offline: 'offline', logging_off: 'logging is off', older_version: 'older version' };

export function create({ doc, api }) {
  const logging = el(doc, 'input', { type: 'checkbox', 'data-setting': 'logging' });
  const openLog = button(doc, 'Open log location', () => reveal());
  const collect = button(doc, 'Collect logs from the desk', () => collectLogs());
  const collected = el(doc, 'ul', { class: 'collected' });
  const bundlePath = el(doc, 'p', { class: 'bundle-path hint' });
  const errorLine = el(doc, 'p', { class: 'error', role: 'alert' });
  const element = el(
    doc,
    'div',
    { class: 'diagnostics' },
    el(
      doc,
      'section',
      {},
      el(doc, 'h2', {}, 'Diagnostic logging'),
      el(doc, 'label', {}, logging, ' Write a diagnostic log'),
      el(doc, 'p', { class: 'hint' }, 'Turn this on when asked to help find a problem. It records what Mouser does, never what you type.'),
      openLog,
    ),
    el(
      doc,
      'section',
      {},
      el(doc, 'h2', {}, "The desk's logs"),
      el(doc, 'p', { class: 'hint' }, 'Gathers the log of every computer on your desk into one file on this computer. Nothing is sent anywhere.'),
      collect,
      collected,
      bundlePath,
    ),
    errorLine,
  );
  const state = { logPath: '' };
  show(openLog, false);

  const reveal = guarded(errorLine, () => api.revealInFileManager(state.logPath));
  const collectLogs = guarded(errorLine, async () => {
    const bundle = await api.collectLogs();
    collected.replaceChildren(
      ...bundle.computers.map((c) =>
        el(doc, 'li', {}, c.included ? `${c.name}: included` : `${c.name}: not included (${NOT_INCLUDED[c.reason]})`),
      ),
    );
    bundlePath.textContent = `Saved to ${bundle.path}`;
    await api.revealInFileManager(bundle.path);
  });
  logging.addEventListener(
    'change',
    guarded(errorLine, async () => {
      const { path } = await api.call('logging.set', { enabled: logging.checked });
      state.logPath = path;
      show(openLog, logging.checked);
    }),
  );

  return {
    element,
    update(status) {
      logging.checked = status.logging.enabled;
      state.logPath = status.logging.path;
      show(openLog, status.logging.enabled);
    },
    open(part) {
      element.querySelector(`[data-setting="${part}"]`).focus();
    },
  };
}
