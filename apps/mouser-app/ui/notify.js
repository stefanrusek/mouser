// Problem banners: the plain-language message and at most one fix button.

import { button, el, errorText } from './dom.js';

const FIX_LABELS = {
  open_system_settings: 'Open settings',
  approve_join: 'Review the request',
  open_update: 'Update now',
  cross_anyway_hint: 'Show me how',
  open_arranger_edge: 'Change this edge',
};

// "open_system_settings:accessibility" -> "Open settings".
export function fixLabel(fix) {
  return FIX_LABELS[fix.split(':')[0]];
}

export function renderProblem(doc, problem, onFix) {
  const node = el(doc, 'div', { class: 'problem', role: 'alert' }, el(doc, 'p', {}, problem.message));
  if (problem.fix) {
    const failure = el(doc, 'p', { class: 'error' });
    node.append(
      button(doc, fixLabel(problem.fix), async () => {
        failure.textContent = '';
        try {
          await onFix(problem.fix);
        } catch (error) {
          failure.textContent = errorText(error);
        }
      }),
      failure,
    );
  }
  return node;
}

export function renderProblems(container, problems, onFix) {
  const doc = container.ownerDocument;
  container.replaceChildren(...problems.map((problem) => renderProblem(doc, problem, onFix)));
}

// The open problems after `problem` was raised or cleared.
export function applyProblem(problems, problem) {
  const others = problems.filter((p) => p.problem_id !== problem.problem_id);
  return problem.cleared_at ? others : [...others, problem];
}

// Keep `container` showing the open problems: start from `initial`, then feed notifications to `update`.
export function trackProblems(container, api, initial) {
  let problems = initial;
  const draw = () => renderProblems(container, problems, (fix) => api.fix(fix));
  draw();
  return {
    set(next) {
      problems = next;
      draw();
    },
    update(note) {
      if (note.method === 'problem') {
        problems = applyProblem(problems, note.params);
        draw();
      }
    },
  };
}
