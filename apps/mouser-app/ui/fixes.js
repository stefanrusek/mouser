// A problem's fix, as one action.

import { button, el, errorText } from './dom.js';

const LABELS = {
  open_system_settings: 'Open settings',
  approve_join: 'Review the request',
  open_update: 'Update now',
  cross_anyway_hint: 'Show me how',
  open_arranger_edge: 'Change this edge',
  restart_elevated: 'Restart Mouser as administrator',
  keep_placement: 'Keep this placement',
  update_computer: 'Update this computer',
};

// "open_system_settings:accessibility" -> "Open settings".
export const fixLabel = (fix) => LABELS[fix.split(':')[0]];

// Do what `problem.fix` says. `navigate(route)` moves the window to a section.
// `computerName` is the computer the problem concerns; a restart as administrator
// answers with what to do next (its Windows prompt is approved at that computer).
export async function runFix(api, problem, navigate, computerName) {
  const [kind, argument] = problem.fix.split(':');
  switch (kind) {
    case 'open_system_settings':
      return api.openPermissionSettings(argument);
    case 'keep_placement':
      return api.call('layout.keep_placement', { computer: argument });
    case 'update_computer':
      return api.call('update.apply', { computer: argument });
    case 'restart_elevated':
      // The engine restarts this computer itself and asks a peer to do the same.
      await api.call('elevation.restart', { computer: argument });
      return `Approve the Windows prompt on ${computerName}`;
    case 'approve_join':
      return navigate('#desk');
    default:
      // The app itself runs the rest (it opens windows and acknowledges the notice).
      return api.fix(problem.problem_id, problem.fix);
  }
}

// The action for `problem`'s fix, with the failure shown beside it; null when it has none.
export function renderFix(doc, api, problem, navigate, computerName) {
  if (!problem.fix) return null;
  const failure = el(doc, 'p', { class: 'error' });
  const note = el(doc, 'p', { class: 'note' });
  return el(
    doc,
    'div',
    { class: 'fix' },
    button(doc, fixLabel(problem.fix), async () => {
      failure.textContent = '';
      note.textContent = '';
      try {
        note.textContent = (await runFix(api, problem, navigate, computerName)) ?? '';
      } catch (error) {
        failure.textContent = errorText(error);
      }
    }),
    note,
    failure,
  );
}
